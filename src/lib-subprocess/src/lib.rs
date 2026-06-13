//! Subprocess utility helpers shared across CLI / hook crates.
//!
//! 順位 173a (todo11.md): combine_output extract — 5 crate horizontal duplication 解消の最初の sub-PR。
//! 順位 173b: wait_with_timeout を polling 系 2 variant (`_safe` / `_basic`) として抽出。
//! ADR-026 Cargo workspace + ADR-012 lib-* naming に整合。
//!
//! 後続 sub-PR (173c/d) で `drain_pipe` / `run_cmd` を variant 単位で抽出予定。
//! `cli-pr-monitor/src/classifier_runner.rs` の channel-based wait_with_timeout は
//! signature と設計意図 (pipe buffer overflow 対策) が異なるため本 lib では別 variant
//! として扱わず、必要なら 173e で評価する。

use std::process::ExitStatus;
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
}
