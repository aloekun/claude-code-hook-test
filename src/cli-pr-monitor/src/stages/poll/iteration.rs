use std::time::Duration;

use crate::classifier_runner::classify_findings;
use crate::config::{ClassifierConfig, DEFAULT_CHECK_TIMEOUT_SECS};
use crate::log::{log_info, truncate_safe};
use crate::runner::run_cmd_direct;
use crate::state::{
    read_state, update_state_from_check_result, write_state, CiState, CodeRabbitState,
    PrMonitorState,
};
use crate::util::{utc_now_iso8601, PrInfo};

use super::rate_limit::handle_rate_limit_branch;
use super::{error_poll_result, PollContext, PollResult};

pub(super) fn run_one_iteration(ctx: &PollContext<'_>) -> Option<PollResult> {
    let effective_push_time = ctx.fix_push_time.unwrap_or(ctx.push_time);
    let args = build_checker_args(effective_push_time, ctx.pr_info);
    let result = match invoke_checker(ctx.checker, &args) {
        Ok(r) => r,
        Err(pr) => return Some(*pr),
    };
    let mut state = build_state_for_iteration(
        ctx.pr_info,
        ctx.push_time,
        &result,
        ctx.skip_ci,
        ctx.skip_coderabbit,
    );
    enrich_with_classifier(&mut state, ctx.classifier_config);
    log_info(&format!(
        "ポーリング: action={}, summary={}",
        state.action, state.summary
    ));

    if state.action != "continue_monitoring" {
        return Some(make_terminal_result(state, result));
    }

    if let Some(terminal) = handle_rate_limit_branch(
        &mut state,
        ctx.rate_limit_config,
        ctx.pr_info,
        ctx.review_recheck_wait_secs,
        &result,
    ) {
        return Some(terminal);
    }

    if ctx.start.elapsed() >= Duration::from_secs(ctx.max_duration) {
        log_info(&format!("監視タイムアウト ({}秒)", ctx.max_duration));
        return Some(make_timeout_result(state, ctx.max_duration, result));
    }

    None
}

fn build_checker_args(push_time: &str, pr_info: &PrInfo) -> Vec<String> {
    let mut args: Vec<String> = vec!["--push-time".into(), push_time.into()];
    if let Some(ref repo) = pr_info.repo {
        args.push("--repo".into());
        args.push(repo.clone());
    }
    if let Some(pr) = pr_info.pr_number {
        args.push("--pr".into());
        args.push(pr.to_string());
    }
    args
}

fn invoke_checker(
    checker: &std::path::Path,
    args: &[String],
) -> Result<serde_json::Value, Box<PollResult>> {
    let (success, output) = run_cmd_direct(
        &checker.to_string_lossy(),
        &[],
        args,
        DEFAULT_CHECK_TIMEOUT_SECS,
    );

    if !success {
        log_info(&format!("checker 失敗: {}", truncate_safe(&output, 200)));
        return Err(Box::new(error_poll_result(&format!(
            "check-ci-coderabbit.exe 失敗: {}",
            truncate_safe(&output, 200)
        ))));
    }

    serde_json::from_str::<serde_json::Value>(&output).map_err(|e| {
        log_info(&format!("JSON パース失敗: {}", e));
        Box::new(error_poll_result(&format!(
            "checker 出力の JSON パース失敗: {}",
            e
        )))
    })
}

/// `PrMonitorState::new` は毎回 notified / rate_limit_retries を 0 リセットするため、
/// 既存 state から runtime-updated な値を読み戻して 1 iteration の base state を組む。
fn build_state_for_iteration(
    pr_info: &PrInfo,
    push_time: &str,
    result: &serde_json::Value,
    skip_ci: bool,
    skip_coderabbit: bool,
) -> PrMonitorState {
    let mut state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        push_time.to_string(),
    );
    update_state_from_check_result(&mut state, result);

    if let Some(existing) = read_state() {
        state.notified = existing.notified;
        state.rate_limit_retries = existing.rate_limit_retries;
        state.rate_limit_last_retriggered_at = existing.rate_limit_last_retriggered_at;
        state.review_recheck_count = existing.review_recheck_count;
        state.head_commit = existing.head_commit;
        state.classified_findings = existing.classified_findings;
        state.fix_push_time = existing.fix_push_time;
    }

    apply_skip_handling(&mut state, skip_ci, skip_coderabbit);
    state.last_checked = Some(utc_now_iso8601());
    if let Err(e) = write_state(&state) {
        log_info(&format!("state 書き込み失敗 (skip 反映後、続行): {}", e));
    }
    state
}

/// classifier (ADR-038, Phase 5) で findings を enrich する。
///
/// `config.classifier.enabled = false` または findings が空のときは何もしない。
/// 実行成功時は state.classified_findings を populate して state file を再書き出す。
/// 失敗時は state.classified_findings は空のまま (caller は findings をそのまま使えばよい)。
fn enrich_with_classifier(state: &mut PrMonitorState, config: &ClassifierConfig) {
    if !config.enabled || state.findings.is_empty() {
        return;
    }
    let classified = classify_findings(config, &state.findings);
    if classified.is_empty() {
        return;
    }
    log_info(&format!(
        "classifier: {} findings を分類完了",
        classified.len()
    ));
    state.classified_findings = classified;
    if let Err(e) = write_state(state) {
        log_info(&format!(
            "state 書き込み失敗 (classifier enrich 後、続行): {}",
            e
        ));
    }
}

fn apply_skip_handling(state: &mut PrMonitorState, skip_ci: bool, skip_coderabbit: bool) {
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
        state.action = recompute_action(state, skip_ci, skip_coderabbit);
    }
}

fn make_terminal_result(state: PrMonitorState, result: serde_json::Value) -> PollResult {
    PollResult {
        action: state.action,
        summary: state.summary,
        ci: state.ci,
        coderabbit: state.coderabbit,
        findings: state.findings,
        check_output: Some(result),
        rate_limit: state.rate_limit,
    }
}

fn make_timeout_result(
    state: PrMonitorState,
    max_duration: u64,
    result: serde_json::Value,
) -> PollResult {
    PollResult {
        action: "timed_out".into(),
        summary: format!("監視タイムアウト ({}秒)", max_duration),
        ci: state.ci,
        coderabbit: state.coderabbit,
        findings: state.findings,
        check_output: Some(result),
        rate_limit: state.rate_limit,
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
        state.action.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib_report_formatter::Finding;

    /// PR #120 W-001 follow-up (順位 83): `enrich_with_classifier` の `!config.enabled`
    /// guard を **単独で** 検証する。`findings` を非空 (= `findings.is_empty()` guard
    /// 不発)、`enabled = false` (= 本 guard 発火) にして 2 つの OR guard を直交させる。
    ///
    /// 検証対象 field `state.classified_findings` を sentinel で pre-populate し、
    /// 早期 return しなかった場合の代入 (`state.classified_findings = classified;`)
    /// を sentinel 消失として検出する設計。空のまま渡すと「不変=空」が早期 return
    /// 由来か他経路由来か判別できないため sentinel 必須。
    #[test]
    fn enrich_with_classifier_skips_when_disabled() {
        use crate::classifier_runner::ClassifiedFinding;

        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.findings = vec![Finding {
            severity: "Major".into(),
            file: "f.rs".into(),
            line: "1".into(),
            issue: "issue".into(),
            suggestion: "fix".into(),
            source: "coderabbit".into(),
        }];
        let sentinel = ClassifiedFinding {
            finding: Finding {
                severity: "Minor".into(),
                file: "sentinel.rs".into(),
                line: "1".into(),
                issue: "sentinel".into(),
                suggestion: "must not be overwritten".into(),
                source: "test".into(),
            },
            action: "auto_fix".into(),
            action_confidence: 0.99,
            normalized_issue: None,
            fallback_reason: None,
        };
        state.classified_findings = vec![sentinel.clone()];
        let disabled = ClassifierConfig {
            enabled: false,
            ..ClassifierConfig::default()
        };

        enrich_with_classifier(&mut state, &disabled);

        assert_eq!(
            state.classified_findings,
            vec![sentinel],
            "!config.enabled guard should early return before any mutation"
        );
    }

    /// `state.findings.is_empty()` guard (`enrich_with_classifier` 2 番目の早期 return)
    /// を単独で検証する。`enabled = true` (明示、= `!config.enabled` guard 不発)、
    /// `findings` 空 (= 本 guard 発火) にして他条件と直交させる。
    #[test]
    fn enrich_with_classifier_skips_when_findings_empty() {
        use crate::classifier_runner::ClassifiedFinding;

        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        assert!(
            state.findings.is_empty(),
            "test precondition: findings must be empty so `!enabled` guard stays unfired"
        );
        let sentinel = ClassifiedFinding {
            finding: Finding {
                severity: "Minor".into(),
                file: "sentinel.rs".into(),
                line: "1".into(),
                issue: "sentinel".into(),
                suggestion: "must not be overwritten".into(),
                source: "test".into(),
            },
            action: "auto_fix".into(),
            action_confidence: 0.99,
            normalized_issue: None,
            fallback_reason: None,
        };
        state.classified_findings = vec![sentinel.clone()];
        let enabled = ClassifierConfig {
            enabled: true,
            ..ClassifierConfig::default()
        };

        enrich_with_classifier(&mut state, &enabled);

        assert_eq!(
            state.classified_findings,
            vec![sentinel],
            "findings.is_empty() guard should early return before any mutation"
        );
    }
}
