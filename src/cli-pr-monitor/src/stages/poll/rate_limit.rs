//! rate-limit 検出 branch と関連 helper。
//!
//! - `handle_rate_limit_branch` + `dispatch_rate_limit_outcome` (branch entry)
//! - `handle_rate_limit_retry` / `post_review_immediately` (retry logic)
//! - `RateLimitOutcome` (enum)
//! - `make_max_retries_result` / `make_action_required_result` (general result builders)
//! - `emit_shortcut_signal_if_eligible` / `fetch_mergeable_status` /
//!   `evaluate_rate_limit_shortcut` / `format_shortcut_signal` (順位 141 shortcut。旧
//!   `rate_limit_signal.rs` の PARK signal 整形部分は park モデルごと撤去したが、この
//!   shortcut は park の付随物ではなく「rate-limit 中でも既に mergeable なら即 merge を
//!   選べる」独立機能のため、terminal 化した `finalize_waiting_reset` に引き続き残す)
//!
//! WP-17 PR 3 (wakeup 廃止) で park モデルを撤去した。旧実装は reset 時刻が未来の場合に
//! state へ wakeup を書き PARK signal を出していたが、single-shot モデルでは
//! 「rate-limit 中である」ことを terminal に報告して終了する (`finalize_waiting_reset`)。
//! reset 後の再レビューは CodeRabbit の後続イベント → GitHub Actions 経路が処理する。

use std::path::Path;

use crate::config::RateLimitConfig;
use crate::log::log_info;
use crate::runner::run_gh_quiet;
use crate::state::{write_state_to, PrMonitorState};
use crate::util::PrInfo;

use super::PollResult;

/// rate-limit 検出 branch を集約する。
///
/// dedup: 同一 rate-limit comment は invocation を跨いで残るため `comment_event_time`
/// で dedup する。dedup なしでは即時 retrigger を繰り返し max_retries を浪費する。
/// CR が新たな rate-limit comment を投稿すると event_time が変わり再 handle 対象になる。
pub(super) fn handle_rate_limit_branch(
    state: &mut PrMonitorState,
    rate_limit_config: &RateLimitConfig,
    pr_info: &PrInfo,
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
        return Some(finalize_waiting_reset(
            state, &rl, pr_info, result, state_path,
        ));
    }

    if state.rate_limit_retries >= rate_limit_config.max_retries {
        log_info(&format!(
            "[rate_limit] max_retries={} 到達、自動 retry を停止",
            rate_limit_config.max_retries
        ));
        return Some(make_max_retries_result(state, result));
    }

    if !rate_limit_config.auto_retry_enabled {
        return Some(finalize_waiting_reset(
            state, &rl, pr_info, result, state_path,
        ));
    }

    Some(dispatch_rate_limit_outcome(
        state, &rl, pr_info, result, state_path,
    ))
}

fn dispatch_rate_limit_outcome(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    result: &serde_json::Value,
    state_path: &Path,
) -> PollResult {
    match handle_rate_limit_retry(rl, state, pr_info) {
        RateLimitOutcome::Posted => {
            finalize_posted_retrigger(state, rl, pr_info, result, state_path)
        }
        RateLimitOutcome::WaitingReset => {
            finalize_waiting_reset(state, rl, pr_info, result, state_path)
        }
        RateLimitOutcome::Failed(e) => {
            log_info(&format!("[rate_limit] retrigger 失敗: {}", e));
            make_action_required_result(
                state,
                result,
                &format!(
                    "rate-limit 自動 retry 失敗 ({})。手動で `@coderabbitai review` を投稿してください",
                    e
                ),
            )
        }
    }
}

/// retrigger を投稿した後の terminal 化。
///
/// 旧実装は「retrigger 後の review 完了待ち」を park していたが (順位 80 fix)、
/// single-shot モデルでは retrigger 済みであることを報告して終了する。
/// 再レビュー到着は GitHub Actions 経路が処理する。silent exit ではない点は
/// 旧実装と同じ (ADR-064)。
fn finalize_posted_retrigger(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    result: &serde_json::Value,
    state_path: &Path,
) -> PollResult {
    state.rate_limit_last_retriggered_at = Some(rl.comment_event_time.clone());
    state.record_head_commit(pr_info.head_commit.as_deref());
    state.action = "pending_review".into();
    state.summary = format!(
        "rate-limit へ retrigger を投稿 (retry={}/state 参照)。review 再実行の後続は GitHub Actions 経路が処理",
        state.rate_limit_retries
    );

    if let Err(e) = write_state_to(state_path, state) {
        log_info(&format!(
            "[rate_limit] retrigger 後の state 永続化失敗、自動 retry を停止: {}",
            e
        ));
        return make_action_required_result(
            state,
            result,
            &format!(
                "rate-limit retry 後の state 永続化に失敗 ({})。手動で `@coderabbitai review` の重複投稿に注意してください",
                e
            ),
        );
    }

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

/// reset 時刻が未来 (= いま retrigger しても弾かれる) 場合の terminal 化。
///
/// 「rate-limit 中でレビュー未実施」を loud に報告して終了する (ADR-064 (b):
/// レポート判定文の保留保証)。reset 後の再レビューは CodeRabbit の後続イベント →
/// GitHub Actions 経路が処理し、ローカルの時限 wakeup は使わない (WP-17 PR 3)。
///
/// state 書き込みの成否に関わらず、順位 141 の mergeable shortcut 判定
/// (`emit_shortcut_signal_if_eligible`) は実行する — shortcut は「今すぐ merge するか」
/// を stdout 経由でユーザーに問う独立した通知であり、この terminal 化自体の成否には
/// 依存しない。
///
/// **state 書き込み失敗時は fail-open** (log のみで続行し、通常どおり `rate_limited` を
/// 返す) — このパスは `finalize_posted_retrigger` と異なり `@coderabbitai review` 投稿
/// のような副作用を伴わないため、書き込み失敗を理由に checker 呼び出し済みの判定結果を
/// 破棄する必要がない (`finalize_pending_review` (mod.rs) と同じ fail-open 方針)。
/// head_commit の継続性 (`should_continue_state`) が失われるリスクは残るが、次回
/// `--monitor-only` 再実行時に fresh 初期化へ倒れるだけで、`finalize_posted_retrigger` の
/// 二重投稿リスクのような不可逆な問題にはならない (SIM-NEW-rate_limit-L158)。
fn finalize_waiting_reset(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    result: &serde_json::Value,
    state_path: &Path,
) -> PollResult {
    finalize_waiting_reset_with(state, rl, pr_info, result, state_path, fetch_mergeable_status)
}

/// [`finalize_waiting_reset`] の本体。mergeable 取得を注入可能にして shell (gh) 層を
/// テストから外す (CodeRabbit #353: テストが実 `gh` を起動していた。
/// `verify_diff_covers_pr_range` (cli-push-runner) と同じ注入の流儀)。
fn finalize_waiting_reset_with(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    result: &serde_json::Value,
    state_path: &Path,
    fetch_mergeable: impl FnOnce(&PrInfo) -> Option<MergeableStatus>,
) -> PollResult {
    state.action = "rate_limited".into();
    state.summary = format!(
        "CodeRabbit rate-limit 中 (残り約 {}m{}s)。reset 後の再レビューは GitHub Actions 経路が処理",
        rl.wait_minutes, rl.wait_seconds
    );
    state.record_head_commit(pr_info.head_commit.as_deref());
    if let Err(e) = write_state_to(state_path, state) {
        log_info(&format!(
            "state 書き込み失敗 (rate_limited 確定後、続行): {}",
            e
        ));
    }

    emit_shortcut_signal_if_eligible(state, rl, pr_info, fetch_mergeable);

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

/// 順位 141: rate-limit 検出 + mergeable CLEAN + CR 全フィールドクリーンの条件が揃ったとき
/// `[RATE_LIMIT_BUT_MERGEABLE]` signal を stdout に出力する shortcut path。
///
/// gh への問い合わせが失敗する / 条件を満たさない場合は何も出力しない (fail-safe: 通常の
/// `rate_limited` terminal 報告のみで完結し、shortcut 不在は機能低下であって障害ではない)。
fn emit_shortcut_signal_if_eligible(
    state: &PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    fetch_mergeable: impl FnOnce(&PrInfo) -> Option<MergeableStatus>,
) {
    let Some(mergeable) = fetch_mergeable(pr_info) else {
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
fn evaluate_rate_limit_shortcut(
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
fn format_shortcut_signal(
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
  B: reset 後の再レビュー到着を待つ (後続は GitHub Actions 経路のコメント、または reset 後の --monitor-only 再実行で把握する)
[/RATE_LIMIT_BUT_MERGEABLE]",
        merge = mergeable.mergeable,
        state = mergeable.merge_state,
    )
}

/// 順位 141: gh `pr view --json mergeable,mergeStateStatus` の結果を保持する DTO。
#[derive(Debug, Clone)]
struct MergeableStatus {
    mergeable: String,
    merge_state: String,
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

/// `handle_rate_limit_retry` の outcome 種別。
pub(crate) enum RateLimitOutcome {
    /// 即時 retrigger を投稿した (reset 時刻は既に過去だった)。
    Posted,
    /// reset 時刻が未来のため何もしない (terminal な rate_limited 報告へ)。
    WaitingReset,
    Failed(String),
}

/// rate-limit 検出時の outcome を返す。
pub(super) fn handle_rate_limit_retry(
    rl: &crate::state::RateLimitState,
    state: &mut PrMonitorState,
    pr_info: &PrInfo,
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
            "[rate_limit] reset まで {}秒 (wait={}m{}s + 60s buffer)、rate_limited として終了",
            sleep_secs, rl.wait_minutes, rl.wait_seconds
        ));
        return RateLimitOutcome::WaitingReset;
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
#[path = "rate_limit/tests.rs"]
mod tests;
