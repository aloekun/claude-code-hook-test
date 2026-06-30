//! rate-limit 検出 branch と関連 helper (PR B refactor で `mod.rs` から切り出し)。
//!
//! - `handle_rate_limit_branch` + `dispatch_rate_limit_outcome` (branch entry)
//! - `finalize_posted_retrigger` / `finalize_parked` (state finalize)
//! - `handle_rate_limit_retry` / `post_review_immediately` (retry logic)
//! - `RateLimitOutcome` (enum)
//! - `make_max_retries_result` / `make_action_required_result` (general result builders、
//!   review_recheck.rs からも参照される)
//!
//! signal 整形部分 (`format_park_signal` / shortcut signal /
//! `format_posted_retrigger_review_park_signal`) は `rate_limit_signal.rs` に分離。

use std::path::Path;

use crate::config::RateLimitConfig;
use crate::log::log_info;
use crate::runner::run_gh_quiet;
use crate::state::{write_state_to, PrMonitorState};
use crate::util::PrInfo;

use super::rate_limit_signal::{
    emit_shortcut_signal_if_eligible, format_park_signal,
    format_posted_retrigger_review_park_signal,
};
use super::{make_park_poll_result, PollResult};

/// rate-limit 検出 branch を集約する。
///
/// dedup: 同一 rate-limit comment は iteration を跨いで残るため `comment_event_time`
/// で dedup する。dedup なしでは即時 retrigger を秒単位で繰り返し max_retries を浪費する。
/// CR が新たな rate-limit comment を投稿すると event_time が変わり再 handle 対象になる。
pub(super) fn handle_rate_limit_branch(
    state: &mut PrMonitorState,
    rate_limit_config: &RateLimitConfig,
    pr_info: &PrInfo,
    review_recheck_wait_secs: u64,
    result: &serde_json::Value,
    state_path: &Path,
) -> Option<PollResult> {
    let rl = state.rate_limit.clone()?;
    let already_handled =
        state.rate_limit_last_retriggered_at.as_deref() == Some(rl.comment_event_time.as_str());

    if already_handled {
        log_info(&format!(
            "[rate_limit] 同じ rate-limit comment ({}) は処理済み、retrigger スキップ",
            rl.comment_event_time
        ));
        return None;
    }

    if state.rate_limit_retries >= rate_limit_config.max_retries {
        log_info(&format!(
            "[rate_limit] max_retries={} 到達、自動 retry を停止",
            rate_limit_config.max_retries
        ));
        return Some(make_max_retries_result(state, result));
    }

    if !rate_limit_config.auto_retry_enabled {
        return None;
    }

    dispatch_rate_limit_outcome(
        state,
        &rl,
        pr_info,
        rate_limit_config.max_retries,
        review_recheck_wait_secs,
        result,
        state_path,
    )
}

fn dispatch_rate_limit_outcome(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    max_retries: u32,
    review_recheck_wait_secs: u64,
    result: &serde_json::Value,
    state_path: &Path,
) -> Option<PollResult> {
    match handle_rate_limit_retry(rl, state, pr_info, max_retries) {
        RateLimitOutcome::Posted => finalize_posted_retrigger(
            state,
            rl,
            pr_info,
            review_recheck_wait_secs,
            result,
            state_path,
        ),
        RateLimitOutcome::Parked { wakeup_at_unix } => Some(finalize_parked(
            state,
            rl,
            pr_info,
            wakeup_at_unix,
            max_retries,
            result,
            state_path,
        )),
        RateLimitOutcome::Failed(e) => {
            log_info(&format!("[rate_limit] retrigger 失敗: {}", e));
            Some(make_action_required_result(
                state,
                result,
                &format!(
                    "rate-limit 自動 retry 失敗 ({})。手動で `@coderabbitai review` を投稿してください",
                    e
                ),
            ))
        }
    }
}

pub(super) fn finalize_posted_retrigger(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    review_recheck_wait_secs: u64,
    result: &serde_json::Value,
    state_path: &Path,
) -> Option<PollResult> {
    state.rate_limit_last_retriggered_at = Some(rl.comment_event_time.clone());

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let park_at_unix = now_unix + review_recheck_wait_secs as i64;

    state.action = "parked_review_recheck".into();
    state.next_wakeup_at_unix = Some(park_at_unix);
    state.wakeup_reason = Some("rate_limit_post_retrigger".into());
    state.head_commit = pr_info.head_commit.clone();
    state.summary = format!(
        "rate-limit retrigger 後の review 完了待ちを {}s 後に予約 (順位 80 fix: silent exit 防止)",
        review_recheck_wait_secs
    );

    if let Err(e) = write_state_to(state_path, state) {
        log_info(&format!(
            "[rate_limit] retrigger 後の state 永続化失敗、自動 retry を停止: {}",
            e
        ));
        return Some(make_action_required_result(
            state,
            result,
            &format!(
                "rate-limit retry 後の state 永続化に失敗 ({})。手動で `@coderabbitai review` の重複投稿に注意してください",
                e
            ),
        ));
    }

    let signal = format_posted_retrigger_review_park_signal(state, pr_info);
    println!("{}", signal);

    Some(make_park_poll_result(state.clone()))
}

pub(super) fn finalize_parked(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    wakeup_at_unix: i64,
    max_retries: u32,
    result: &serde_json::Value,
    state_path: &Path,
) -> PollResult {
    state.action = "parked_rate_limit".into();
    state.next_wakeup_at_unix = Some(wakeup_at_unix);
    state.wakeup_reason = Some("rate_limit_retry".into());
    state.head_commit = pr_info.head_commit.clone();
    state.summary = format!(
        "CodeRabbit rate-limit: wakeup を {}m{}s 後に予約 (PARK signal 参照)",
        rl.wait_minutes, rl.wait_seconds
    );
    if let Err(e) = write_state_to(state_path, state) {
        let msg = format!("park state 永続化失敗のため PARK signal を中止 ({})。手動で `@coderabbitai review` を投稿してください", e);
        return make_action_required_result(state, result, &msg);
    }
    let signal = format_park_signal(state, rl, pr_info, max_retries);
    println!("{}", signal);

    emit_shortcut_signal_if_eligible(state, rl, pr_info);

    PollResult {
        action: state.action.clone(),
        summary: state.summary.clone(),
        ci: state.ci.clone(),
        coderabbit: state.coderabbit.clone(),
        findings: state.findings.clone(),
        check_output: Some(result.clone()),
        rate_limit: state.rate_limit.clone(),
    }
}

fn make_max_retries_result(state: &PrMonitorState, result: &serde_json::Value) -> PollResult {
    let summary = format!(
        "CodeRabbit rate-limit が {} 回再試行後も継続。手動で `@coderabbitai review` を投稿してください",
        state.rate_limit_retries
    );
    make_action_required_result(state, result, &summary)
}

pub(super) fn make_action_required_result(
    state: &PrMonitorState,
    result: &serde_json::Value,
    summary: &str,
) -> PollResult {
    PollResult {
        action: "action_required".into(),
        summary: summary.into(),
        ci: state.ci.clone(),
        coderabbit: state.coderabbit.clone(),
        findings: state.findings.clone(),
        check_output: Some(result.clone()),
        rate_limit: state.rate_limit.clone(),
    }
}

/// `handle_rate_limit_retry` の outcome 種別 (Bb-1, Bundle b PR-1)。
pub(crate) enum RateLimitOutcome {
    Posted,
    Parked { wakeup_at_unix: i64 },
    Failed(String),
}

/// rate-limit 検出時の outcome を返す。
pub(super) fn handle_rate_limit_retry(
    rl: &crate::state::RateLimitState,
    state: &mut PrMonitorState,
    pr_info: &PrInfo,
    max_retries: u32,
) -> RateLimitOutcome {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let sleep_secs = (rl.until_unix_secs - now_unix).max(0) as u64;

    let Some(pr) = pr_info.pr_number else {
        return RateLimitOutcome::Failed("PR 番号未確定のため retrigger スキップ".into());
    };

    if sleep_secs > 0 {
        log_info(&format!(
            "[rate_limit] reset まで {}秒 (wait={}m{}s + 60s buffer)、Park で wakeup 要求 (retry 候補={}/{})",
            sleep_secs,
            rl.wait_minutes,
            rl.wait_seconds,
            state.rate_limit_retries + 1,
            max_retries
        ));
        return RateLimitOutcome::Parked {
            wakeup_at_unix: rl.until_unix_secs,
        };
    }

    post_review_immediately(pr, state)
}

/// `sleep_secs <= 0` 経路の即時 `@coderabbitai review` 投稿を担う helper。
fn post_review_immediately(pr: u64, state: &mut PrMonitorState) -> RateLimitOutcome {
    log_info(&format!(
        "[rate_limit] reset 時刻は既に過去、即時 retrigger (retry={})",
        state.rate_limit_retries + 1
    ));

    let pr_str = pr.to_string();
    if run_gh_quiet(&["pr", "comment", &pr_str, "--body", "@coderabbitai review"]).is_none() {
        return RateLimitOutcome::Failed(format!("gh pr comment 投稿失敗 (PR #{})", pr));
    }

    log_info(&format!(
        "[rate_limit] @coderabbitai review を投稿 (PR #{}, retry={})",
        pr,
        state.rate_limit_retries + 1
    ));

    state.rate_limit_retries += 1;
    RateLimitOutcome::Posted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RateLimitState;

    #[test]
    fn rate_limit_state_persists_retries_across_polls() {
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
        assert!(2 < cfg.max_retries);
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

        state.rate_limit_retries = 1;
        state.rate_limit_last_retriggered_at = Some(comment_a.into());

        let already_handled_iter2 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_a.comment_event_time.as_str());
        assert!(
            already_handled_iter2,
            "Iter 2: 同じ comment は dedup で skip されるべき"
        );

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

    /// Bb-1: reset 時刻が未来の場合、`handle_rate_limit_retry` は Parked を返し
    /// state.rate_limit_retries を変更しない (実 retry 計上は wakeup 経由で post 投稿後)。
    #[test]
    fn rate_limit_retry_returns_parked_when_reset_in_future() {
        let future_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
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
            head_commit: None,
            fix_push_time: None,
        };

        let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info, 3);
        match outcome {
            RateLimitOutcome::Parked { wakeup_at_unix } => {
                assert_eq!(wakeup_at_unix, future_unix);
            }
            _ => panic!("expected Parked outcome for future reset, got other variant"),
        }
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// Bb-1: PR 番号未確定の場合、`handle_rate_limit_retry` は Failed を返し
    /// state を変更しない (caller は action_required で抜ける)。
    #[test]
    fn rate_limit_retry_returns_failed_when_pr_number_missing() {
        let past_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 60;
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
            head_commit: None,
            fix_push_time: None,
        };

        let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info, 3);
        assert!(matches!(outcome, RateLimitOutcome::Failed(_)));
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// 書き込み先がディレクトリ不在のため write が必ず失敗する path を返す。
    fn unwritable_state_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pr-monitor-T2-2-{}", std::process::id()))
            .join("nonexistent-dir")
            .join("state.json")
    }

    /// Bb-1 (T2-2): `finalize_parked` は write_state 失敗時に PARK signal emit を中止し
    /// `action_required` を返却する fail-safe 経路を持つ (CodeRabbit Major #1 fix の固定化)。
    #[test]
    fn finalize_parked_returns_action_required_when_write_state_fails() {
        let bad_path = unwritable_state_path();

        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        let rl = RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "2026-05-01T00:00:00Z".into(),
            wait_minutes: 47,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
            head_commit: None,
            fix_push_time: None,
        };
        let result = serde_json::json!({});

        let outcome = finalize_parked(
            &mut state,
            &rl,
            &pr_info,
            1_775_088_000,
            3,
            &result,
            &bad_path,
        );

        assert_eq!(
            outcome.action, "action_required",
            "T2-2: write_state 失敗 → action_required で抜ける fail-safe が必要"
        );
        assert!(
            outcome.summary.contains("PARK signal を中止")
                || outcome.summary.contains("永続化失敗"),
            "summary に永続化失敗の説明が含まれること: {}",
            outcome.summary
        );
    }

    fn setup_posted_retrigger_fixture() -> (PrMonitorState, RateLimitState, crate::util::PrInfo) {
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.action = "continue_monitoring".into();
        state.rate_limit_retries = 1;
        let rl = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: "2026-05-08T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
            fix_push_time: None,
        };
        (state, rl, pr_info)
    }

    #[test]
    fn finalize_posted_retrigger_schedules_park_after_post() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");

        let (mut state, rl, pr_info) = setup_posted_retrigger_fixture();
        let result = finalize_posted_retrigger(
            &mut state,
            &rl,
            &pr_info,
            300,
            &serde_json::Value::Null,
            &state_path,
        );

        let park_result =
            result.expect("順位 80 fix: Posted 後は必ず park を返し silent exit を防ぐ");
        assert_eq!(park_result.action, "parked_review_recheck");
        assert_eq!(
            state.wakeup_reason.as_deref(),
            Some("rate_limit_post_retrigger")
        );
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let wakeup = state
            .next_wakeup_at_unix
            .expect("next_wakeup_at_unix が設定される");
        assert!(wakeup > now_unix && wakeup <= now_unix + 301);
        assert_eq!(
            state.rate_limit_last_retriggered_at.as_deref(),
            Some("2026-05-08T00:00:00Z")
        );
    }

    #[test]
    fn finalize_posted_retrigger_action_required_when_write_state_fails() {
        let bad_path = unwritable_state_path();

        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.action = "continue_monitoring".into();
        let rl = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: "2026-05-08T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
            fix_push_time: None,
        };

        let result = finalize_posted_retrigger(
            &mut state,
            &rl,
            &pr_info,
            300,
            &serde_json::Value::Null,
            &bad_path,
        );

        assert!(result.is_some());
        assert_eq!(
            result.unwrap().action,
            "action_required",
            "write_state 失敗時は action_required で抜ける (sibling parity with finalize_parked)"
        );
    }
}
