use crate::config::TaktConfig;
use crate::log::log_stage;
use crate::runner::run_cmd_inherit;

pub(crate) fn run_takt(config: &TaktConfig) -> bool {
    log_stage(
        "takt",
        &format!("ワークフロー '{}' を起動", config.workflow),
    );

    let mut args = vec!["exec", "takt", "-w", &config.workflow, "-t", &config.task];

    let extra: Vec<&str> = config
        .extra_args
        .as_ref()
        .map(|v| v.iter().map(String::as_str).collect())
        .unwrap_or_default();
    args.extend(extra);

    let success = run_cmd_inherit("takt", "pnpm", &args);

    if success {
        log_stage("takt", "ワークフロー完了");
    } else {
        log_stage("takt", "ワークフロー失敗");
    }

    success
}
