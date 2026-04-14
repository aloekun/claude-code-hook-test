use crate::config::{PushConfig, DEFAULT_STEP_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::run_stage_cmd;

pub(crate) fn run_push(config: &PushConfig) -> bool {
    log_stage("push", &config.command);

    match run_stage_cmd("push", &config.command, DEFAULT_STEP_TIMEOUT_SECS) {
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
