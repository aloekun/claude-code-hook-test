use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use crate::log::log_info;

const MAX_LINES: usize = 40;

pub(crate) fn drain_pipe(
    pipe: impl std::io::Read + Send + 'static,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut collected = Vec::with_capacity(MAX_LINES);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < MAX_LINES {
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

pub(crate) fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

/// タイムアウト付きで子プロセスの終了を待つ。
/// `None` はタイムアウトを意味する（プロセスは kill 済み）。
pub(crate) fn wait_with_timeout(
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
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Failed to wait for {}: {}", label, e)),
        }
    }
}

pub(crate) fn run_cmd(label: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain_pipe(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = drain_pipe(child.stderr.take().expect("stderr must be piped"));

    let exit_status = match wait_with_timeout(label, &mut child, timeout_secs) {
        Ok(status) => status,
        Err(e) => return (false, e),
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

/// コマンドを実行し、成功時は出力を `Ok`、失敗時はエラー出力を `Err` で返す。
pub(crate) fn run_stage_cmd(label: &str, cmd: &str, timeout: u64) -> Result<String, String> {
    let (success, output) = run_cmd(label, cmd, timeout);
    if success {
        Ok(output)
    } else {
        Err(output)
    }
}

pub(crate) fn run_cmd_inherit(label: &str, program: &str, args: &[&str]) -> bool {
    log_info(&format!("{}: {} {}", label, program, args.join(" ")));
    match Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
    {
        Ok(status) => status.success(),
        Err(e) => {
            log_info(&format!("{} の起動に失敗: {}", label, e));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_output_both_present() {
        assert_eq!(combine_output("out", "err"), "out\nerr");
    }

    #[test]
    fn combine_output_only_stdout() {
        assert_eq!(combine_output("out", ""), "out");
    }

    #[test]
    fn combine_output_only_stderr() {
        assert_eq!(combine_output("", "err"), "err");
    }

    #[test]
    fn combine_output_both_empty() {
        assert_eq!(combine_output("", ""), "");
    }

    #[test]
    fn run_stage_cmd_returns_ok_on_success() {
        let result = run_stage_cmd("test", "echo hello", 10);
        assert!(result.is_ok(), "successful command should return Ok");
    }

    #[test]
    fn run_stage_cmd_returns_err_on_failure() {
        let result = run_stage_cmd("test", "exit 1", 10);
        assert!(result.is_err(), "failed command should return Err");
    }

    #[test]
    fn wait_with_timeout_returns_exit_status_directly() {
        let mut child = Command::new("cmd")
            .args(["/c", "exit 0"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn test process");

        let result = wait_with_timeout("test", &mut child, 10).expect("wait_with_timeout failed");
        assert!(
            result.is_some(),
            "process should have exited, not timed out"
        );
        assert!(result.unwrap().success(), "exit 0 should be success");
    }
}
