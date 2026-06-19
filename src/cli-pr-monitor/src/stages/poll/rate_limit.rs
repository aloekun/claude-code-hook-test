//! rate-limit 検出 branch と関連 helper (PR B refactor で `mod.rs` から切り出し)。
//!
//! - `handle_rate_limit_branch` + `dispatch_rate_limit_outcome` (branch entry)
//! - `finalize_posted_retrigger` / `finalize_parked` (state finalize)
//! - `emit_shortcut_signal_if_eligible` / `fetch_mergeable_status` /
//!   `evaluate_rate_limit_shortcut` / `format_shortcut_signal` (順位 141 shortcut)
//! - `handle_rate_limit_retry` / `post_review_immediately` (retry logic)
//! - `format_park_signal` (rate_limit_retry PARK signal)
//! - `MergeableStatus` / `RateLimitOutcome` (DTO/enum)
//! - `make_max_retries_result` / `make_action_required_result` (general result builders、
//!   review_recheck.rs からも参照される)

use crate::config::RateLimitConfig;
use crate::log::log_info;
use crate::runner::run_gh_quiet;
use crate::state::{write_state, PrMonitorState};
use crate::util::PrInfo;

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
    )
}

fn dispatch_rate_limit_outcome(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    max_retries: u32,
    review_recheck_wait_secs: u64,
    result: &serde_json::Value,
) -> Option<PollResult> {
    match handle_rate_limit_retry(rl, state, pr_info, max_retries) {
        RateLimitOutcome::Posted => finalize_posted_retrigger(
            state,
            rl,
            pr_info,
            review_recheck_wait_secs,
            max_retries,
            result,
        ),
        RateLimitOutcome::Parked { wakeup_at_unix } => Some(finalize_parked(
            state,
            rl,
            pr_info,
            wakeup_at_unix,
            max_retries,
            result,
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
    max_retries: u32,
    result: &serde_json::Value,
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

    if let Err(e) = write_state(state) {
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

    let signal = format_park_signal(state, rl, pr_info, max_retries);
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
) -> PollResult {
    state.action = "parked_rate_limit".into();
    state.next_wakeup_at_unix = Some(wakeup_at_unix);
    state.wakeup_reason = Some("rate_limit_retry".into());
    state.head_commit = pr_info.head_commit.clone();
    state.summary = format!(
        "CodeRabbit rate-limit: wakeup を {}m{}s 後に予約 (PARK signal 参照)",
        rl.wait_minutes, rl.wait_seconds
    );
    if let Err(e) = write_state(state) {
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

/// 順位 141: rate-limit 検出 + mergeable CLEAN + 未解決 thread なしの 3 条件が揃ったとき
/// `[RATE_LIMIT_BUT_MERGEABLE]` signal を stdout に出力する shortcut path。
fn emit_shortcut_signal_if_eligible(
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

/// 順位 141: mergeable + 未解決 thread の 3 条件評価を pure 関数化 (test 容易性)。
pub(super) fn evaluate_rate_limit_shortcut(
    coderabbit: Option<&crate::state::CodeRabbitState>,
    mergeable: &MergeableStatus,
) -> bool {
    let cr_clean = coderabbit
        .map(|c| c.unresolved_threads.unwrap_or(0) == 0)
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
        .unwrap_or_else(|_| "cli-pr-monitor.exe".into());
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
