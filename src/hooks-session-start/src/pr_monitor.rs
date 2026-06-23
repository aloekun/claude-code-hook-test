//! PR monitor catch-up (Bb-3 順位 55)。
//!
//! cli-pr-monitor の state file を読み、`next_wakeup_at_unix` が現在時刻以前
//! (= 待機時刻を過ぎている) なら `additionalContext` で Claude に手動再起動を促す。
//! 別プロセス spawn ではなく Claude に nudge する設計 (handle 継承や stdout
//! 可視性の問題を回避し、PARK signal flow を session 内に保つ)。

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// catch-up nudge で案内する手動再開コマンド。
/// pre-push-review (PR #115) 指摘 [B]: nudge 文字列のうちスクリプト名は const に切り出して
/// rename 時の drift を防ぐ。実際のコマンド実行ロジックは package.json (`scripts.push`) +
/// cli-push-runner にあり、本 const は表示用 hint のみ。
pub(crate) const RESUME_MONITORING_COMMAND: &str = "pnpm push --monitor-only";

/// cli-pr-monitor の state file から catch-up に必要な field のみ部分デシリアライズ。
/// 完全な PrMonitorState を別 crate から共有しないことで coupling を最小化する。
#[derive(Deserialize)]
pub(crate) struct ParkedStatePartial {
    pub(crate) pr: Option<u64>,
    pub(crate) repo: Option<String>,
    pub(crate) next_wakeup_at_unix: Option<i64>,
    pub(crate) wakeup_reason: Option<String>,
    /// 監視ステータス。`"parked_*"` (parked_rate_limit / parked_review_recheck) のみ
    /// catch-up nudge の対象。`"action_required"` 等の terminal 値では
    /// next_wakeup_at_unix が古い park 由来で残っていても nudge を抑制する。
    #[serde(default)]
    pub(crate) action: String,
}

/// cli-pr-monitor の state file パス (`<exe>/pr-monitor-state.json`)。
/// hooks-session-start.exe は cli-pr-monitor.exe と同じ `.claude/` 配下に配置される
/// 前提 (deploy:hooks スクリプトで保証)。
pub(crate) fn pr_monitor_state_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("pr-monitor-state.json")
}

/// `next_wakeup_at_unix` が現在時刻以前なら catch-up nudge メッセージを返す。
///
/// session が park 中に終了 (CronCreate 発火前) し、後で再開された場合、
/// CronCreate スケジュールが消えているため自動 wakeup は起こらない。
/// このとき手動で監視継続するための指示を Claude に渡す。
///
/// 返り値: Some(message) なら additionalContext に注入する文字列、None なら何もしない。
///
/// 抑制条件: action が `"parked_*"` でない (= terminal 状態) 場合、`next_wakeup_at_unix`
/// が残っていても false-positive nudge を出さない。terminal 経路では cli-pr-monitor が
/// `next_wakeup_at_unix` を明示クリアしないため、action ベースの guard が必要。
pub(crate) fn compute_catchup_nudge(
    state: &ParkedStatePartial,
    now_unix: i64,
) -> Option<String> {
    if !state.action.starts_with("parked_") {
        return None;
    }
    let wakeup_at = state.next_wakeup_at_unix?;
    if wakeup_at > now_unix {
        return None;
    }
    let pr = state
        .pr
        .map(|n| format!("#{}", n))
        .unwrap_or_else(|| "?".into());
    let repo = state.repo.as_deref().unwrap_or("?");
    let reason = state.wakeup_reason.as_deref().unwrap_or("unknown");
    Some(format!(
        "[PR_MONITOR_CATCHUP]\n\
         pending wakeup detected for PR {pr} ({repo}), reason={reason}, scheduled_at_unix={wakeup_at}, now={now_unix}.\n\
         CronCreate may have expired during session downtime. If the PR is still relevant, run `{cmd}` to resume monitoring.",
        cmd = RESUME_MONITORING_COMMAND
    ))
}

pub(crate) fn read_parked_state(path: &Path) -> Option<ParkedStatePartial> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parked_state(
        pr: Option<u64>,
        repo: Option<&str>,
        wakeup_at: Option<i64>,
        reason: Option<&str>,
        action: &str,
    ) -> ParkedStatePartial {
        ParkedStatePartial {
            pr,
            repo: repo.map(String::from),
            next_wakeup_at_unix: wakeup_at,
            wakeup_reason: reason.map(String::from),
            action: action.into(),
        }
    }

    #[test]
    fn catchup_nudge_none_when_no_wakeup_scheduled() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            None,
            Some("review_recheck"),
            "parked_review_recheck",
        );
        assert!(compute_catchup_nudge(&state, 1_775_088_000).is_none());
    }

    #[test]
    fn catchup_nudge_none_when_wakeup_in_future() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            Some("review_recheck"),
            "parked_review_recheck",
        );
        let now = 1_775_087_999;
        assert!(compute_catchup_nudge(&state, now).is_none());
    }

    #[test]
    fn catchup_nudge_emitted_when_wakeup_passed() {
        let state = parked_state(
            Some(42),
            Some("owner/repo"),
            Some(1_775_088_000),
            Some("review_recheck"),
            "parked_review_recheck",
        );
        let now = 1_775_088_001;
        let msg = compute_catchup_nudge(&state, now).expect("nudge should be emitted");
        assert!(msg.contains("[PR_MONITOR_CATCHUP]"));
        assert!(msg.contains("PR #42"));
        assert!(msg.contains("owner/repo"));
        assert!(msg.contains("review_recheck"));
        assert!(
            msg.contains(RESUME_MONITORING_COMMAND),
            "nudge は const RESUME_MONITORING_COMMAND を hint として埋め込むこと (pre-push-review #115 [B] 対策、コマンド名 rename 時に test が落ちて drift を catch)"
        );
    }

    #[test]
    fn catchup_nudge_handles_missing_optional_fields() {
        let state = parked_state(None, None, Some(0), None, "parked_review_recheck");
        let msg = compute_catchup_nudge(&state, 1).expect("nudge should still be emitted");
        assert!(msg.contains("PR ?"));
        assert!(msg.contains("(?)"));
        assert!(msg.contains("reason=unknown"));
    }

    /// terminal 経路では `next_wakeup_at_unix` が古い park 由来で残っていても
    /// false-positive nudge を出さない (advisor 指摘: 順位 55 review)。
    #[test]
    fn catchup_nudge_suppressed_for_terminal_action_required() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            Some("review_recheck"),
            "action_required",
        );
        let now = 1_775_088_001;
        assert!(
            compute_catchup_nudge(&state, now).is_none(),
            "terminal 経路 (action_required) では nudge を抑制すること"
        );
    }

    #[test]
    fn catchup_nudge_suppressed_for_continue_monitoring() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            None,
            "continue_monitoring",
        );
        let now = 1_775_088_001;
        assert!(compute_catchup_nudge(&state, now).is_none());
    }

    #[test]
    fn catchup_nudge_emitted_for_parked_rate_limit() {
        let state = parked_state(
            Some(42),
            Some("o/r"),
            Some(1_775_088_000),
            Some("rate_limit_retry"),
            "parked_rate_limit",
        );
        let msg = compute_catchup_nudge(&state, 1_775_088_001)
            .expect("parked_rate_limit should emit nudge");
        assert!(msg.contains("rate_limit_retry"));
    }

    #[test]
    fn read_parked_state_returns_none_when_file_missing() {
        let tmp = std::env::temp_dir().join(format!(
            "test-parked-state-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);
        assert!(read_parked_state(&tmp).is_none());
    }

    #[test]
    fn read_parked_state_parses_partial_fields() {
        let tmp = std::env::temp_dir().join(format!(
            "test-parked-state-partial-{}",
            std::process::id()
        ));
        let json = r#"{
            "pr": 42,
            "repo": "owner/repo",
            "started_at": "2026-05-01T00:00:00Z",
            "action": "parked_review_recheck",
            "summary": "...",
            "notified": false,
            "daemon_pid": null,
            "daemon_status": "active",
            "next_wakeup_at_unix": 1775088000,
            "wakeup_reason": "review_recheck"
        }"#;
        std::fs::write(&tmp, json).unwrap();

        let state = read_parked_state(&tmp).expect("should parse");
        assert_eq!(state.pr, Some(42));
        assert_eq!(state.repo.as_deref(), Some("owner/repo"));
        assert_eq!(state.next_wakeup_at_unix, Some(1_775_088_000));
        assert_eq!(state.wakeup_reason.as_deref(), Some("review_recheck"));
        assert_eq!(state.action, "parked_review_recheck");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn pr_monitor_state_path_ends_with_filename() {
        let path = pr_monitor_state_path();
        assert!(path.to_string_lossy().ends_with("pr-monitor-state.json"));
    }
}
