use std::process::Command;

use lib_subprocess::{combine_output, drain_pipe_capped, wait_with_timeout_basic};

use crate::log::log_info;

pub(crate) const MAX_LINES: usize = 40;

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

    let stdout_handle = drain_pipe_capped(child.stdout.take().expect("stdout must be piped"), MAX_LINES);
    let stderr_handle = drain_pipe_capped(child.stderr.take().expect("stderr must be piped"), MAX_LINES);

    let exit_status = match wait_with_timeout_basic(label, &mut child, timeout_secs) {
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
    fn run_stage_cmd_returns_ok_on_success() {
        let result = run_stage_cmd("test", "echo hello", 10);
        assert!(result.is_ok(), "successful command should return Ok");
    }

    #[test]
    fn run_stage_cmd_returns_err_on_failure() {
        let result = run_stage_cmd("test", "exit 1", 10);
        assert!(result.is_err(), "failed command should return Err");
    }

}
