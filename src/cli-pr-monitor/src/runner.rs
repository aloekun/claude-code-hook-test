use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// NOTE: push-pipeline 版は MAX_LINES=40 でログ表示用に切り詰めるが、
// こちらは check-ci-coderabbit の JSON 出力全体をパースするため制限なし。

pub(crate) fn drain_pipe(
    pipe: impl std::io::Read + Send + 'static,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::Read;
        let mut output = String::new();
        let mut reader = std::io::BufReader::new(pipe);
        let _ = reader.read_to_string(&mut output);
        output.trim_end().to_string()
    })
}

/// 引数を配列で直接渡す版（スペースを含む引数を正しくハンドリング）
pub(crate) fn run_cmd_direct(
    program: &str,
    fixed_args: &[&str],
    extra_args: &[String],
    timeout_secs: u64,
) -> (bool, String) {
    let mut child = match Command::new(program)
        .args(fixed_args)
        .args(extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                false,
                format!("Failed to execute {} {:?}: {}", program, fixed_args, e),
            )
        }
    };

    let stdout_handle = drain_pipe(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe(child.stderr.take().unwrap());

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break true,
        }
    };

    let stdout_text = stdout_handle.join().unwrap_or_default();
    let stderr_text = stderr_handle.join().unwrap_or_default();
    let combined = format!("{}{}", stdout_text, stderr_text).trim().to_string();

    if timed_out {
        return (
            false,
            format!("{}\n(timeout after {}s)", combined, timeout_secs),
        );
    }

    let code = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    (code == 0, combined)
}

#[allow(dead_code)]
pub(crate) fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    }
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
    let (ok, out) =
        run_cmd_direct("jj", &["diff", "--from", from, "--to", to, "--stat"], &[], 30);
    if !ok {
        crate::log::log_info(&format!(
            "[state] diff_is_empty 判定失敗 (空として扱い push をスキップ): {}",
            out.trim()
        ));
        return true;
    }
    out.trim().is_empty()
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

pub(crate) fn checker_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("check-ci-coderabbit.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_output_both() {
        assert_eq!(combine_output("a", "b"), "a\nb");
    }

    #[test]
    fn combine_output_stdout_only() {
        assert_eq!(combine_output("a", ""), "a");
    }

    #[test]
    fn combine_output_stderr_only() {
        assert_eq!(combine_output("", "b"), "b");
    }

    #[test]
    fn combine_output_empty() {
        assert_eq!(combine_output("", ""), "");
    }
}
