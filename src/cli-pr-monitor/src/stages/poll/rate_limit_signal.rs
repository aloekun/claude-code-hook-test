//! rate-limit park / shortcut signal の formatting helper
//! (PR-W2 refactor で `rate_limit.rs` から signal 整形部分を切り出し)。
//!
//! - `emit_shortcut_signal_if_eligible` / `fetch_mergeable_status` /
//!   `evaluate_rate_limit_shortcut` / `format_shortcut_signal` (順位 141 shortcut)
//! - `format_park_signal` (rate_limit_retry PARK signal)
//! - `collect_posted_retrigger_park_fields` / `format_posted_retrigger_review_park_signal`
//!   (rate-limit 解消後の review 待ち PARK signal)
//! - `MergeableStatus` / `PostedRetriggerParkFields` (DTO)

use crate::state::PrMonitorState;
use crate::util::PrInfo;

use crate::runner::run_gh_quiet;

use super::review_recheck_signal::round_up_to_next_minute;

/// 順位 141: rate-limit 検出 + mergeable CLEAN + CR 全フィールドクリーンの条件が揃ったとき
/// `[RATE_LIMIT_BUT_MERGEABLE]` signal を stdout に出力する shortcut path。
pub(super) fn emit_shortcut_signal_if_eligible(
    state: &PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
) {
    let Some(mergeable) = fetch_mergeable_status(pr_info) else {
        return;
    };
    if !evaluate_rate_limit_shortcut(state.coderabbit.as_ref(), &mergeable) {
        return;
    }
    println!("{}", format_shortcut_signal(rl, pr_info, &mergeable));
}

/// 順位 141: PR の mergeable / mergeStateStatus を gh で取得。失敗時は None。
fn fetch_mergeable_status(pr_info: &PrInfo) -> Option<MergeableStatus> {
    let pr = pr_info.pr_number?;
    let pr_str = pr.to_string();
    let mut args: Vec<&str> = vec![
        "pr",
        "view",
        &pr_str,
        "--json",
        "mergeable,mergeStateStatus",
    ];
    if let Some(repo) = pr_info.repo.as_deref() {
        args.push("--repo");
        args.push(repo);
    }
    let json_str = run_gh_quiet(&args)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    Some(MergeableStatus {
        mergeable: parsed.get("mergeable")?.as_str()?.to_string(),
        merge_state: parsed.get("mergeStateStatus")?.as_str()?.to_string(),
    })
}

/// 順位 141: mergeable + CR 全フィールドクリーンの条件評価を pure 関数化 (test 容易性)。
pub(super) fn evaluate_rate_limit_shortcut(
    coderabbit: Option<&crate::state::CodeRabbitState>,
    mergeable: &MergeableStatus,
) -> bool {
    let cr_clean = coderabbit
        .map(|c| {
            c.new_comments == 0
                && c.actionable_comments.unwrap_or(0) == 0
                && c.unresolved_threads.unwrap_or(0) == 0
        })
        .unwrap_or(true);
    mergeable.mergeable == "MERGEABLE" && mergeable.merge_state == "CLEAN" && cr_clean
}

/// 順位 141: `[RATE_LIMIT_BUT_MERGEABLE]` signal を構築 (pure)。
pub(super) fn format_shortcut_signal(
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    mergeable: &MergeableStatus,
) -> String {
    let pr = pr_info
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let repo = pr_info.repo.as_deref().unwrap_or("?");
    let reset_iso = if rl.until_unix_secs > 0 {
        lib_pending_file::epoch_secs_to_iso8601(rl.until_unix_secs as u64)
    } else {
        "?".into()
    };
    let wait_total_secs = rl.wait_minutes * 60 + rl.wait_seconds;
    format!(
        "[RATE_LIMIT_BUT_MERGEABLE]
pr: {pr}
repo: {repo}
rate_limit_reset_at_iso_utc: {reset_iso}
rate_limit_wait_seconds: {wait_total_secs}
mergeable: {merge}
merge_state: {state}

ACTION REQUIRED: ユーザーに以下 2 択を AskUserQuestion で問うこと:
  A: 今すぐ merge する (rate-limit reset を待たない、CR 2 回目 review なしで進める)
  B: reset を待って通常 auto-retry flow に乗る
[/RATE_LIMIT_BUT_MERGEABLE]",
        merge = mergeable.mergeable,
        state = mergeable.merge_state,
    )
}

/// 順位 141: gh `pr view --json mergeable,mergeStateStatus` の結果を保持する DTO。
#[derive(Debug, Clone)]
pub(crate) struct MergeableStatus {
    pub(crate) mergeable: String,
    pub(crate) merge_state: String,
}

struct PostedRetriggerParkFields {
    pr: String,
    repo: String,
    wakeup_unix: i64,
    wakeup_iso: String,
    safe_unix: i64,
    safe_iso: String,
    wait_secs: i64,
    recheck: u32,
    exe: String,
    cwd: String,
}

fn collect_posted_retrigger_park_fields(
    state: &PrMonitorState,
    pr_info: &PrInfo,
) -> PostedRetriggerParkFields {
    let pr = pr_info
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let repo = pr_info.repo.as_deref().unwrap_or("?").to_string();
    let wakeup_unix = state.next_wakeup_at_unix.unwrap_or(0);
    let wakeup_iso = if wakeup_unix > 0 {
        lib_pending_file::epoch_secs_to_iso8601(wakeup_unix as u64)
    } else {
        "?".into()
    };
    let safe_unix = if wakeup_unix > 0 {
        round_up_to_next_minute(wakeup_unix)
    } else {
        0
    };
    let safe_iso = if safe_unix > 0 {
        lib_pending_file::epoch_secs_to_iso8601(safe_unix as u64)
    } else {
        "?".into()
    };
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let wait_secs = (wakeup_unix - now_unix).max(0);
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| format!("cli-pr-monitor{}", std::env::consts::EXE_SUFFIX));
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into());
    PostedRetriggerParkFields {
        pr,
        repo,
        wakeup_unix,
        wakeup_iso,
        safe_unix,
        safe_iso,
        wait_secs,
        recheck: state.review_recheck_count,
        exe,
        cwd,
    }
}

/// rate-limit 解消後の `@coderabbitai review` 投稿完了 → review 待ち PARK signal を生成する。
///
/// `format_park_signal` (reason: rate_limit_retry, reset_at_unix 基準) とは異なり、
/// `state.next_wakeup_at_unix` の review 待ち wakeup を基準に `reason: review_recheck` で
/// discriminate する。過去 timestamp (`rl.until_unix_secs`) を CronCreate に誤使用しない。
pub(super) fn format_posted_retrigger_review_park_signal(
    state: &PrMonitorState,
    pr_info: &PrInfo,
) -> String {
    let PostedRetriggerParkFields {
        pr,
        repo,
        wakeup_unix,
        wakeup_iso,
        safe_unix,
        safe_iso,
        wait_secs,
        recheck,
        exe,
        cwd,
    } = collect_posted_retrigger_park_fields(state, pr_info);
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
[/PR_MONITOR_PARK]"
    )
}

/// PARK signal を stdout に書き出すための pure 関数 (Bb-1)。
pub(crate) fn format_park_signal(
    state: &PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    max_retries: u32,
) -> String {
    let pr = pr_info
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let repo = pr_info.repo.as_deref().unwrap_or("?");
    let reset_iso = if rl.until_unix_secs > 0 {
        lib_pending_file::epoch_secs_to_iso8601(rl.until_unix_secs as u64)
    } else {
        "?".into()
    };
    let wait_total_secs = rl.wait_minutes * 60 + rl.wait_seconds;
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| format!("cli-pr-monitor{}", std::env::consts::EXE_SUFFIX));
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into());
    let retry_attempt = state.rate_limit_retries + 1;

    format!(
        "[PR_MONITOR_PARK]
reason: rate_limit_retry
pr: {pr}
repo: {repo}
reset_at_unix: {until}
reset_at_iso_utc: {reset_iso}
wait_total_seconds: {wait_total_secs}
retry_count: {retry_attempt}
max_retries: {max_retries}
exe: {exe}
cwd: {cwd}

ACTION REQUIRED: please schedule one-shot wakeup using CronCreate.

CronCreate({{
  cron: \"<reset_at_iso_utc を local timezone の ISO 8601 形式に変換, e.g. 2024-01-15T09:30:00>\",
  recurring: false,
  durable: true,
  prompt: \"Wakeup: rate-limit retry for PR #{pr} ({repo}). cd \\\"{cwd}\\\" && \\\"{exe}\\\" --monitor-only\"
}})
[/PR_MONITOR_PARK]",
        until = rl.until_unix_secs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finding #3: rate-limit retrigger 後の PARK signal が `reason: review_recheck` を使い、
    /// 過去 timestamp (`rl.until_unix_secs`) ではなく `state.next_wakeup_at_unix` を参照する。
    #[test]
    fn format_posted_retrigger_review_park_signal_uses_review_recheck_reason() {
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.next_wakeup_at_unix = Some(1_775_044_800);
        state.review_recheck_count = 1;
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
            head_commit: None,
            fix_push_time: None,
        };

        let signal = format_posted_retrigger_review_park_signal(&state, &pr_info);

        assert!(
            signal.starts_with("[PR_MONITOR_PARK]"),
            "PARK signal ヘッダが正しい形式でない: {}",
            signal
        );
        assert!(
            signal.contains("reason: review_recheck"),
            "Finding #3: rate-limit retrigger 後も reason は review_recheck であるべき。実際: {}",
            signal
        );
        assert!(
            !signal.contains("reason: rate_limit_retry"),
            "Finding #3: rate_limit_retry は誤った reason (rate-limit PARK と混同)。実際: {}",
            signal
        );
        assert!(
            signal.contains("next_wakeup_at_unix: 1775044800"),
            "state.next_wakeup_at_unix を参照すべき (rl.until_unix_secs の過去 timestamp ではない)。実際: {}",
            signal
        );
    }

    /// Bb-1: PARK signal は CronCreate 呼び出しに必要な構造化情報を含む。
    #[test]
    fn format_park_signal_includes_required_fields() {
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.rate_limit_retries = 0;
        let rl = crate::state::RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "2026-05-01T00:00:00Z".into(),
            wait_minutes: 47,
            wait_seconds: 0,
            wait_time_parsed: true,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
            head_commit: None,
            fix_push_time: None,
        };

        let signal = format_park_signal(&state, &rl, &pr_info, 3);
        assert!(signal.starts_with("[PR_MONITOR_PARK]"));
        assert!(signal.contains("[/PR_MONITOR_PARK]"));
        assert!(signal.contains("pr: 42"));
        assert!(signal.contains("repo: o/r"));
        assert!(signal.contains("reset_at_unix: 1775088000"));
        assert!(signal.contains("wait_total_seconds: 2820"));
        assert!(signal.contains("retry_count: 1"));
        assert!(signal.contains("max_retries: 3"));
        assert!(signal.contains("CronCreate("));
        assert!(signal.contains("durable: true"));
        assert!(signal.contains("recurring: false"));
        assert!(signal.contains("--monitor-only"));
    }

    /// Bb-1: PR 番号 / repo が None でも format_park_signal は panic せず "?" を出す。
    #[test]
    fn format_park_signal_handles_missing_pr_info() {
        let state = PrMonitorState::new(None, None, "t".into());
        let rl = crate::state::RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "2026-05-01T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 30,
            wait_time_parsed: true,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: None,
            repo: None,
            push_time: None,
            head_commit: None,
            fix_push_time: None,
        };

        let signal = format_park_signal(&state, &rl, &pr_info, 3);
        assert!(signal.contains("pr: ?"));
        assert!(signal.contains("repo: ?"));
        assert!(signal.contains("wait_total_seconds: 330"));
    }

    /// 順位 141: shortcut signal の trigger 条件 (mergeable CLEAN + unresolved 0) で true。
    #[test]
    fn evaluate_rate_limit_shortcut_when_all_conditions_met() {
        let m = MergeableStatus {
            mergeable: "MERGEABLE".into(),
            merge_state: "CLEAN".into(),
        };
        let cr = crate::state::CodeRabbitState {
            review_state: "approved".into(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
        };
        assert!(evaluate_rate_limit_shortcut(Some(&cr), &m));
    }

    /// 順位 141: unresolved thread が残っていれば shortcut を抑止 (CR の指摘が未対応)。
    #[test]
    fn evaluate_rate_limit_shortcut_blocks_when_unresolved_threads_exist() {
        let m = MergeableStatus {
            mergeable: "MERGEABLE".into(),
            merge_state: "CLEAN".into(),
        };
        let cr = crate::state::CodeRabbitState {
            review_state: "commented".into(),
            new_comments: 1,
            actionable_comments: Some(1),
            unresolved_threads: Some(1),
        };
        assert!(!evaluate_rate_limit_shortcut(Some(&cr), &m));
    }

    /// 順位 141: new_comments > 0 のとき unresolved_threads が 0 でも shortcut を抑止。
    /// CR がまだコメントを処理中の状態で merge 判定を通過させない。
    #[test]
    fn evaluate_rate_limit_shortcut_blocks_when_new_comments_exist() {
        let m = MergeableStatus {
            mergeable: "MERGEABLE".into(),
            merge_state: "CLEAN".into(),
        };
        let cr = crate::state::CodeRabbitState {
            review_state: "commented".into(),
            new_comments: 1,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
        };
        assert!(!evaluate_rate_limit_shortcut(Some(&cr), &m));
    }

    /// 順位 141: mergeable が BLOCKED なら shortcut を抑止 (GitHub 側で merge 不可)。
    #[test]
    fn evaluate_rate_limit_shortcut_blocks_when_not_mergeable() {
        let m = MergeableStatus {
            mergeable: "BLOCKED".into(),
            merge_state: "BLOCKED".into(),
        };
        assert!(!evaluate_rate_limit_shortcut(None, &m));
    }

    /// 順位 141: CR state が None (初回 review なし) でも mergeable CLEAN なら shortcut 可。
    #[test]
    fn evaluate_rate_limit_shortcut_passes_when_coderabbit_none() {
        let m = MergeableStatus {
            mergeable: "MERGEABLE".into(),
            merge_state: "CLEAN".into(),
        };
        assert!(evaluate_rate_limit_shortcut(None, &m));
    }

    /// 順位 141: signal format に必須 field が全て含まれ、Claude が AskUserQuestion 化できる。
    #[test]
    fn format_shortcut_signal_includes_required_fields() {
        let rl = crate::state::RateLimitState {
            until_unix_secs: 1_779_432_672,
            comment_event_time: "2026-05-22T06:08:02Z".into(),
            wait_minutes: 38,
            wait_seconds: 30,
            wait_time_parsed: true,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(169),
            repo: Some("aloekun/claude-code-hook-test".into()),
            push_time: None,
            head_commit: None,
            fix_push_time: None,
        };
        let m = MergeableStatus {
            mergeable: "MERGEABLE".into(),
            merge_state: "CLEAN".into(),
        };
        let sig = format_shortcut_signal(&rl, &pr_info, &m);
        assert!(sig.starts_with("[RATE_LIMIT_BUT_MERGEABLE]"));
        assert!(sig.contains("[/RATE_LIMIT_BUT_MERGEABLE]"));
        assert!(sig.contains("pr: 169"));
        assert!(sig.contains("repo: aloekun/claude-code-hook-test"));
        assert!(sig.contains("rate_limit_wait_seconds: 2310"));
        assert!(sig.contains("mergeable: MERGEABLE"));
        assert!(sig.contains("merge_state: CLEAN"));
        assert!(sig.contains("AskUserQuestion"));
    }
}
