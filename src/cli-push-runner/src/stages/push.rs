use crate::config::{PushConfig, DEFAULT_PUSH_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::run_stage_cmd;

pub(crate) fn run_push(config: &PushConfig) -> bool {
    let timeout = config.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS);
    log_stage("push", &config.command);

    match run_stage_cmd("push", &config.command, timeout) {
        Ok(output) => {
            log_stage("push", "成功");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            true
        }
        Err(output) => {
            log_stage("push", "失敗");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            false
        }
    }
}
