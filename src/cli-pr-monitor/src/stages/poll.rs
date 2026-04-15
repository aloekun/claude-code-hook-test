use lib_report_formatter::Finding;
use std::time::Duration;

use crate::config::{MonitorConfig, DEFAULT_CHECK_TIMEOUT_SECS};
use crate::log::{log_info, truncate_safe};
use crate::runner::{checker_exe_path, run_cmd_direct};
use crate::state::{
    update_state_from_check_result, write_state, CiState, CodeRabbitState, PrMonitorState,
};
use crate::util::{utc_now_iso8601, PrInfo};

pub(crate) struct PollResult {
    pub(crate) action: String,
    pub(crate) summary: String,
    pub(crate) ci: Option<CiState>,
    pub(crate) coderabbit: Option<CodeRabbitState>,
    pub(crate) findings: Vec<Finding>,
    pub(crate) check_output: Option<serde_json::Value>,
}

/// in-process 同期ポーリングループ (daemon.rs の同期版)
pub(crate) fn run_poll_loop(config: &MonitorConfig, pr_info: &PrInfo) -> PollResult {
    let poll_interval = config.poll_interval_secs;
    let max_duration = config.max_duration_secs;
    let skip_ci = !config.check_ci;
    let skip_coderabbit = !config.check_coderabbit;

    let checker = checker_exe_path();
    if !checker.exists() {
        log_info(&format!(
            "check-ci-coderabbit.exe が見つかりません: {}",
            checker.display()
        ));
        return PollResult {
            action: "error".into(),
            summary: "check-ci-coderabbit.exe が見つかりません".into(),
            ci: None,
            coderabbit: None,
            findings: Vec::new(),
            check_output: None,
        };
    }

    let push_time = pr_info
        .push_time
        .as_deref()
        .unwrap_or("1970-01-01T00:00:00Z");

    let start = std::time::Instant::now();

    loop {
        // Build checker arguments
        let mut checker_args: Vec<String> = vec!["--push-time".to_string(), push_time.to_string()];
        if let Some(ref repo) = pr_info.repo {
            checker_args.push("--repo".to_string());
            checker_args.push(repo.clone());
        }
        if let Some(pr) = pr_info.pr_number {
            checker_args.push("--pr".to_string());
            checker_args.push(pr.to_string());
        }

        // Run check-ci-coderabbit.exe
        let (success, output) = run_cmd_direct(
            &checker.to_string_lossy(),
            &[],
            &checker_args,
            DEFAULT_CHECK_TIMEOUT_SECS,
        );

        if !success {
            log_info(&format!("checker 失敗: {}", truncate_safe(&output, 200)));
            return PollResult {
                action: "error".into(),
                summary: format!(
                    "check-ci-coderabbit.exe 失敗: {}",
                    truncate_safe(&output, 200)
                ),
                ci: None,
                coderabbit: None,
                findings: Vec::new(),
                check_output: None,
            };
        }

        let result = match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(r) => r,
            Err(e) => {
                log_info(&format!("JSON パース失敗: {}", e));
                return PollResult {
                    action: "error".into(),
                    summary: format!("checker 出力の JSON パース失敗: {}", e),
                    ci: None,
                    coderabbit: None,
                    findings: Vec::new(),
                    check_output: None,
                };
            }
        };

        // Update state from check result
        let mut state = PrMonitorState::new(
            pr_info.pr_number,
            pr_info.repo.clone(),
            push_time.to_string(),
        );
        update_state_from_check_result(&mut state, &result);

        // Skip handling: skipped なチェックを成功扱いにした後、action を再計算する
        if skip_ci {
            state.ci = Some(CiState {
                overall: "skipped".into(),
                runs: vec![],
            });
        }
        if skip_coderabbit {
            state.coderabbit = Some(CodeRabbitState {
                review_state: "skipped".into(),
                new_comments: 0,
                actionable_comments: None,
                unresolved_threads: None,
            });
            state.findings = Vec::new();
        }
        if skip_ci || skip_coderabbit {
            state.action = recompute_action(&state, skip_ci, skip_coderabbit);
        }

        state.last_checked = Some(utc_now_iso8601());

        // Write state for debug/observability
        let _ = write_state(&state);

        log_info(&format!(
            "ポーリング: action={}, summary={}",
            state.action, state.summary
        ));

        // Terminal action -> return result
        if state.action != "continue_monitoring" {
            return PollResult {
                action: state.action,
                summary: state.summary,
                ci: state.ci,
                coderabbit: state.coderabbit,
                findings: state.findings,
                check_output: Some(result),
            };
        }

        // Timeout check
        if start.elapsed() >= Duration::from_secs(max_duration) {
            log_info(&format!("監視タイムアウト ({}秒)", max_duration));
            return PollResult {
                action: "timed_out".into(),
                summary: format!("監視タイムアウト ({}秒)", max_duration),
                ci: state.ci,
                coderabbit: state.coderabbit,
                findings: state.findings,
                check_output: Some(result),
            };
        }

        // Sleep before next poll
        std::thread::sleep(Duration::from_secs(poll_interval));
    }
}

/// skip 適用後に、有効なチェックだけを見て action を再導出する
fn recompute_action(state: &PrMonitorState, skip_ci: bool, skip_coderabbit: bool) -> String {
    let ci_ok = skip_ci
        || state
            .ci
            .as_ref()
            .map(|c| c.overall == "success" || c.overall == "skipped")
            .unwrap_or(false);

    let cr_ok = skip_coderabbit
        || state
            .coderabbit
            .as_ref()
            .map(|c| {
                c.review_state == "skipped"
                    || (c.new_comments == 0 && c.unresolved_threads.unwrap_or(0) == 0)
            })
            .unwrap_or(false);

    let ci_pending = !skip_ci
        && state
            .ci
            .as_ref()
            .map(|c| c.overall == "pending")
            .unwrap_or(true);

    let cr_pending = !skip_coderabbit
        && state
            .coderabbit
            .as_ref()
            .map(|c| c.review_state == "not_found" || c.review_state == "pending")
            .unwrap_or(true);

    if ci_pending || cr_pending {
        return "continue_monitoring".into();
    }

    let ci_failed = !skip_ci
        && state
            .ci
            .as_ref()
            .map(|c| c.overall == "failure")
            .unwrap_or(false);

    let cr_action_required = !skip_coderabbit
        && state
            .coderabbit
            .as_ref()
            .map(|c| c.new_comments > 0 || c.unresolved_threads.unwrap_or(0) > 0)
            .unwrap_or(false);

    if ci_failed {
        "stop_monitoring_failure".into()
    } else if cr_action_required {
        "action_required".into()
    } else if ci_ok && cr_ok {
        "stop_monitoring_success".into()
    } else {
        // Fallback: keep original action
        state.action.clone()
    }
}
