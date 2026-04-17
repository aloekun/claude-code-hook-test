use super::push_jj_bookmark::advance_jj_bookmarks;
use crate::config::{PushConfig, DEFAULT_PUSH_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::run_stage_cmd;

pub(crate) fn run_push(config: &PushConfig) -> bool {
    // (takt fix や手動 jj describe で @ が進んでも bookmark が旧コミットのまま残る問題の対策)
    if config.command.starts_with("jj ") {
        if let Err(e) = advance_jj_bookmarks() {
            log_stage(
                "push",
                &format!("bookmark 自動更新失敗 (push は続行): {}", e),
            );
        }
    }

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
