//! Review recheck park 関連 (PR B refactor で `mod.rs` から切り出し)。
//!
//! - Bb-2 アーキで定義された review_recheck park 経路 (`finalize_initial_review_park`,
//!   `finalize_review_recheck_park`, `finalize_review_recheck_max_reached`,
//!   `schedule_next_review_recheck_park`)
//!
//! signal 整形部分 (`round_up_to_next_minute` / `compute_safe_minute_for_park_signal` /
//! `format_review_park_signal`) は `review_recheck_signal.rs` に分離。

use crate::log::log_info;
use crate::state::{read_state, write_state, PrMonitorState};

use super::rate_limit::make_action_required_result;
use super::review_recheck_signal::format_review_park_signal;
use super::{make_park_poll_result, PollContext, PollResult};

/// Bb-2: fresh push 経路で review_recheck park を行う (checker 呼び出しなし)。
///
/// 動機: push 直後は CR がまだ review を開始していない可能性が高く、即 check は wasteful。
/// `initial_review_wait_secs` 後に wakeup を予約 → 1 回 check という 2-step フローに分離する。
///
/// CR Major #2 fix (Bb-2 PR #114 review): 既存 state に残った `review_recheck_count` を
/// fresh push では 0 に明示リセット (前サイクルが MAX 到達等で残った count が新 push に
/// 持ち越されると summary "(initial wait, recheck=0/N)" と PARK signal の recheck_count が
/// 食い違い、最悪 max 到達状態で park される)。
/// CR Major #1 fix: head_commit を state に保存し detect_wakeup_resume の比較対象とする。
pub(super) fn finalize_initial_review_park(ctx: &PollContext<'_>) -> PollResult {
    let mut state = read_state().unwrap_or_else(|| {
        PrMonitorState::new(
            ctx.pr_info.pr_number,
            ctx.pr_info.repo.clone(),
            ctx.push_time.to_string(),
        )
    });
    state.pr = ctx.pr_info.pr_number;
    state.repo = ctx.pr_info.repo.clone();
    state.started_at = ctx.push_time.to_string();
    state.review_recheck_count = 0;
    state.head_commit = ctx.pr_info.head_commit.clone();
    state.fix_push_time = state
        .fix_push_time
        .or_else(|| ctx.fix_push_time.map(String::from));
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    state.next_wakeup_at_unix = Some(now_unix + ctx.initial_review_wait_secs as i64);
    state.wakeup_reason = Some("review_recheck".into());
    state.action = "parked_review_recheck".into();
    state.summary = format!(
        "review check を {}s 後に予約 (initial wait, recheck=0/{})",
        ctx.initial_review_wait_secs, ctx.max_review_rechecks
    );

    if let Err(e) = write_state(&state) {
        log_info(&format!(
            "[review_recheck] initial park state 永続化失敗、action_required で抜ける: {}",
            e
        ));
        return make_action_required_result(
            &state,
            &serde_json::Value::Null,
            &format!("review park の state 永続化失敗 ({})。手動確認が必要", e),
        );
    }

    println!("{}", format_review_park_signal(&state, ctx));
    make_park_poll_result(state)
}

/// Bb-2: wakeup 経路の review_recheck park (checker check 後に continue_monitoring の場合)。
///
/// `review_recheck_count` をインクリメントし、`max_review_rechecks` 到達なら
/// `action_required` で抜ける (review が想定時間内に未完了を通知)。
/// 未到達なら `review_recheck_wait_secs` 後の wakeup を予約して return。
pub(super) fn finalize_review_recheck_park(ctx: &PollContext<'_>) -> PollResult {
    let mut state = read_state().unwrap_or_else(|| {
        PrMonitorState::new(
            ctx.pr_info.pr_number,
            ctx.pr_info.repo.clone(),
            ctx.push_time.to_string(),
        )
    });
    state.review_recheck_count += 1;
    state.fix_push_time = state
        .fix_push_time
        .or_else(|| ctx.fix_push_time.map(String::from));

    if state.review_recheck_count >= ctx.max_review_rechecks {
        return finalize_review_recheck_max_reached(&mut state, ctx.max_review_rechecks);
    }

    schedule_next_review_recheck_park(&mut state, ctx)
}

fn finalize_review_recheck_max_reached(
    state: &mut PrMonitorState,
    max_review_rechecks: u32,
) -> PollResult {
    log_info(&format!(
        "[review_recheck] max {} 回到達、action_required で抜ける",
        max_review_rechecks
    ));
    let summary = format!(
        "review が想定時間内に完了せず ({} recheck 後)。手動で PR を確認してください",
        state.review_recheck_count
    );
    state.action = "action_required".into();
    state.summary = summary.clone();
    state.next_wakeup_at_unix = None;
    state.wakeup_reason = None;
    if let Err(e) = write_state(state) {
        log_info(&format!(
            "state 書き込み失敗 (action_required 確定後、続行): {}",
            e
        ));
    }
    make_action_required_result(state, &serde_json::Value::Null, &summary)
}

pub(super) fn schedule_next_review_recheck_park(
    state: &mut PrMonitorState,
    ctx: &PollContext<'_>,
) -> PollResult {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    state.next_wakeup_at_unix = Some(now_unix + ctx.review_recheck_wait_secs as i64);
    state.wakeup_reason = Some("review_recheck".into());
    state.action = "parked_review_recheck".into();
    state.head_commit = ctx.pr_info.head_commit.clone();
    state.summary = format!(
        "review check を {}s 後に予約 (recheck={}/{})",
        ctx.review_recheck_wait_secs, state.review_recheck_count, ctx.max_review_rechecks
    );

    if let Err(e) = write_state(state) {
        log_info(&format!(
            "[review_recheck] park state 永続化失敗、action_required で抜ける: {}",
            e
        ));
        return make_action_required_result(
            state,
            &serde_json::Value::Null,
            &format!("review park の state 永続化失敗 ({})。手動確認が必要", e),
        );
    }

    println!("{}", format_review_park_signal(state, ctx));
    make_park_poll_result(state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ClassifierConfig, RateLimitConfig};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// PR_MONITOR_STATE_FILE_OVERRIDE は process-global env var のため、
    /// override 設定 / 解除を test 並行実行で race させない serial guard。
    fn env_override_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    /// 書き込み先がディレクトリ不在のため write が必ず失敗する override path を返す。
    fn unwritable_state_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pr-monitor-T2-2-{}", std::process::id()))
            .join("nonexistent-dir")
            .join("state.json")
    }

    fn seed_stale_recheck_state(tmp_path: &std::path::Path) {
        let mut stale_state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        stale_state.review_recheck_count = 3;
        stale_state.action = "action_required".into();
        crate::state::write_state_to(tmp_path, &stale_state).unwrap();
    }

    fn pr_info_for_initial_review_park_test() -> crate::util::PrInfo {
        crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
            fix_push_time: None,
        }
    }

    fn make_default_test_ctx<'a>(
        checker: &'a std::path::Path,
        pr_info: &'a crate::util::PrInfo,
        rate_limit_config: &'a RateLimitConfig,
        classifier_config: &'a ClassifierConfig,
    ) -> PollContext<'a> {
        PollContext {
            checker,
            push_time: "2026-05-01T00:00:00Z",
            fix_push_time: None,
            pr_info,
            rate_limit_config,
            classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 300,
            review_recheck_wait_secs: 300,
            max_review_rechecks: 3,
        }
    }

    /// Finding #5: `finalize_review_recheck_max_reached` は `action_required` 確定後に
    /// 残留 wakeup fields を None にクリアする。ADR-030 invariant:
    /// "wakeup は parked_* action のときのみスケジュールされる"。
    #[test]
    fn finalize_review_recheck_max_reached_clears_wakeup_fields() {
        let _guard = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &state_path);

        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        let stale_wakeup_unix: i64 = 9_999_999_999;
        let stale_wakeup_reason = "review_recheck";
        state.next_wakeup_at_unix = Some(stale_wakeup_unix);
        state.wakeup_reason = Some(stale_wakeup_reason.into());
        state.review_recheck_count = 3;
        state.action = "parked_review_recheck".into();

        finalize_review_recheck_max_reached(&mut state, 3);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert!(
            state.next_wakeup_at_unix.is_none(),
            "Finding #5: action_required 確定時に next_wakeup_at_unix が None にクリアされること。実際: {:?}",
            state.next_wakeup_at_unix
        );
        assert!(
            state.wakeup_reason.is_none(),
            "Finding #5: action_required 確定時に wakeup_reason が None にクリアされること。実際: {:?}",
            state.wakeup_reason
        );
        assert_eq!(
            state.action, "action_required",
            "Finding #5: action が action_required に確定されること"
        );
    }

    /// Bb-2 (T2-2): `schedule_next_review_recheck_park` は write_state 失敗時に
    /// PARK signal emit を中止し `action_required` を返却する (sibling parity)。
    #[test]
    fn schedule_next_review_recheck_park_returns_action_required_when_write_state_fails() {
        let _guard = env_override_lock();
        let bad_path = unwritable_state_path();
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &bad_path);

        let mut state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        state.review_recheck_count = 1;
        let checker_path = std::path::PathBuf::from("dummy-checker");
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
            fix_push_time: None,
        };
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker_path,
            push_time: "2026-05-01T00:00:00Z",
            fix_push_time: None,
            pr_info: &pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 300,
            review_recheck_wait_secs: 300,
            max_review_rechecks: 3,
        };

        let outcome = schedule_next_review_recheck_park(&mut state, &ctx);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert_eq!(
            outcome.action, "action_required",
            "T2-2 sibling parity: review park も write_state 失敗 → action_required で抜けること"
        );
    }

    /// CR Major #2 fix (Bb-2 PR #114 review): fresh push 経路では `finalize_initial_review_park`
    /// が `review_recheck_count` を 0 に明示リセットすること。前サイクルが MAX 到達 (count=3)
    /// で残った state を持ち越さないことを machine-enforce する。
    #[test]
    fn finalize_initial_review_park_resets_recheck_count() {
        let _guard = env_override_lock();
        let tmp_path = std::env::temp_dir().join(format!(
            "pr-monitor-CR-M2-{}-state.json",
            std::process::id()
        ));
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &tmp_path);
        seed_stale_recheck_state(&tmp_path);

        let pr_info = pr_info_for_initial_review_park_test();
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = make_default_test_ctx(&checker, &pr_info, &rate_limit_config, &classifier_config);

        let outcome = finalize_initial_review_park(&ctx);
        let persisted = crate::state::read_state_from(&tmp_path).unwrap();

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");
        let _ = std::fs::remove_file(&tmp_path);

        assert_eq!(outcome.action, "parked_review_recheck");
        assert_eq!(
            persisted.review_recheck_count, 0,
            "CR Major #2: fresh push 経路で count=3 が残らず 0 にリセットされること"
        );
        assert_eq!(
            persisted.head_commit.as_deref(),
            Some("abc1234"),
            "CR Major #1: fresh push 経路で head_commit が pr_info から保存されること"
        );
    }

    /// 順位 141: `fix_push_time` の write-once 不変条件 —
    /// `finalize_initial_review_park` が state に既存の `fix_push_time` がある場合に
    /// `ctx.fix_push_time` の値で上書きしないことを検証する。
    ///
    /// `ctx.fix_push_time = Some("new_time")` (= None ではなく非 None) を使うことで、
    /// or_else 被演算子の入れ替えバグを discriminate できる。
    #[test]
    fn finalize_initial_review_park_preserves_existing_fix_push_time() {
        let _guard = env_override_lock();
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &state_path);

        let mut seeded =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        seeded.fix_push_time = Some("2026-05-22T06:06:00Z".into());
        crate::state::write_state_to(&state_path, &seeded).unwrap();

        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let mut ctx =
            make_default_test_ctx(&checker, &pr_info, &rate_limit_config, &classifier_config);
        let ctx_fix_push_time_must_lose = "2026-05-22T06:10:00Z";
        ctx.fix_push_time = Some(ctx_fix_push_time_must_lose);

        finalize_initial_review_park(&ctx);
        let persisted = crate::state::read_state_from(&state_path).unwrap();
        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert_eq!(
            persisted.fix_push_time.as_deref(),
            Some("2026-05-22T06:06:00Z"),
            "write-once: state に既存 fix_push_time がある場合、ctx の値で上書きしない"
        );
    }

    /// 順位 141: `fix_push_time` の write-once 不変条件 —
    /// `finalize_review_recheck_park` が state に既存の `fix_push_time` がある場合に
    /// `ctx.fix_push_time` の値で上書きしないことを検証する。
    #[test]
    fn finalize_review_recheck_park_preserves_existing_fix_push_time() {
        let _guard = env_override_lock();
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &state_path);

        let mut seeded =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        seeded.fix_push_time = Some("2026-05-22T06:06:00Z".into());
        seeded.review_recheck_count = 0;
        crate::state::write_state_to(&state_path, &seeded).unwrap();

        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let mut ctx =
            make_default_test_ctx(&checker, &pr_info, &rate_limit_config, &classifier_config);
        let ctx_fix_push_time_must_lose = "2026-05-22T06:10:00Z";
        ctx.fix_push_time = Some(ctx_fix_push_time_must_lose);

        finalize_review_recheck_park(&ctx);
        let persisted = crate::state::read_state_from(&state_path).unwrap();
        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert_eq!(
            persisted.fix_push_time.as_deref(),
            Some("2026-05-22T06:06:00Z"),
            "write-once: state に既存 fix_push_time がある場合、ctx の値で上書きしない"
        );
    }
}
