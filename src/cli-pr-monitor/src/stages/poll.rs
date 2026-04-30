use lib_report_formatter::Finding;
use std::time::Duration;

use crate::config::{Config, MonitorConfig, RateLimitConfig, DEFAULT_CHECK_TIMEOUT_SECS};
use crate::log::{log_info, truncate_safe};
use crate::runner::{checker_exe_path, run_cmd_direct, run_gh_quiet};
use crate::state::{
    read_state, update_state_from_check_result, write_state, CiState, CodeRabbitState,
    PrMonitorState,
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
pub(crate) fn run_poll_loop(full_config: &Config, pr_info: &PrInfo) -> PollResult {
    let config: &MonitorConfig = &full_config.monitor;
    let rate_limit_config: &RateLimitConfig = &full_config.rate_limit;
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

        // `PrMonitorState::new` は毎回 notified=false / rate_limit_retries=0 で初期化するため、
        // 既存 state から runtime-updated な値を読み戻す。新規セッションでは
        // start_monitoring 冒頭で init_state により reset 済み。
        if let Some(existing) = read_state() {
            state.notified = existing.notified;
            state.rate_limit_retries = existing.rate_limit_retries;
            state.rate_limit_last_retriggered_at = existing.rate_limit_last_retriggered_at;
        }

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

        // Rate-limit 自動 retry (PR #89 T2-1)
        //
        // dedup: 同一の rate-limit comment は iteration を跨いで PR コメント一覧に残るため
        // `comment_event_time` で dedup しないと、毎回 sleep_secs=0 で即時 retrigger を繰り返し
        // 数秒で max_retries を消費してしまう。CR が新たな rate-limit comment を投稿した時点で
        // created_at が変わり再度 retrigger 対象になる。
        if let Some(rl) = state.rate_limit.clone() {
            let already_handled = state.rate_limit_last_retriggered_at.as_deref()
                == Some(rl.comment_event_time.as_str());

            if already_handled {
                log_info(&format!(
                    "[rate_limit] 同じ rate-limit comment ({}) は処理済み、retrigger スキップ",
                    rl.comment_event_time
));
                // 通常 polling cadence で待機する (CR レビュー完了を待つ)
            } else if rate_limit_config.auto_retry_enabled
                && state.rate_limit_retries < rate_limit_config.max_retries
            {
                let elapsed = start.elapsed().as_secs();
                let remaining_monitor_secs = max_duration.saturating_sub(elapsed);

                if let Err(e) = handle_rate_limit_retry(
                    &rl,
                    &mut state,
                    pr_info,
                    rate_limit_config.max_retries,
                    remaining_monitor_secs,
                ) {
                    // 失敗時は dedup を更新せず action_required で抜ける。
                    // last_retriggered_at が未更新のため、次セッションで同 comment を
                    // 再 trigger 試行できる (本セッションでは budget 超過/post 失敗のため停止)。
                    log_info(&format!("[rate_limit] retrigger 失敗: {}", e));
                    return PollResult {
                        action: "action_required".into(),
                        summary: format!(
                            "rate-limit 自動 retry 失敗 ({})。手動で `@coderabbitai review` を投稿してください",
                            e
                        ),
                        ci: state.ci,
                        coderabbit: state.coderabbit,
                        findings: state.findings,
                        check_output: Some(result),
                    };
                }
                state.rate_limit_last_retriggered_at = Some(rl.comment_event_time.clone());
                // state 永続化失敗時は dedup / max_retries が壊れる可能性があるため、
                // 自動 retry を停止し action_required で抜ける (重複投稿リスクを回避)。
                if let Err(e) = write_state(&state) {
                    log_info(&format!(
                        "[rate_limit] retrigger 後の state 永続化失敗、自動 retry を停止: {}",
                        e
                    ));
                    return PollResult {
                        action: "action_required".into(),
                        summary: format!(
                            "rate-limit retry 後の state 永続化に失敗 ({})。手動で `@coderabbitai review` の重複投稿に注意してください",
                            e
                        ),
                        ci: state.ci,
                        coderabbit: state.coderabbit,
                        findings: state.findings,
                        check_output: Some(result),
                    };
                }
                continue; // skip 通常 sleep、次 iteration で fresh polling
            } else if state.rate_limit_retries >= rate_limit_config.max_retries {
                log_info(&format!(
                    "[rate_limit] max_retries={} 到達、自動 retry を停止 (action_required で抜ける)",
                    rate_limit_config.max_retries
                ));
                return PollResult {
                    action: "action_required".into(),
                    summary: format!(
                        "CodeRabbit rate-limit が {} 回再試行後も継続。手動で `@coderabbitai review` を投稿してください",
                        state.rate_limit_retries
                    ),
                    ci: state.ci,
                    coderabbit: state.coderabbit,
                    findings: state.findings,
                    check_output: Some(result),
                };
            }
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

/// rate-limit reset まで sleep し、`@coderabbitai review` を post する。
///
/// 成功時のみ `Ok(())` を返し、`state.rate_limit_retries` をインクリメントする。
/// 失敗ケース (PR 番号未確定 / sleep が監視残り予算超過 / gh post 失敗) は
/// state を変更せず `Err` を返す。caller は `last_retriggered_at` を更新せず、
/// 同 comment を未処理のまま残すこと。
///
/// `until_unix_secs <= now` の場合は sleep をスキップして即時 retrigger
/// (= 過去の rate-limit comment を発見、既にリセット済み)。
fn handle_rate_limit_retry(
    rl: &crate::state::RateLimitState,
    state: &mut PrMonitorState,
    pr_info: &PrInfo,
    max_retries: u32,
    remaining_monitor_secs: u64,
) -> Result<(), String> {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let sleep_secs = (rl.until_unix_secs - now_unix).max(0) as u64;

    // 監視残り時間を超える sleep は実施しない (max_duration を素通りさせない)
    if sleep_secs > remaining_monitor_secs {
        return Err(format!(
            "rate-limit sleep ({}s) > 監視残り予算 ({}s)",
            sleep_secs, remaining_monitor_secs
        ));
    }

    let pr = pr_info
        .pr_number
        .ok_or_else(|| "PR 番号未確定のため retrigger スキップ".to_string())?;

    if sleep_secs > 0 {
        log_info(&format!(
            "[rate_limit] reset まで sleep {}秒 (wait={}m{}s + 60s buffer、retry={}/{})",
            sleep_secs,
            rl.wait_minutes,
            rl.wait_seconds,
            state.rate_limit_retries + 1,
            max_retries
        ));
        std::thread::sleep(Duration::from_secs(sleep_secs));
    } else {
        log_info(&format!(
            "[rate_limit] reset 時刻は既に過去、即時 retrigger (retry={})",
            state.rate_limit_retries + 1
        ));
    }

    let pr_str = pr.to_string();
    if run_gh_quiet(&["pr", "comment", &pr_str, "--body", "@coderabbitai review"]).is_none() {
        return Err(format!("gh pr comment 投稿失敗 (PR #{})", pr));
    }

    log_info(&format!(
        "[rate_limit] @coderabbitai review を投稿 (PR #{}, retry={})",
        pr,
        state.rate_limit_retries + 1
    ));

    state.rate_limit_retries += 1;
    // rate_limit field は次の polling iteration で再 detect されるためここでは clear しない。
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RateLimitState;

    #[test]
    fn rate_limit_state_persists_retries_across_polls() {
        // simulate state.json round-trip behavior: 1 iteration で incremented した
        // retries が次 iteration で復元されることを検証
        let tmp = std::env::temp_dir().join(format!("test-rl-retries-{}.json", std::process::id()));
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.rate_limit_retries = 2;
        state.rate_limit = Some(RateLimitState {
            until_unix_secs: 1_735_689_600,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 13,
        });
        crate::state::write_state_to(&tmp, &state).unwrap();

        let loaded = crate::state::read_state_from(&tmp).unwrap();
        assert_eq!(loaded.rate_limit_retries, 2);
        assert_eq!(
            loaded.rate_limit.as_ref().unwrap().until_unix_secs,
            1_735_689_600
        );

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn rate_limit_default_config_allows_retry_within_limit() {
        let cfg = RateLimitConfig::default();
        assert!(cfg.auto_retry_enabled);
        assert_eq!(cfg.max_retries, 3);
        // 2 retries 後: 2 < 3 で auto_retry_enabled パスを通る
        assert!(2 < cfg.max_retries);
        // 3 retries 後: 3 >= 3 で max 到達 → action_required で抜ける
        assert!(3 >= cfg.max_retries);
    }

    /// 同じ rate-limit comment が iteration 跨ぎで残った場合に dedup が働くことを検証する。
    ///
    /// シナリオ (advisor 発見のバグ):
    /// - Iter 1: comment A, retries=0, last_retriggered=None → handle 対象
    /// - Iter 2: 同じ comment A still in PR, last_retriggered=A → 即時 retrigger を skip
    /// - Iter 3: CR が新たな rate-limit comment B を投稿, last_retriggered=A != B → 再 handle 対象
    ///
    /// dedup なしだと Iter 2/3 で sleep_secs=0 となり数秒で max_retries を消費する。
    #[test]
    fn rate_limit_dedup_skips_repeated_comment() {
        let comment_a = "2026-04-30T00:00:00Z";
        let comment_b = "2026-04-30T00:30:00Z";

        // Iter 1: 初回 detection (last_retriggered=None)
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        let rl_a = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: comment_a.into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let already_handled_iter1 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_a.comment_event_time.as_str());
        assert!(
            !already_handled_iter1,
            "Iter 1: 初回 detection は handle されるべき"
        );

        // Iter 1 で handle した結果を simulate
        state.rate_limit_retries = 1;
        state.rate_limit_last_retriggered_at = Some(comment_a.into());

        // Iter 2: 同じ comment が PR に残っている (CR レビュー再開待ち)
        let already_handled_iter2 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_a.comment_event_time.as_str());
        assert!(
            already_handled_iter2,
            "Iter 2: 同じ comment は dedup で skip されるべき"
        );

        // Iter 3: CR が新たな rate-limit comment を投稿
        let rl_b = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: comment_b.into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let already_handled_iter3 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_b.comment_event_time.as_str());
        assert!(
            !already_handled_iter3,
            "Iter 3: 新 comment は再度 handle 対象"
        );
    }

    /// state.json round-trip で rate_limit_last_retriggered_at が persistence される。
    #[test]
    fn rate_limit_last_retriggered_at_persists_across_polls() {
        let tmp =
            std::env::temp_dir().join(format!("test-rl-last-handled-{}.json", std::process::id()));
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.rate_limit_last_retriggered_at = Some("2026-04-30T00:00:00Z".into());
        crate::state::write_state_to(&tmp, &state).unwrap();

        let loaded = crate::state::read_state_from(&tmp).unwrap();
        assert_eq!(
            loaded.rate_limit_last_retriggered_at.as_deref(),
            Some("2026-04-30T00:00:00Z")
        );

        let _ = std::fs::remove_file(&tmp);
    }

    /// Finding 2 (PR #97 round 3): sleep が監視残り予算を超える場合、
    /// `handle_rate_limit_retry` は Err を返し state を変更しない。
    #[test]
    fn rate_limit_retry_returns_err_when_sleep_exceeds_budget() {
        let future_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600; // 10 分後
        let rl = RateLimitState {
            until_unix_secs: future_unix,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 10,
            wait_seconds: 0,
        };
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
        };

        // remaining=60s なのに sleep=600s 必要 → Err
        let result = handle_rate_limit_retry(&rl, &mut state, &pr_info, 3, 60);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("監視残り予算"),
            "Err message に budget 不足の説明が含まれるべき: {}",
            err_msg
        );
        // state は変更されない (retries 0 のまま、last_retriggered_at None のまま)
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// Finding 3 (PR #97 round 3): PR 番号未確定の場合、Err を返し state を変更しない。
    #[test]
    fn rate_limit_retry_returns_err_when_pr_number_missing() {
        let past_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 60; // 1 分前 (sleep_secs=0 経路)
        let rl = RateLimitState {
            until_unix_secs: past_unix,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 0,
            wait_seconds: 0,
        };
        let mut state = PrMonitorState::new(None, None, "t".into());
        let pr_info = crate::util::PrInfo {
            pr_number: None,
            repo: None,
            push_time: None,
        };

        let result = handle_rate_limit_retry(&rl, &mut state, &pr_info, 3, 1000);
        assert!(result.is_err());
        // state は変更されない
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }
}
