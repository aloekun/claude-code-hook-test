//! Subprocess utility helpers shared across CLI / hook crates.
//!
//! 順位 173a (todo11.md): combine_output extract — 5 crate horizontal duplication 解消の最初の sub-PR。
//! 順位 173b: wait_with_timeout を polling 系 2 variant (`_safe` / `_basic`) として抽出。
//! 順位 173c: drain_pipe を 3 variant (`_unlimited` / `_capped` / `_capped_reporting`) として抽出。
//! 順位 173d: run_cmd_shell を 2 variant (`_capped` / `_capped_reporting`) として抽出。
//! ADR-026 Cargo workspace + ADR-012 lib-* naming に整合。
//!
//! 順位 173e で variant merge を検討予定。
//! `cli-pr-monitor/src/classifier_runner.rs` の channel-based wait_with_timeout と
//! `cli-pr-monitor/src/runner.rs` の `run_cmd_direct` (direct args / variant B) は
//! signature と設計意図が異なるため本 lib では別 variant として扱わず、必要なら 173e
//! で評価する。

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// stdout と stderr を結合する。
///
/// 挙動:
/// - どちらか片方が空ならもう片方をそのまま返す
/// - 両方非空で stdout が改行で終わっている場合は separator を挿入しない (二重改行回避)
/// - 両方非空で stdout が改行で終わっていない場合は `\n` で連結
///
/// この `\n` suffix 吸収版は元 `hooks-post-tool-linter` で採用されていた頑健版。
/// 4 crate (cli-*) の basic 版とは `stdout.ends_with('\n')` のときに挙動が分岐するが
/// (basic="out\n\nerr" / robust="out\nerr")、production の呼び出し側はすべて
/// `drain_pipe` 経由で trailing newline を除去済の文字列を渡すため顕在化しない。
/// 既存 4 crate test も `\n` suffix case を含まないため全 case pass。
pub fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.ends_with('\n') {
        format!("{}{}", stdout, stderr)
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

const POLL_INTERVAL_MS: u64 = 100;

/// 子プロセスの終了を timeout 付きで待機する。Err 経路 (try_wait 失敗) で
/// child を kill + wait してから Err を返す **safe** variant。
///
/// 戻り値:
/// - `Ok(Some(status))`: 正常終了
/// - `Ok(None)`: timeout (child は kill + wait 済)
/// - `Err(msg)`: try_wait 失敗 (child は kill + wait 済)
///
/// timeout / try_wait エラーの両経路で `child.kill()` + `child.wait()` を実施し、
/// 子プロセスと呼び出し側 reader スレッドが zombie 化するのを防ぐ。
/// subprocess lifecycle の strictness を要求する callsite で使用する。
pub fn wait_with_timeout_safe(
    label: &str,
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> Result<Option<ExitStatus>, String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed to wait for {}: {}", label, e));
            }
        }
    }
}

/// 子プロセスの終了を timeout 付きで待機する。Err 経路 (try_wait 失敗) で
/// child を kill せずそのまま Err を返す **basic** variant。
///
/// 戻り値:
/// - `Ok(Some(status))`: 正常終了
/// - `Ok(None)`: timeout (child は kill + wait 済)
/// - `Err(msg)`: try_wait 失敗 (child は kill されない、呼び出し側で扱う)
///
/// timeout 経路のみ `child.kill()` + `child.wait()` を実施する。
/// try_wait 失敗時の child cleanup は呼び出し側が制御する設計の callsite で使用する。
/// `_safe` との 2 variant 並立は 173b 時点での保存対応で、merge 可否は 173e で判断する。
pub fn wait_with_timeout_basic(
    label: &str,
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> Result<Option<ExitStatus>, String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(e) => return Err(format!("Failed to wait for {}: {}", label, e)),
        }
    }
}

/// 子プロセスの stdout / stderr パイプを別スレッドで全量読込し、行末空白を trim した
/// 文字列を返す **unlimited** variant。
///
/// `read_to_string` で全データを単一バッファに読み込むため出力サイズ制限なし。
/// JSON 等の構造化出力を pipe 経由で完全パースする callsite (例: check-ci-coderabbit
/// の JSON 受け取り) で使用する。出力過大時にメモリ消費が線形成長する点に注意。
pub fn drain_pipe_unlimited(
    pipe: impl Read + Send + 'static,
) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut output = String::new();
        let mut reader = BufReader::new(pipe);
        let _ = reader.read_to_string(&mut output);
        output.trim_end().to_string()
    })
}

/// 子プロセスの stdout / stderr パイプを別スレッドで最大 `max_lines` 行まで読込し、
/// 改行で結合した文字列を返す **capped** variant (silent truncate)。
///
/// `max_lines` 超過分は読み捨て (パイプバッファの排出は継続) されるため、超過行数は
/// 戻り値に反映されない。ログ表示用の callsite で使用する (cli-push-runner /
/// cli-push-pipeline / hooks-stop-quality 等)。
pub fn drain_pipe_capped(
    pipe: impl Read + Send + 'static,
    max_lines: usize,
) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut collected = Vec::with_capacity(max_lines);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < max_lines {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    }
                }
                Err(_) => break,
            }
        }
        collected.join("\n")
    })
}

/// 子プロセスの stdout / stderr パイプを別スレッドで最大 `max_lines` 行まで読込し、
/// 超過があれば末尾に `"... (N lines truncated)"` を付与する **reporting capped** variant。
///
/// `drain_pipe_capped` と同じく行毎読込 + truncate だが、切り捨て行数を最終出力に反映する。
/// マージパイプライン等で「ログは確認したいが過大化は避けたい」 callsite で使用する
/// (cli-merge-pipeline、MAX_LINES=200)。
pub fn drain_pipe_capped_reporting(
    pipe: impl Read + Send + 'static,
    max_lines: usize,
) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut collected = Vec::with_capacity(max_lines);
        let mut buf = Vec::new();
        let mut truncated = 0usize;
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < max_lines {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    } else {
                        truncated += 1;
                    }
                }
                Err(_) => break,
            }
        }
        if truncated > 0 {
            collected.push(format!("... ({} lines truncated)", truncated));
        }
        collected.join("\n")
    })
}

/// Kills `child`, waits for it to be reaped, joins `stdout_handle` and `stderr_handle`,
/// then returns `(false, error)`.
///
/// Joining threads without first killing the child would block indefinitely if the child is
/// still running (the reader threads only break on pipe EOF, which arrives when the child
/// exits). Always kill before joining to avoid that deadlock.
fn kill_and_join_err(
    child: &mut std::process::Child,
    stdout_handle: JoinHandle<String>,
    stderr_handle: JoinHandle<String>,
    error: String,
) -> (bool, String) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();
    (false, error)
}

/// `cmd /c <cmd>` で shell コマンドを実行し timeout 付きで結果を返す **silent capped** variant。
///
/// 戻り値: `(success, combined_output)`。
/// - 起動失敗 / try_wait 失敗 → `(false, error_message)`
/// - timeout → `(false, "timed out after Ns\n<combined>")`
/// - exit 正常 → `(status.success(), combined)`
///
/// 内部で `drain_pipe_capped(max_lines)` を使用するため stdout / stderr は `max_lines` 行で
/// silent truncate。control flow 判定に出力を使う callsite では別途 `drain_pipe_unlimited`
/// を直接組み立てるか、`max_lines` を十分大きく取ること。
///
/// 内部で `wait_with_timeout_basic` を使用 (= Err 経路で child を kill しない basic semantics)。
pub fn run_cmd_shell_capped(
    label: &str,
    cmd: &str,
    timeout_secs: u64,
    max_lines: usize,
) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain_pipe_capped(
        child.stdout.take().expect("stdout must be piped"),
        max_lines,
    );
    let stderr_handle = drain_pipe_capped(
        child.stderr.take().expect("stderr must be piped"),
        max_lines,
    );

    let exit_status = match wait_with_timeout_basic(label, &mut child, timeout_secs) {
        Ok(status) => status,
        Err(e) => return kill_and_join_err(&mut child, stdout_handle, stderr_handle, e),
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = combine_output(&stdout, &stderr);

    match exit_status {
        None => {
            let mut msg = format!("timed out after {}s", timeout_secs);
            if !combined.is_empty() {
                msg = format!("{}\n{}", msg, combined);
            }
            (false, msg)
        }
        Some(status) => (status.success(), combined),
    }
}

/// `cmd /c <cmd>` で shell コマンドを実行し timeout 付きで結果を返す **reporting capped** variant。
///
/// `run_cmd_shell_capped` と同 signature だが内部で `drain_pipe_capped_reporting(max_lines)`
/// を使用、stdout / stderr 超過時は末尾に `"... (N lines truncated)"` が追加される。
/// merge log 等で truncate されたか reviewer に明示したい callsite で使用する
/// (cli-merge-pipeline、MAX_LINES=200)。
pub fn run_cmd_shell_capped_reporting(
    label: &str,
    cmd: &str,
    timeout_secs: u64,
    max_lines: usize,
) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain_pipe_capped_reporting(
        child.stdout.take().expect("stdout must be piped"),
        max_lines,
    );
    let stderr_handle = drain_pipe_capped_reporting(
        child.stderr.take().expect("stderr must be piped"),
        max_lines,
    );

    let exit_status = match wait_with_timeout_basic(label, &mut child, timeout_secs) {
        Ok(status) => status,
        Err(e) => return kill_and_join_err(&mut child, stdout_handle, stderr_handle, e),
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = combine_output(&stdout, &stderr);

    match exit_status {
        None => {
            let mut msg = format!("timed out after {}s", timeout_secs);
            if !combined.is_empty() {
                msg = format!("{}\n{}", msg, combined);
            }
            (false, msg)
        }
        Some(status) => (status.success(), combined),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_output_both_present_inserts_newline() {
        assert_eq!(combine_output("out", "err"), "out\nerr");
    }

    #[test]
    fn combine_output_only_stdout_returns_stdout() {
        assert_eq!(combine_output("out", ""), "out");
    }

    #[test]
    fn combine_output_only_stderr_returns_stderr() {
        assert_eq!(combine_output("", "err"), "err");
    }

    #[test]
    fn combine_output_both_empty_returns_empty() {
        assert_eq!(combine_output("", ""), "");
    }

    #[test]
    fn combine_output_stdout_trailing_newline_does_not_insert_separator() {
        assert_eq!(combine_output("out\n", "err"), "out\nerr");
    }

    use std::process::{Command, Stdio};

    fn spawn_quick_exit() -> std::process::Child {
        Command::new("cmd")
            .args(["/c", "exit 0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn quick-exit cmd")
    }

    fn spawn_long_running() -> std::process::Child {
        Command::new("cmd")
            .args(["/c", "ping 127.0.0.1 -n 10"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn long-running cmd")
    }

    #[test]
    fn wait_with_timeout_safe_returns_exit_status_on_quick_completion() {
        let mut child = spawn_quick_exit();
        let result = wait_with_timeout_safe("test", &mut child, 10).expect("safe wait failed");
        assert!(result.is_some(), "child should have exited cleanly");
        assert!(result.unwrap().success(), "exit 0 should report success");
    }

    #[test]
    fn wait_with_timeout_safe_kills_child_on_timeout() {
        let mut child = spawn_long_running();
        let result = wait_with_timeout_safe("test", &mut child, 1).expect("safe wait failed");
        assert!(result.is_none(), "timeout path should return Ok(None)");
        assert!(
            child.try_wait().expect("try_wait after kill failed").is_some(),
            "child should be reaped after safe-variant timeout",
        );
    }

    #[test]
    fn wait_with_timeout_basic_returns_exit_status_on_quick_completion() {
        let mut child = spawn_quick_exit();
        let result = wait_with_timeout_basic("test", &mut child, 10).expect("basic wait failed");
        assert!(result.is_some(), "child should have exited cleanly");
        assert!(result.unwrap().success(), "exit 0 should report success");
    }

    #[test]
    fn wait_with_timeout_basic_kills_child_on_timeout() {
        let mut child = spawn_long_running();
        let result = wait_with_timeout_basic("test", &mut child, 1).expect("basic wait failed");
        assert!(result.is_none(), "timeout path should return Ok(None)");
        assert!(
            child.try_wait().expect("try_wait after kill failed").is_some(),
            "child should be reaped after basic-variant timeout",
        );
    }

    use std::io::Cursor;

    #[test]
    fn drain_pipe_unlimited_reads_entire_input_and_trims_trailing_whitespace() {
        let input = Cursor::new(b"line1\nline2\nline3\n".to_vec());
        let handle = drain_pipe_unlimited(input);
        assert_eq!(handle.join().unwrap(), "line1\nline2\nline3");
    }

    #[test]
    fn drain_pipe_unlimited_preserves_long_output_without_truncation() {
        let input: String = (0..500).map(|i| format!("line{}\n", i)).collect();
        let expected: String = (0..500)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let handle = drain_pipe_unlimited(Cursor::new(input.into_bytes()));
        assert_eq!(handle.join().unwrap(), expected);
    }

    #[test]
    fn drain_pipe_capped_truncates_silently_at_max_lines() {
        let input = Cursor::new(b"a\nb\nc\nd\ne\n".to_vec());
        let handle = drain_pipe_capped(input, 3);
        assert_eq!(handle.join().unwrap(), "a\nb\nc");
    }

    #[test]
    fn drain_pipe_capped_returns_all_lines_when_under_cap() {
        let input = Cursor::new(b"only\ntwo\n".to_vec());
        let handle = drain_pipe_capped(input, 100);
        assert_eq!(handle.join().unwrap(), "only\ntwo");
    }

    #[test]
    fn drain_pipe_capped_reporting_appends_truncation_summary_when_over_cap() {
        let input = Cursor::new(b"a\nb\nc\nd\ne\n".to_vec());
        let handle = drain_pipe_capped_reporting(input, 3);
        assert_eq!(handle.join().unwrap(), "a\nb\nc\n... (2 lines truncated)");
    }

    #[test]
    fn drain_pipe_capped_reporting_omits_summary_when_within_cap() {
        let input = Cursor::new(b"a\nb\n".to_vec());
        let handle = drain_pipe_capped_reporting(input, 10);
        assert_eq!(handle.join().unwrap(), "a\nb");
    }

    #[test]
    fn run_cmd_shell_capped_returns_true_on_exit_zero() {
        let (ok, _output) = run_cmd_shell_capped("test", "exit 0", 10, 40);
        assert!(ok, "exit 0 should report success");
    }

    #[test]
    fn run_cmd_shell_capped_returns_false_on_exit_nonzero() {
        let (ok, _output) = run_cmd_shell_capped("test", "exit 1", 10, 40);
        assert!(!ok, "exit 1 should report failure");
    }

    #[test]
    fn run_cmd_shell_capped_captures_stdout_within_cap() {
        let (ok, output) = run_cmd_shell_capped("test", "echo hello", 10, 40);
        assert!(ok);
        assert!(
            output.contains("hello"),
            "stdout should be captured: {:?}",
            output,
        );
    }

    #[test]
    fn run_cmd_shell_capped_reports_timeout_with_message() {
        let (ok, output) = run_cmd_shell_capped("test", "ping 127.0.0.1 -n 10", 1, 40);
        assert!(!ok, "timeout should report failure");
        assert!(
            output.starts_with("timed out after 1s"),
            "timeout message expected: {:?}",
            output,
        );
    }

    #[test]
    fn run_cmd_shell_capped_reporting_returns_true_on_exit_zero() {
        let (ok, _output) = run_cmd_shell_capped_reporting("test", "exit 0", 10, 40);
        assert!(ok, "exit 0 should report success");
    }

    #[test]
    fn run_cmd_shell_capped_reporting_reports_timeout_with_message() {
        let (ok, output) =
            run_cmd_shell_capped_reporting("test", "ping 127.0.0.1 -n 10", 1, 40);
        assert!(!ok, "timeout should report failure");
        assert!(
            output.starts_with("timed out after 1s"),
            "timeout message expected: {:?}",
            output,
        );
    }
}
