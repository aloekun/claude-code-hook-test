//! Review recheck PARK signal の formatting helper
//! (PR-W2 refactor で `review_recheck.rs` から signal 整形部分を切り出し)。
//!
//! - 順位 209 / 210 の安全 cron spec 生成 helper (`round_up_to_next_minute`,
//!   `compute_safe_minute_for_park_signal`)
//! - 順位 209 で導入された PARK signal format (`format_review_park_signal`)

use crate::state::PrMonitorState;

use super::PollContext;

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
fn compute_safe_minute_for_park_signal(wakeup_unix: i64) -> (i64, String) {
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
    let (safe_minute_unix, safe_minute_iso_utc) = compute_safe_minute_for_park_signal(wakeup_unix);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ClassifierConfig, RateLimitConfig};

    #[test]
    fn round_up_to_next_minute_keeps_value_when_seconds_already_zero() {
        let aligned = 1_775_044_800;
        assert_eq!(round_up_to_next_minute(aligned), aligned);
    }

    #[test]
    fn round_up_to_next_minute_rounds_up_when_seconds_present() {
        let unaligned = 1_775_044_819;
        assert_eq!(round_up_to_next_minute(unaligned), 1_775_044_860);
    }

    #[test]
    fn round_up_to_next_minute_rounds_up_one_second_before_next_minute() {
        let one_sec_before = 1_775_044_859;
        assert_eq!(round_up_to_next_minute(one_sec_before), 1_775_044_860);
    }

    #[test]
    fn round_up_to_next_minute_one_second_past_minute_rounds_up_to_next_full_minute() {
        let one_sec_past = 1_775_044_801;
        assert_eq!(round_up_to_next_minute(one_sec_past), 1_775_044_860);
    }

    #[test]
    fn round_up_to_next_minute_handles_zero_input_as_minute_zero() {
        assert_eq!(round_up_to_next_minute(0), 0);
    }

    #[test]
    fn compute_safe_minute_returns_sentinel_when_input_zero() {
        let (safe_unix, safe_iso) = compute_safe_minute_for_park_signal(0);
        assert_eq!(safe_unix, 0);
        assert_eq!(safe_iso, "?");
    }

    #[test]
    fn compute_safe_minute_returns_sentinel_when_input_negative() {
        let (safe_unix, safe_iso) = compute_safe_minute_for_park_signal(-1);
        assert_eq!(safe_unix, 0);
        assert_eq!(safe_iso, "?");
    }

    #[test]
    fn compute_safe_minute_rounds_up_and_formats_iso_when_input_unaligned() {
        let (safe_unix, safe_iso) = compute_safe_minute_for_park_signal(1_775_044_819);
        assert_eq!(safe_unix, 1_775_044_860);
        assert_eq!(safe_iso, "2026-04-01T12:01:00Z");
    }

    #[test]
    fn compute_safe_minute_preserves_iso_when_input_already_aligned() {
        let (safe_unix, safe_iso) = compute_safe_minute_for_park_signal(1_775_044_800);
        assert_eq!(safe_unix, 1_775_044_800);
        assert_eq!(safe_iso, "2026-04-01T12:00:00Z");
    }

    /// Bb-3 (順位 55): `max_review_rechecks` の config 化が実際に PARK signal に
    /// 反映されることを machine-enforce する (default 3 ではなく custom 値が出力されること)。
    #[test]
    fn format_review_park_signal_uses_configured_max_rechecks() {
        let state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker,
            push_time: "2026-05-01T00:00:00Z",
            fix_push_time: None,
            pr_info: &pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 120,
            review_recheck_wait_secs: 240,
            max_review_rechecks: 7,
        };

        let signal = format_review_park_signal(&state, &ctx);

        assert!(
            signal.contains("max_rechecks: 7"),
            "PARK signal に config 値 (max_rechecks: 7) が反映されること: {}",
            signal
        );
        assert!(
            !signal.contains("max_rechecks: 3"),
            "default 値 3 が hard-coded で残っていないこと: {}",
            signal
        );
    }

    #[test]
    fn format_review_park_signal_includes_safe_minute_iso_utc_field() {
        let mut state =
            PrMonitorState::new(Some(99), Some("o/r".into()), "2026-04-01T00:00:00Z".into());
        state.next_wakeup_at_unix = Some(1_775_044_819);
        let pr_info = crate::util::PrInfo {
            pr_number: Some(99),
            repo: Some("o/r".into()),
            push_time: Some("2026-04-01T00:00:00Z".into()),
            head_commit: None,
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker,
            push_time: "2026-04-01T00:00:00Z",
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

        let signal = format_review_park_signal(&state, &ctx);

        assert!(
            signal.contains("safe_minute_at_unix: 1775044860"),
            "PARK signal に safe_minute_at_unix の round-UP 値が含まれること: {}",
            signal
        );
        assert!(
            signal.contains("safe_minute_at_iso_utc: 2026-04-01T12:01:00Z"),
            "PARK signal に safe_minute_at_iso_utc の round-UP ISO が含まれること: {}",
            signal
        );
    }
}
