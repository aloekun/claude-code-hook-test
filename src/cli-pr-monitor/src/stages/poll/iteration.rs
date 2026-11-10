use std::path::Path;

use crate::classifier_runner::classify_findings;
use crate::config::{ClassifierConfig, DEFAULT_CHECK_TIMEOUT_SECS};
use crate::log::{log_info, truncate_safe};
use crate::runner::run_cmd_capture;
use crate::state::{
    read_state_from, update_state_from_check_result, write_state_to, CiState, CodeRabbitState,
    PrMonitorState,
};
use crate::util::{utc_now_iso8601, PrInfo};

use super::rate_limit::handle_rate_limit_branch;
use super::{error_poll_result, PollContext, PollResult};

/// 1 回の checker 呼び出しの結果。
pub(super) enum Iteration {
    /// 監視を終える結果。
    Terminal(Box<PollResult>),
    /// 未確定。最後の checker 出力を持つ。`pending_review` の結果にも載せ、takt の分析が
    /// pre-merge checks などを読めるようにする (PR #625 CodeRabbit)。
    Pending(serde_json::Value),
}

/// checker を 1 回呼び、監視を終えるか (terminal) 未確定かを返す。
pub(super) fn run_one_iteration(ctx: &PollContext<'_>) -> Iteration {
    let effective_push_time = ctx.fix_push_time.unwrap_or(ctx.push_time);
    let args = build_checker_args(effective_push_time, ctx.pr_info);
    let result = match invoke_checker(ctx.checker, &args) {
        Ok(r) => r,
        Err(pr) => return Iteration::Terminal(pr),
    };
    let mut state = build_state_for_iteration(
        ctx.pr_info,
        ctx.push_time,
        &result,
        ctx.skip_ci,
        ctx.skip_coderabbit,
        ctx.state_path,
    );
    enrich_with_classifier(&mut state, ctx.classifier_config, ctx.state_path);
    log_info(&format!(
        "ポーリング: action={}, summary={}",
        state.action, state.summary
    ));

    if state.action != "continue_monitoring" {
        return Iteration::Terminal(Box::new(make_terminal_result(state, result)));
    }

    if let Some(terminal) =
        handle_rate_limit_branch(&mut state, ctx.rate_limit_config, ctx.pr_info, &result, ctx.state_path)
    {
        return Iteration::Terminal(Box::new(terminal));
    }

    Iteration::Pending(result)
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

/// checker を起動し stdout のみを JSON としてパースする。
///
/// stderr は log 転送に留める: checker は repo 検出失敗等を stderr に警告しつつ
/// exit 0 で fail-soft JSON を返すことがあり、結合出力をパースすると正常な JSON
/// の後ろに stderr が連結され「trailing characters」で監視が停止する (PR #238 実観測)。
fn invoke_checker(
    checker: &std::path::Path,
    args: &[String],
) -> Result<serde_json::Value, Box<PollResult>> {
    let cap = run_cmd_capture(
        &checker.to_string_lossy(),
        &[],
        args,
        DEFAULT_CHECK_TIMEOUT_SECS,
    );

    let stderr_trimmed = cap.stderr.trim();
    if !stderr_trimmed.is_empty() {
        log_info(&format!(
            "checker stderr (JSON パース対象から分離): {}",
            truncate_safe(stderr_trimmed, 300)
        ));
    }

    if !cap.ok {
        let mut detail = format!("{}{}", cap.stdout, cap.stderr).trim().to_string();
        if cap.timed_out {
            detail = format!("{} (timeout {}s)", detail, DEFAULT_CHECK_TIMEOUT_SECS);
        }
        log_info(&format!("checker 失敗: {}", truncate_safe(&detail, 200)));
        return Err(Box::new(error_poll_result(&format!(
            "check-ci-coderabbit.exe 失敗: {}",
            truncate_safe(&detail, 200)
        ))));
    }

    serde_json::from_str::<serde_json::Value>(cap.stdout.trim()).map_err(|e| {
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
    state_path: &Path,
) -> PrMonitorState {
    let mut state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        push_time.to_string(),
    );
    update_state_from_check_result(&mut state, result);

    if let Some(existing) = read_state_from(state_path) {
        state.notified = existing.notified;
        state.rate_limit_retries = existing.rate_limit_retries;
        state.rate_limit_last_retriggered_at = existing.rate_limit_last_retriggered_at;
        state.head_commit = existing.head_commit;
        state.classified_findings = existing.classified_findings;
        state.fix_push_time = existing.fix_push_time;
    }

    apply_skip_handling(&mut state, skip_ci, skip_coderabbit);
    state.last_checked = Some(utc_now_iso8601());
    if let Err(e) = write_state_to(state_path, &state) {
        log_info(&format!("state 書き込み失敗 (skip 反映後、続行): {}", e));
    }
    state
}

/// classifier (ADR-038, Phase 5) で findings を enrich する。
///
/// `config.classifier.enabled = false` または findings が空のときは何もしない。
/// 実行成功時は state.classified_findings を populate して state file を再書き出す。
/// 失敗時は state.classified_findings は空のまま (caller は findings をそのまま使えばよい)。
fn enrich_with_classifier(
    state: &mut PrMonitorState,
    config: &ClassifierConfig,
    state_path: &Path,
) {
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
    if let Err(e) = write_state_to(state_path, state) {
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

/// skip しなかったチェックのどれかが、まだ結果を出していないか。
///
/// CodeRabbit の `not_started` (CR がレビューをスキップした状態、順位 585) も未確定に数える。
/// 依頼しない限りレビューは来ないので、完了扱いにすると未レビューのまま監視が終わる。
fn any_check_unsettled(state: &PrMonitorState, skip_ci: bool, skip_coderabbit: bool) -> bool {
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
            .map(|c| matches!(c.review_state.as_str(), "not_found" | "pending" | "not_started"))
            .unwrap_or(true);
    ci_pending || cr_pending
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

    if any_check_unsettled(state, skip_ci, skip_coderabbit) {
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
        pre_merge_override(state, skip_coderabbit)
            .or_else(|| severity_override(state, skip_coderabbit))
            .unwrap_or_else(|| "stop_monitoring_success".into())
    } else {
        state.action.clone()
    }
}

/// 指摘の重大度を全件読めなかったなら、success の代わりに `action_required` を返す (順位 603)。
///
/// check-ci-coderabbit の `severity_readout::apply_severity_gate` と同じ規則で、
/// [`pre_merge_override`] と同じ理由で複製している。規則を変えるときは両方を直す。
fn severity_override(state: &PrMonitorState, skip_coderabbit: bool) -> Option<String> {
    (!skip_coderabbit && state.severity_unreadable).then(|| "action_required".into())
}

/// 要約コメントの pre-merge check が失敗していれば、success の代わりの action を返す。
///
/// 今の head で測った失敗は `action_required`、古い測定は `continue_monitoring` (再測定待ち)。
/// check-ci-coderabbit の `pre_merge::apply_pre_merge_gate` と同じ規則で、crate 間で共有する
/// lib が無いので複製している。規則を変えるときは両方を直す。
fn pre_merge_override(state: &PrMonitorState, skip_coderabbit: bool) -> Option<String> {
    if skip_coderabbit {
        return None;
    }
    let checks = state.pre_merge_checks.as_ref().filter(|c| !c.failed.is_empty())?;
    let action = if checks.stale { "continue_monitoring" } else { "action_required" };
    Some(action.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib_report_formatter::Finding;

    fn state_with_review(review_state: &str) -> PrMonitorState {
        let mut s = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        s.coderabbit = Some(CodeRabbitState {
            review_state: review_state.into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(0),
        });
        s
    }

    /// 順位 585: CI を skip しても、CodeRabbit の `not_started` (スキップ通知) は未確定のまま。
    /// 完了扱いにすると、レビューされていない PR の監視が success で終わる。
    #[test]
    fn not_started_review_keeps_monitoring_when_ci_is_skipped() {
        let state = state_with_review("not_started");
        assert!(any_check_unsettled(&state, true, false));
        assert_eq!(recompute_action(&state, true, false), "continue_monitoring");
    }

    #[test]
    fn completed_review_settles_when_ci_is_skipped() {
        let state = state_with_review("success");
        assert!(!any_check_unsettled(&state, true, false));
        assert_eq!(recompute_action(&state, true, false), "stop_monitoring_success");
    }

    fn with_pre_merge_failure(mut state: PrMonitorState, stale: bool) -> PrMonitorState {
        state.pre_merge_checks = Some(crate::state::PreMergeChecksState {
            failed: vec![crate::state::PreMergeCheckState {
                name: "Docstring Coverage".into(),
                status: "Warning".into(),
            }],
            stale,
        });
        state
    }

    /// #623: 指摘 0 件・未解決 0 件でも、失敗した pre-merge check があれば success で終えない。
    /// CI を skip した経路では action を state から決め直すので、ここでも止める。
    #[test]
    fn pre_merge_failure_blocks_success_when_ci_is_skipped() {
        let current = with_pre_merge_failure(state_with_review("success"), false);
        assert_eq!(recompute_action(&current, true, false), "action_required");
        let stale = with_pre_merge_failure(state_with_review("success"), true);
        assert_eq!(recompute_action(&stale, true, false), "continue_monitoring");
    }

    /// CodeRabbit 自体を skip するなら、その要約コメントの check も見ない。
    #[test]
    fn pre_merge_failure_is_ignored_when_coderabbit_is_skipped() {
        let mut state = with_pre_merge_failure(state_with_review("success"), false);
        state.ci = Some(CiState {
            overall: "success".into(),
            runs: vec![],
        });
        assert_eq!(recompute_action(&state, false, true), "stop_monitoring_success");
    }

    /// 順位 603: 重大度を全件読めなかったなら、CI を skip した経路でも success で終えない。
    /// CodeRabbit を skip するなら見ない (findings 自体を捨てるため)。
    #[test]
    fn unreadable_severity_blocks_success_when_ci_is_skipped() {
        let mut state = state_with_review("success");
        state.severity_unreadable = true;
        assert_eq!(recompute_action(&state, true, false), "action_required");
        state.ci = Some(CiState {
            overall: "success".into(),
            runs: vec![],
        });
        assert_eq!(recompute_action(&state, false, true), "stop_monitoring_success");
    }

    /// PR #238 regression: checker が stderr に警告 (repo 検出失敗等の fail-soft ログ)
    /// を出しても、stdout の JSON パースが壊れないこと。修正前は stdout+stderr の
    /// 結合テキストをパースしており「trailing characters」で監視が停止した。
    #[test]
    #[cfg(windows)]
    fn invoke_checker_parses_stdout_json_despite_stderr_noise() {
        let result = invoke_checker(
            std::path::Path::new("cmd"),
            &[
                "/C".to_string(),
                "echo [1,2]& echo [checker] repo detect failed 1>&2".to_string(),
            ],
        );
        let value = result.unwrap_or_else(|pr| {
            panic!(
                "stderr ノイズ入りでもパース成功すべき: action={}, summary={}",
                pr.action, pr.summary
            )
        });
        assert_eq!(value, serde_json::json!([1, 2]));
    }

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
        let dir = tempfile::tempdir().unwrap();

        enrich_with_classifier(&mut state, &disabled, &dir.path().join("state.json"));

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
        let dir = tempfile::tempdir().unwrap();

        enrich_with_classifier(&mut state, &enabled, &dir.path().join("state.json"));

        assert_eq!(
            state.classified_findings,
            vec![sentinel],
            "findings.is_empty() guard should early return before any mutation"
        );
    }
}
