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
    state.head_commit = pr_info.head_commit.clone();
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
fn finalize_waiting_reset(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    result: &serde_json::Value,
    state_path: &Path,
) -> PollResult {
    state.action = "rate_limited".into();
    state.summary = format!(
        "CodeRabbit rate-limit 中 (残り約 {}m{}s)。reset 後の再レビューは GitHub Actions 経路が処理",
        rl.wait_minutes, rl.wait_seconds
    );
    state.head_commit = pr_info.head_commit.clone();
    if let Err(e) = write_state_to(state_path, state) {
        log_info(&format!(
            "state 書き込み失敗 (rate_limited 確定後、続行): {}",
            e
        ));
    }

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

/// 順位 141: rate-limit 検出 + mergeable CLEAN + CR 全フィールドクリーンの条件が揃ったとき
/// `[RATE_LIMIT_BUT_MERGEABLE]` signal を stdout に出力する shortcut path。
///
/// gh への問い合わせが失敗する / 条件を満たさない場合は何も出力しない (fail-safe: 通常の
/// `rate_limited` terminal 報告のみで完結し、shortcut 不在は機能低下であって障害ではない)。
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
mod tests {
    use super::*;
    use crate::state::RateLimitState;

    /// pr_number: None の `PrInfo` — `emit_shortcut_signal_if_eligible` を早期 return させ、
    /// テストから実 gh CLI 呼び出し (ネットワーク依存) を発生させないための fixture。
    fn pr_info_without_shortcut() -> crate::util::PrInfo {
        crate::util::PrInfo {
            pr_number: None,
            repo: None,
            push_time: None,
            head_commit: None,
            fix_push_time: None,
        }
    }

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
    }

    /// 同じ rate-limit comment が invocation 跨ぎで残った場合に dedup が働くことを検証する。
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
        assert!(!already_handled_iter1, "初回 detection は handle されるべき");

        state.rate_limit_retries = 1;
        state.rate_limit_last_retriggered_at = Some(comment_a.into());

        let already_handled_iter2 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_a.comment_event_time.as_str());
        assert!(
            already_handled_iter2,
            "同じ comment は dedup で skip されるべき"
        );

        let rl_b = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: comment_b.into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let already_handled_iter3 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_b.comment_event_time.as_str());
        assert!(!already_handled_iter3, "新 comment は再度 handle 対象");
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

    /// PR 3 (wakeup 廃止): reset 時刻が未来の場合、`handle_rate_limit_retry` は
    /// WaitingReset を返し state.rate_limit_retries を変更しない (旧 Parked 相当。
    /// wakeup 時刻は返さない — park しないため不要)。
    #[test]
    fn rate_limit_retry_returns_waiting_reset_when_reset_in_future() {
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

        let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info);
        assert!(matches!(outcome, RateLimitOutcome::WaitingReset));
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// PR 番号未確定の場合、`handle_rate_limit_retry` は Failed を返し
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

        let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info);
        assert!(matches!(outcome, RateLimitOutcome::Failed(_)));
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// PR 3 (wakeup 廃止): reset 未来の rate-limit は terminal な `rate_limited` action で
    /// 報告される。summary は保留 (レビュー未実施) と後続の引き継ぎ先を明示する
    /// (ADR-064 (b): レポート判定文の保留保証を Actions 経路へ引き継ぐまでの間も維持)。
    #[test]
    fn finalize_waiting_reset_reports_rate_limited_terminal() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.rate_limit = Some(RateLimitState {
            until_unix_secs: 9_999_999_999,
            comment_event_time: "2026-08-03T00:00:00Z".into(),
            wait_minutes: 47,
            wait_seconds: 10,
        });
        let rl = state.rate_limit.clone().unwrap();
        let pr_info = pr_info_without_shortcut();

        let outcome = finalize_waiting_reset(
            &mut state,
            &rl,
            &pr_info,
            &serde_json::Value::Null,
            &state_path,
        );

        assert_eq!(outcome.action, "rate_limited");
        assert!(
            outcome.summary.contains("rate-limit 中"),
            "レビュー未実施であることを明示: {}",
            outcome.summary
        );
        assert!(
            outcome.summary.contains("GitHub Actions"),
            "後続の引き継ぎ先を明示: {}",
            outcome.summary
        );
        assert!(
            outcome.rate_limit.is_some(),
            "rate_limit を伝播し caller (monitor.rs) が takt invoke を skip できること (#C-3)"
        );
        let persisted = crate::state::read_state_from(&state_path).unwrap();
        assert_eq!(persisted.action, "rate_limited");
    }

    /// state-continuity-drop 回帰 (SIM-NEW-rate_limit-L154): `finalize_waiting_reset` は
    /// state.head_commit を pr_info から persist する。これが欠けると
    /// `should_continue_state` (monitor.rs) が次回 `--monitor-only` 再実行時に
    /// legacy state (head_commit None) 扱いで fresh 初期化に倒れ、push_time /
    /// fix_push_time が「今」にリセットされて間に届いた CR コメントを取りこぼす。
    #[test]
    fn finalize_waiting_reset_persists_head_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.rate_limit = Some(RateLimitState {
            until_unix_secs: 9_999_999_999,
            comment_event_time: "2026-08-03T00:00:00Z".into(),
            wait_minutes: 47,
            wait_seconds: 10,
        });
        let rl = state.rate_limit.clone().unwrap();
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
            head_commit: Some("deadbeef".into()),
            fix_push_time: None,
        };

        let outcome = finalize_waiting_reset(
            &mut state,
            &rl,
            &pr_info,
            &serde_json::Value::Null,
            &state_path,
        );

        assert_eq!(outcome.action, "rate_limited");
        assert_eq!(state.head_commit.as_deref(), Some("deadbeef"));
        let persisted = crate::state::read_state_from(&state_path).unwrap();
        assert_eq!(
            persisted.head_commit.as_deref(),
            Some("deadbeef"),
            "head_commit が persist されないと should_continue_state が次回 fresh 初期化に倒れる"
        );
    }

    /// state-continuity-drop 回帰 (SIM-NEW-rate_limit-L154): `finalize_posted_retrigger` も
    /// 同様に head_commit を persist する (retrigger 投稿後の継続判定も同じ invariant に従う)。
    #[test]
    fn finalize_posted_retrigger_persists_head_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        let rl = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: "2026-08-03T00:00:00Z".into(),
            wait_minutes: 0,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
            head_commit: Some("cafef00d".into()),
            fix_push_time: None,
        };

        let outcome =
            finalize_posted_retrigger(&mut state, &rl, &pr_info, &serde_json::Value::Null, &state_path);

        assert_eq!(outcome.action, "pending_review");
        assert_eq!(state.head_commit.as_deref(), Some("cafef00d"));
        let persisted = crate::state::read_state_from(&state_path).unwrap();
        assert_eq!(
            persisted.head_commit.as_deref(),
            Some("cafef00d"),
            "head_commit が persist されないと should_continue_state が次回 fresh 初期化に倒れる"
        );
    }

    /// dedup 済み (同一 comment 処理済み) の rate-limit も terminal な rate_limited に
    /// 落ちること — 旧実装はここで None を返し park に流れていた。
    #[test]
    fn handle_rate_limit_branch_terminalizes_already_handled_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.rate_limit = Some(RateLimitState {
            until_unix_secs: 9_999_999_999,
            comment_event_time: "2026-08-03T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 0,
        });
        state.rate_limit_last_retriggered_at = Some("2026-08-03T00:00:00Z".into());
        let pr_info = pr_info_without_shortcut();

        let outcome = handle_rate_limit_branch(
            &mut state,
            &RateLimitConfig::default(),
            &pr_info,
            &serde_json::Value::Null,
            &state_path,
        );

        let terminal = outcome.expect("rate-limit active なら必ず terminal を返す (park しない)");
        assert_eq!(terminal.action, "rate_limited");
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
        let rl = RateLimitState {
            until_unix_secs: 1_779_432_672,
            comment_event_time: "2026-05-22T06:08:02Z".into(),
            wait_minutes: 38,
            wait_seconds: 30,
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
