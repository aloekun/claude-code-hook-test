use std::path::Path;
use std::time::Duration;

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
        ctx.state_path,
    );
    enrich_with_classifier(&mut state, ctx.classifier_config, ctx.state_path);
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
        ctx.state_path,
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
        state.review_recheck_count = existing.review_recheck_count;
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
        mark_ci_skipped(state);
    }
    if skip_coderabbit {
        mark_coderabbit_skipped(state);
    }
    if skip_ci || skip_coderabbit {
        state.action = recompute_action(state, skip_ci, skip_coderabbit);
    } else if should_downgrade_rate_limited_success(state, skip_coderabbit) {
        state.action = "continue_monitoring".into();
    }
}

fn mark_ci_skipped(state: &mut PrMonitorState) {
    state.ci = Some(CiState {
        overall: "skipped".into(),
        runs: vec![],
    });
}

fn mark_coderabbit_skipped(state: &mut PrMonitorState) {
    state.coderabbit = Some(CodeRabbitState {
        review_state: "skipped".into(),
        new_comments: 0,
        actionable_comments: None,
        unresolved_threads: None,
    });
    state.findings = Vec::new();
}

/// skip 未指定 (本番設定: `check_ci=true` / `check_coderabbit=true`) の既定経路で、
/// rate-limit 中に誤って `stop_monitoring_success` へ倒れた `state.action` を検出する。
///
/// この経路では上の `recompute_action` 呼び出しが発火せず、`state.action` は
/// `check-ci-coderabbit::decide()` の値がそのまま残る。`decide()` は `rate_limit` を
/// 知らないため、レート制限中で「コメント0件」だと誤って `stop_monitoring_success` を
/// 返すことがある (PR #307/#309 実観測)。ここで検出した場合は呼び出し側で
/// `continue_monitoring` に差し戻し、`run_one_iteration` の terminal 短絡を回避して
/// `handle_rate_limit_branch` の park + 再トリガー経路へ流す。
///
/// `action_required` / `stop_monitoring_failure` は `decide()` の判定 (actionable な
/// 指摘 / CI・CR 失敗) を尊重するため対象外 (`stop_monitoring_success` のみ検出)。
fn should_downgrade_rate_limited_success(state: &PrMonitorState, skip_coderabbit: bool) -> bool {
    cr_rate_limited(state, skip_coderabbit) && state.action == "stop_monitoring_success"
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

/// まだ結論を出せる状態にないか (= 監視継続すべきか) を判定する。
///
/// **rate-limit は「レビュー未実施」であって「指摘なし」ではない**。CodeRabbit が
/// レート制限でレビューを開始できなかった場合もコメントは 0 件になるため、
/// `cr_ok` (new_comments == 0 && unresolved_threads == 0) だけを見ると
/// **レビュー未実施と clean レビューが区別できず** success に倒れる
/// (PR #307 / #309 で実観測: 制限中なのに「問題は見つかりませんでした」と報告)。
///
/// ここで continue_monitoring に倒すことで、呼び出し側の terminal 短絡
/// (`action != "continue_monitoring"` で即 return) を回避し、
/// `handle_rate_limit_branch` の park + `@coderabbitai review` 再トリガー経路へ流す。
fn checks_still_outstanding(state: &PrMonitorState, skip_ci: bool, skip_coderabbit: bool) -> bool {
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

    ci_pending || cr_pending || cr_rate_limited(state, skip_coderabbit)
}

/// `skip_coderabbit` 適用後に、CodeRabbit がレート制限中かどうかを判定する。
fn cr_rate_limited(state: &PrMonitorState, skip_coderabbit: bool) -> bool {
    !skip_coderabbit && state.rate_limit.is_some()
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

    if checks_still_outstanding(state, skip_ci, skip_coderabbit) {
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

    /// CI 完了 + CodeRabbit がコメント 0 件 = 一見「clean」に見える state を作る。
    /// rate_limit を後から差し込むことで「レビュー未実施」との差だけを検証できる。
    fn settled_state() -> PrMonitorState {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        state.ci = Some(crate::state::CiState {
            overall: "success".into(),
            runs: vec![],
        });
        state.coderabbit = Some(crate::state::CodeRabbitState {
            review_state: "success".into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(0),
        });
        state
    }

    fn rate_limit_state() -> crate::state::RateLimitState {
        crate::state::RateLimitState {
            until_unix_secs: 1_784_550_887,
            comment_event_time: "2026-07-20T12:10:47Z".into(),
            wait_minutes: 23,
            wait_seconds: 0,
            wait_time_parsed: true,
        }
    }

    /// PR #307 / #309 incident 再現 (bad): rate-limit 中は「コメント 0 件」でも
    /// 結論を出さず監視を継続すること。
    ///
    /// 由来: 2026-07-20。CodeRabbit がレート制限でレビューを開始できなかったのに
    /// 監視が stop_monitoring_success を返した。**レビュー未実施と clean レビューが
    /// どちらも「コメント 0 件」になる**ため、rate_limit を見ないと区別できない。
    /// success に倒れると呼び出し側の terminal 短絡で rate-limit 分岐に到達せず、
    /// park も `@coderabbitai review` の再トリガーも起きない。
    #[test]
    fn rate_limited_review_is_not_treated_as_clean() {
        let mut state = settled_state();
        state.rate_limit = Some(rate_limit_state());

        assert!(
            checks_still_outstanding(&state, false, false),
            "rate-limit 中は監視継続すべき (レビュー未実施を clean と誤判定しない)",
        );
        assert_eq!(
            recompute_action(&state, false, false),
            "continue_monitoring",
            "success に倒すと terminal 短絡で park 経路に到達しない",
        );
    }

    /// good: rate-limit が無ければ従来どおり success に到達すること (退行なし)。
    #[test]
    fn settled_checks_without_rate_limit_still_reach_success() {
        let state = settled_state();

        assert!(!checks_still_outstanding(&state, false, false));
        assert_eq!(recompute_action(&state, false, false), "stop_monitoring_success");
    }

    /// good: CodeRabbit を skip する構成では rate-limit があっても監視を止めないこと。
    /// skip 指定は「CodeRabbit を判断材料にしない」意味なので、その中の rate-limit も
    /// 判断材料から外れる。
    #[test]
    fn rate_limit_is_ignored_when_coderabbit_is_skipped() {
        let mut state = settled_state();
        state.rate_limit = Some(rate_limit_state());

        assert!(
            !checks_still_outstanding(&state, false, true),
            "skip_coderabbit = true なら rate-limit も判断材料から外す",
        );
    }

    /// SIM-NEW-iteration.rs-L259 fix (regression): skip 未指定の既定経路
    /// (本番設定 `check_ci=true` / `check_coderabbit=true` → skip_ci=false /
    /// skip_coderabbit=false) では旧実装が `recompute_action` を呼ばず、
    /// `checks_still_outstanding` の rate-limit チェックが実質デッドコードだった。
    /// `apply_skip_handling` を直接呼び、`state.action` が実際に downgrade
    /// されることを確認する。
    #[test]
    fn apply_skip_handling_downgrades_rate_limited_success_without_skip_flags() {
        let mut state = settled_state();
        state.action = "stop_monitoring_success".into();
        state.rate_limit = Some(rate_limit_state());

        apply_skip_handling(&mut state, false, false);

        assert_eq!(
            state.action, "continue_monitoring",
            "skip 未指定の既定経路でも rate-limit 中は success を確定させてはいけない",
        );
    }

    /// good: rate-limit があっても `action_required` (actionable な既存指摘) は
    /// 上書きしないこと。stale な状態でも既にある指摘を握りつぶすべきではない。
    #[test]
    fn apply_skip_handling_keeps_action_required_even_when_rate_limited() {
        let mut state = settled_state();
        state.action = "action_required".into();
        state.rate_limit = Some(rate_limit_state());

        apply_skip_handling(&mut state, false, false);

        assert_eq!(state.action, "action_required");
    }

    /// good: rate-limit があっても `stop_monitoring_failure` (CI/CR失敗) は
    /// 上書きしないこと。失敗判定は decide() の優先順位を尊重する。
    #[test]
    fn apply_skip_handling_keeps_failure_even_when_rate_limited() {
        let mut state = settled_state();
        state.action = "stop_monitoring_failure".into();
        state.rate_limit = Some(rate_limit_state());

        apply_skip_handling(&mut state, false, false);

        assert_eq!(state.action, "stop_monitoring_failure");
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
