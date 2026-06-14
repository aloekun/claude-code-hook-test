use std::process::Command;

use lib_subprocess::run_cmd_shell_capped;

use crate::log::log_info;

pub(crate) const MAX_LINES: usize = 40;

/// コマンドを実行し、成功時は出力を `Ok`、失敗時はエラー出力を `Err` で返す。
pub(crate) fn run_stage_cmd(label: &str, cmd: &str, timeout: u64) -> Result<String, String> {
    let (success, output) = run_cmd_shell_capped(label, cmd, timeout, MAX_LINES);
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
