use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use lib_subprocess::drain_pipe_unlimited;

const POLL_INTERVAL_MS: u64 = 100;
pub(crate) const JJ_CMD_TIMEOUT_SECS: u64 = 30;

/// [`run_cmd_capture`] の結果。stdout / stderr を分離して保持する。
///
/// stdout を機械可読出力 (JSON 等) としてパースする呼び出しは本構造体を使い、
/// stderr の警告ログ混入でパースが壊れる事故 (PR #238 実観測) を構造的に防ぐ。
pub(crate) struct CmdCapture {
    pub(crate) ok: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
}

/// 引数を配列で直接渡し、stdout / stderr を分離キャプチャして返す。
pub(crate) fn run_cmd_capture(
    program: &str,
    fixed_args: &[&str],
    extra_args: &[String],
    timeout_secs: u64,
) -> CmdCapture {
    let mut child = match Command::new(program)
        .args(fixed_args)
        .args(extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return CmdCapture {
                ok: false,
                stdout: String::new(),
                stderr: format!("Failed to execute {} {:?}: {}", program, fixed_args, e),
                timed_out: false,
            }
        }
    };

    let stdout_handle = drain_pipe_unlimited(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().unwrap());

    let timed_out = wait_child_with_deadline(&mut child, timeout_secs);

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    if timed_out {
        return CmdCapture {
            ok: false,
            stdout,
            stderr,
            timed_out: true,
        };
    }

    let code = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    CmdCapture {
        ok: code == 0,
        stdout,
        stderr,
        timed_out: false,
    }
}

/// 子プロセスを deadline 付きで待機する。timeout 到達時は kill して `true` を返す。
/// try_wait の失敗も timeout 扱い (fail-safe 方向) に倒す。
fn wait_child_with_deadline(child: &mut std::process::Child, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(_) => break true,
        }
    }
}

/// 引数を配列で直接渡す版（スペースを含む引数を正しくハンドリング）。
///
/// stdout / stderr を結合した文字列を返す従来 API。機械可読出力をパースする
/// 用途には [`run_cmd_capture`] を使うこと (stderr 混入でパースが壊れるため)。
pub(crate) fn run_cmd_direct(
    program: &str,
    fixed_args: &[&str],
    extra_args: &[String],
    timeout_secs: u64,
) -> (bool, String) {
    let cap = run_cmd_capture(program, fixed_args, extra_args, timeout_secs);
    let combined = format!("{}{}", cap.stdout, cap.stderr).trim().to_string();

    if cap.timed_out {
        return (
            false,
            format!("{}\n(timeout after {}s)", combined, timeout_secs),
        );
    }
    (cap.ok, combined)
}

/// gh コマンドを静かに実行 (stderr 抑制)
pub(crate) fn run_gh_quiet(args: &[&str]) -> Option<String> {
    let output = Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

// ─── jj CLI ヘルパー ───

/// 現在の working copy (`@`) の commit id を取得する。
/// 失敗時は `None` を返し、呼び出し側で fail-safe に扱う。
pub(crate) fn capture_commit_id() -> Option<String> {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
        &[],
        10,
    );
    if !ok {
        crate::log::log_info(&format!("[state] capture_commit_id 失敗: {}", out.trim()));
        return None;
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// `from` と `to` の間の diff が空か判定する。
/// jj コマンドが失敗した場合は `true` (空扱い = NoChange = push しない) を返す。
/// capture_commit_id と同じ fail-closed 方向に揃えることで誤 push を防ぐ。
pub(crate) fn diff_is_empty(from: &str, to: &str) -> bool {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["diff", "--from", from, "--to", to, "--stat"],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        crate::log::log_info(&format!(
            "[state] diff_is_empty 判定失敗 (空として扱い push をスキップ): {}",
            out.trim()
        ));
        return true;
    }
    out.trim().is_empty()
}

/// 現在の `@` が empty commit (親との差分なし) か判定する。
///
/// `diff_is_empty` は re-push 判定 (fail-safe で「空扱い → push しない」方向) だが、
/// こちらは **abandon 判定**用であり方向が逆: 失敗時は `false` (= diff あり扱い)
/// を返して abandon を見送る。
///
/// 理由: jj コマンド失敗時にうっかり `jj abandon` を走らせると、takt が部分的に
/// amend した child commit ごと消えるリスクがある。「判定不能なら何もしない」方向
/// に倒す。
///
/// 実装: `jj diff --stat` は空 commit でも "0 files changed, ..." のような
/// サマリ行を出力するため空判定に使えない。代わりに jj の `empty` テンプレート
/// keyword を使い、"true"/"false" の明示出力で判定する。
pub(crate) fn diff_at_is_empty() -> bool {
    let (ok, out) = run_cmd_direct(
        "jj",
        &[
            "log",
            "-r",
            "@",
            "--no-graph",
            "-T",
            "if(empty, \"true\", \"false\")",
        ],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        crate::log::log_info(&format!(
            "[state] diff_at_is_empty 判定失敗 (diff あり扱いで abandon をスキップ): {}",
            out.trim()
        ));
        return false;
    }
    out.trim() == "true"
}

/// takt ワークフロー実行のデフォルトタイムアウト (10 分)
const TAKT_TIMEOUT_SECS: u64 = 600;

/// stdio を継承してコマンドを実行する (takt 呼び出し用、タイムアウト付き)
pub(crate) fn run_cmd_inherit(label: &str, program: &str, args: &[&str]) -> bool {
    crate::log::log_info(&format!("{}: {} {}", label, program, args.join(" ")));
    let mut child = match Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            crate::log::log_info(&format!("{} の起動に失敗: {}", label, e));
            return false;
        }
    };

    let deadline = std::time::Instant::now() + Duration::from_secs(TAKT_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    crate::log::log_info(&format!(
                        "{} タイムアウト ({}秒)",
                        label, TAKT_TIMEOUT_SECS
                    ));
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => {
                crate::log::log_info(&format!("{} の待機に失敗: {}", label, e));
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// check-ci-coderabbit 実行ファイルのパスを解決する (cli-pr-monitor と同 dir 前提)。
///
/// 実行ファイル拡張子は OS 依存 (Windows: `.exe` / それ以外: なし) のため
/// `std::env::consts::EXE_SUFFIX` で解決する (WP-13: EXE_SUFFIX 抽象化)。
pub(crate) fn checker_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("check-ci-coderabbit{}", std::env::consts::EXE_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PR #238 regression: stderr の警告ログが stdout の機械可読出力に
    /// 混入しないことを分離キャプチャで保証する。
    #[test]
    #[cfg(windows)]
    fn run_cmd_capture_separates_stdout_and_stderr() {
        let cap = run_cmd_capture("cmd", &["/C", "echo OUT& echo ERR 1>&2"], &[], 30);
        assert!(cap.ok, "stderr: {}", cap.stderr);
        assert!(!cap.timed_out);
        assert!(cap.stdout.contains("OUT"), "stdout: {:?}", cap.stdout);
        assert!(!cap.stdout.contains("ERR"), "stdout: {:?}", cap.stdout);
        assert!(cap.stderr.contains("ERR"), "stderr: {:?}", cap.stderr);
    }

    #[test]
    fn run_cmd_capture_spawn_failure_reports_via_stderr_field() {
        let cap = run_cmd_capture("no-such-program-gitdir-251", &[], &[], 5);
        assert!(!cap.ok);
        assert!(!cap.timed_out);
        assert!(cap.stdout.is_empty());
        assert!(cap.stderr.contains("Failed to execute"));
    }

    #[test]
    #[cfg(windows)]
    fn run_cmd_direct_keeps_combined_output_compatibility() {
        let (ok, combined) = run_cmd_direct("cmd", &["/C", "echo OUT& echo ERR 1>&2"], &[], 30);
        assert!(ok);
        assert!(combined.contains("OUT"));
        assert!(combined.contains("ERR"));
    }
}

