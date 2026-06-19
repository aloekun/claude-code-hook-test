//! Review recheck park 関連 (PR B refactor で `mod.rs` から切り出し)。
//!
//! - 順位 209 / 210 の安全 cron spec 生成 helper (`round_up_to_next_minute`,
//!   `compute_safe_minute_for_park_signal`)
//! - 順位 209 で導入された PARK signal format (`format_review_park_signal`)
//! - Bb-2 アーキで定義された review_recheck park 経路 (`finalize_initial_review_park`,
//!   `finalize_review_recheck_park`, `finalize_review_recheck_max_reached`,
//!   `schedule_next_review_recheck_park`)

use crate::log::log_info;
use crate::state::{read_state, write_state, PrMonitorState};

use super::{
    make_action_required_result, make_park_poll_result, PollContext, PollResult,
};

/// 順位 209: PARK signal の cron spec round-UP rule (= Constraint 1)。
///
/// `unix_secs` の秒部分が `0` でなければ次の完全な分に round-UP した unix seconds を返す。
/// `~/.claude/rules/common/development-workflow.md` § Cron スケジューリングの秒 → 分 round-UP の
/// Constraint 1 (= scheduling minimum lead time) のみを実装。
///
/// Constraint 2 (= execution jitter ≤90s pre-fire / minute `:00`・`:30` 回避) は local TZ
/// awareness が必要で fractional-hour offset (例: IST +5:30) で正しく適用するには
/// AI agent consumer 側での処理が安全。本関数は UTC pure arithmetic に閉じる設計とし、
/// PARK signal の ACTION REQUIRED block で Step 2 として AI agent に明示する。
///
/// 由来: PR #210 セッション (2026-06-16) で実観測した cron timing race。秒解像度 timestamp を
/// 分単位 cron に round-DOWN 変換した結果、`should_resume_wakeup` が `wakeup_at > now` で false
/// 判定 → fresh path に倒れて recheck_count が前進せず、2 回の無駄 wakeup が発生した root cause。
pub(crate) fn round_up_to_next_minute(unix_secs: i64) -> i64 {
    let sec_in_minute = unix_secs.rem_euclid(60);
    if sec_in_minute == 0 {
        unix_secs
    } else {
        unix_secs - sec_in_minute + 60
    }
}

/// 順位 209: PARK signal 用に Constraint 1 (秒 → 分 round-UP) を適用した
/// safe minute の unix seconds と UTC ISO 8601 文字列を返す。
///
/// `wakeup_unix == 0` (未設定) のとき `(0, "?")` を返す sentinel 値を維持し、
/// `format_review_park_signal` 出力の "?" plain string 互換を保つ。
pub(super) fn compute_safe_minute_for_park_signal(wakeup_unix: i64) -> (i64, String) {
    if wakeup_unix <= 0 {
        return (0, "?".into());
    }
    let safe_unix = round_up_to_next_minute(wakeup_unix);
    let safe_iso = lib_pending_file::epoch_secs_to_iso8601(safe_unix as u64);
    (safe_unix, safe_iso)
}

struct ReviewParkSignalFields {
    safe_minute_unix: i64,
    safe_minute_iso_utc: String,
    pr: String,
    repo: String,
    wakeup_unix: i64,
    wakeup_iso: String,
    wait_secs: i64,
    exe: String,
    cwd: String,
    recheck: u32,
    max_rechecks: u32,
}

fn collect_review_park_fields(
    state: &PrMonitorState,
    ctx: &PollContext<'_>,
) -> ReviewParkSignalFields {
    let pr = ctx
        .pr_info
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let repo = ctx.pr_info.repo.clone().unwrap_or_else(|| "?".into());
    let wakeup_unix = state.next_wakeup_at_unix.unwrap_or(0);
    let wakeup_iso = if wakeup_unix > 0 {
        lib_pending_file::epoch_secs_to_iso8601(wakeup_unix as u64)
    } else {
        "?".into()
    };
    let (safe_minute_unix, safe_minute_iso_utc) =
        compute_safe_minute_for_park_signal(wakeup_unix);
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let wait_secs = (wakeup_unix - now_unix).max(0);
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "cli-pr-monitor.exe".into());
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into());

    ReviewParkSignalFields {
        safe_minute_unix,
        safe_minute_iso_utc,
        pr,
        repo,
        wakeup_unix,
        wakeup_iso,
        wait_secs,
        exe,
        cwd,
        recheck: state.review_recheck_count,
        max_rechecks: ctx.max_review_rechecks,
    }
}

/// Bb-2: 初回 push 後の review_recheck park signal を生成する。
///
/// `format_park_signal` (rate_limit_retry) と同じ envelope `[PR_MONITOR_PARK]` を使い、
/// `reason: review_recheck` で discriminate する。Claude Code 側のパーサは両 signal を
/// 同じ format で読める。
pub(super) fn format_review_park_signal(state: &PrMonitorState, ctx: &PollContext<'_>) -> String {
    let f = collect_review_park_fields(state, ctx);
    format!(
        "[PR_MONITOR_PARK]
reason: review_recheck
pr: {pr}
repo: {repo}
next_wakeup_at_unix: {wakeup_unix}
next_wakeup_at_iso_utc: {wakeup_iso}
safe_minute_at_unix: {safe_unix}
safe_minute_at_iso_utc: {safe_iso}
wait_total_seconds: {wait_secs}
recheck_count: {recheck}
max_rechecks: {max}
exe: {exe}
cwd: {cwd}

ACTION REQUIRED: please schedule one-shot wakeup using CronCreate.

Cron spec derivation (apply 2 orthogonal constraints sequentially):
  Step 1 (round-UP, already applied at the source): use `safe_minute_at_iso_utc`
         (= next_wakeup_at_iso_utc with seconds rounded UP to next full minute).
  Step 2 (avoid :00 / :30 minute due to 90s pre-fire jitter): convert
         `safe_minute_at_iso_utc` to LOCAL TZ, then bump the minute by +1 if it
         lands on :00 or :30. Use the resulting `HH:MM` as the cron field.
  Reference: ~/.claude/rules/common/development-workflow.md
             § Cron スケジューリングの秒 → 分 round-UP

CronCreate({{
  cron: \"<see Step 1 + Step 2 above>\",
  recurring: false,
  durable: true,
  prompt: \"Wakeup: review recheck for PR #{pr} ({repo}). cd \\\"{cwd}\\\" && \\\"{exe}\\\" --monitor-only\"
}})
[/PR_MONITOR_PARK]",
        pr = f.pr,
        repo = f.repo,
        wakeup_unix = f.wakeup_unix,
        wakeup_iso = f.wakeup_iso,
        safe_unix = f.safe_minute_unix,
        safe_iso = f.safe_minute_iso_utc,
        wait_secs = f.wait_secs,
        recheck = f.recheck,
        max = f.max_rechecks,
        exe = f.exe,
        cwd = f.cwd,
    )
}

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
    state.fix_push_time = ctx
        .fix_push_time
        .map(String::from)
        .or(state.fix_push_time);
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
    use super::finalize_review_recheck_max_reached;
    use crate::state::PrMonitorState;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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
}
