//! SessionStart hook — セッション ID を環境変数とファイルに伝播する + PR monitor catch-up
//!
//! Claude Code の SessionStart イベントで発火し、以下の3つの経路で session 起動準備を行う:
//!
//!   1. $CLAUDE_ENV_FILE に export 文を追記 → Bash ツールから参照可能
//!   2. .claude/.session-id ファイルに書き出し → 子プロセス (exe) から参照可能
//!   3. PR monitor catch-up (Bb-3 順位 55): cli-pr-monitor の state file を読み、
//!      `next_wakeup_at_unix` が現在時刻以前 (= 待機時刻を過ぎている) なら
//!      `additionalContext` で Claude に手動再起動を促すメッセージを差し込む。
//!      別プロセス spawn ではなく Claude に nudge する設計 (handle 継承や stdout
//!      可視性の問題を回避し、PARK signal flow を session 内に保つ)。
//!
//! .session-id ファイルは「同一 ID スキップ」方式:
//!   - 既に同じ session_id が書かれていれば何もしない (冪等)
//!   - 異なる ID (新セッション or サブセッション) の場合は上書きする

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

/// SessionStart hook の stdin JSON (必要なフィールドのみ)
#[derive(Deserialize)]
struct HookInput {
    session_id: Option<String>,
}

/// catch-up nudge で案内する手動再開コマンド。
/// pre-push-review (PR #115) 指摘 [B]: nudge 文字列のうちスクリプト名は const に切り出して
/// rename 時の drift を防ぐ。実際のコマンド実行ロジックは package.json (`scripts.push`) +
/// cli-push-runner にあり、本 const は表示用 hint のみ。
const RESUME_MONITORING_COMMAND: &str = "pnpm push --monitor-only";

/// cli-pr-monitor の state file から catch-up に必要な field のみ部分デシリアライズ。
/// 完全な PrMonitorState を別 crate から共有しないことで coupling を最小化する。
#[derive(Deserialize)]
struct ParkedStatePartial {
    pr: Option<u64>,
    repo: Option<String>,
    next_wakeup_at_unix: Option<i64>,
    wakeup_reason: Option<String>,
    /// 監視ステータス。`"parked_*"` (parked_rate_limit / parked_review_recheck) のみ
    /// catch-up nudge の対象。`"action_required"` 等の terminal 値では
    /// next_wakeup_at_unix が古い park 由来で残っていても nudge を抑制する。
    #[serde(default)]
    action: String,
}

/// session-id ファイルのパス (.claude/.session-id)
fn session_id_file_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(".session-id")
}

/// cli-pr-monitor の state file パス (`<exe>/pr-monitor-state.json`)。
/// hooks-session-start.exe は cli-pr-monitor.exe と同じ `.claude/` 配下に配置される
/// 前提 (deploy:hooks スクリプトで保証)。
fn pr_monitor_state_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("pr-monitor-state.json")
}

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
fn compute_catchup_nudge(state: &ParkedStatePartial, now_unix: i64) -> Option<String> {
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

fn read_parked_state(path: &Path) -> Option<ParkedStatePartial> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn main() {
    // stdin から JSON を読み取り
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        std::process::exit(0);
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => std::process::exit(0),
    };

    let session_id = match hook_input.session_id {
        Some(id) => {
            let trimmed = id.trim().to_string();
            if trimmed.is_empty() {
                std::process::exit(0);
            }
            trimmed
        }
        _ => std::process::exit(0),
    };

    // 1. $CLAUDE_ENV_FILE に追記 (Bash ツール用)
    // CLAUDE_ENV_FILE はセッションごとに異なるため、常に書き込む
    if let Ok(env_file) = std::env::var("CLAUDE_ENV_FILE") {
        write_to_env_file(&env_file, &session_id);
    }

    // 2. .claude/.session-id ファイルに書き出し (子プロセス exe 用)
    // 同一 ID スキップ方式: 既に同じ session_id が書き込み済みなら何もしない。
    // 異なる ID（新セッション or サブセッション）は上書きする。
    let sid_path = session_id_file_path();
    let should_write = match std::fs::read_to_string(&sid_path) {
        Ok(existing) => existing.trim() != session_id,
        Err(_) => true, // ファイルが存在しない
    };
    if should_write {
        let _ = std::fs::write(&sid_path, &session_id);
    }

    emit_session_start_output(&session_id);
}

/// `additionalContext` (session_id + 任意の PR monitor catch-up nudge) を組み立て
/// Claude Code に返す JSON を stdout に書き出す。
/// serde_json で組み立てることで session_id 内の特殊文字を安全にエスケープする。
fn emit_session_start_output(session_id: &str) {
    let mut context = format!("CLAUDE_CODE_SESSION_ID={}", session_id);
    if let Some(state) = read_parked_state(&pr_monitor_state_path()) {
        if let Some(nudge) = compute_catchup_nudge(&state, current_unix_secs()) {
            context.push_str("\n\n");
            context.push_str(&nudge);
        }
    }
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    });
    println!("{}", output);
}

/// シェル用シングルクォート (内部の ' を '\'' にエスケープ)
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// $CLAUDE_ENV_FILE に CLAUDE_CODE_SESSION_ID を追記する
/// 既に書き込み済みの場合はスキップ (resume/continue 対応)
fn write_to_env_file(env_file: &str, session_id: &str) {
    let marker = "CLAUDE_CODE_SESSION_ID";

    // 既に書き込み済みかチェック
    if let Ok(content) = std::fs::read_to_string(env_file) {
        if content.contains(marker) {
            return;
        }
    }

    // 追記 (シェルクォートで安全にエスケープ)
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env_file)
    {
        let _ = writeln!(f, "export {}={}", marker, shell_quote(session_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hook_input_with_session_id() {
        let json = r#"{"session_id": "abc-123", "hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, Some("abc-123".to_string()));
    }

    #[test]
    fn parse_hook_input_without_session_id() {
        let json = r#"{"hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, None);
    }

    #[test]
    fn session_id_file_path_ends_with_session_id() {
        let path = session_id_file_path();
        assert!(path.to_string_lossy().ends_with(".session-id"));
    }

    #[test]
    fn write_to_env_file_creates_and_writes() {
        let tmp = std::env::temp_dir().join(format!(
            "test-env-file-session-start-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);

        write_to_env_file(tmp.to_str().unwrap(), "test-session-123");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("CLAUDE_CODE_SESSION_ID"));
        assert!(content.contains("'test-session-123'")); // シングルクォート形式

        // 2回目の書き込みはスキップされる
        write_to_env_file(tmp.to_str().unwrap(), "different-id");
        let content2 = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, content2);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn whitespace_only_session_id_is_rejected() {
        let json = r#"{"session_id": "   ", "hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        // trim() すると空になる → main() では exit(0) される
        let trimmed = input.session_id.unwrap().trim().to_string();
        assert!(trimmed.is_empty());
    }

    #[test]
    fn shell_quote_simple() {
        assert_eq!(shell_quote("abc-123"), "'abc-123'");
    }

    #[test]
    fn shell_quote_with_single_quote() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn shell_quote_with_special_chars() {
        assert_eq!(shell_quote(r#"a"$b`c"#), r#"'a"$b`c'"#);
    }

    // --- .session-id 書き込みロジック (同一IDスキップ方式) ---

    #[test]
    fn session_id_file_new_file_is_written() {
        let tmp = std::env::temp_dir().join(format!("test-sid-new-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);

        // ファイルが存在しない → 書き込むべき
        let should_write = match std::fs::read_to_string(&tmp) {
            Ok(existing) => existing.trim() != "session-A",
            Err(_) => true,
        };
        assert!(should_write);
        let _ = std::fs::write(&tmp, "session-A");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-A");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn session_id_file_same_id_is_skipped() {
        let tmp = std::env::temp_dir().join(format!("test-sid-same-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "session-A");

        // 同じ ID → スキップ
        let existing = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(existing.trim(), "session-A");
        let should_write = existing.trim() != "session-A";
        assert!(!should_write);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn session_id_file_different_id_is_overwritten() {
        let tmp = std::env::temp_dir().join(format!("test-sid-diff-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "session-A");

        // 異なる ID → 上書き
        let existing = std::fs::read_to_string(&tmp).unwrap();
        let should_write = existing.trim() != "session-B";
        assert!(should_write);
        let _ = std::fs::write(&tmp, "session-B");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-B");

        let _ = std::fs::remove_file(&tmp);
    }

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
        let tmp =
            std::env::temp_dir().join(format!("test-parked-state-missing-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        assert!(read_parked_state(&tmp).is_none());
    }

    #[test]
    fn read_parked_state_parses_partial_fields() {
        let tmp =
            std::env::temp_dir().join(format!("test-parked-state-partial-{}", std::process::id()));
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

    #[test]
    fn session_id_file_empty_is_written() {
        let tmp = std::env::temp_dir().join(format!("test-sid-empty-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "");

        // 空ファイル → 書き込むべき ("" != "session-A")
        let existing = std::fs::read_to_string(&tmp).unwrap();
        let should_write = existing.trim() != "session-A";
        assert!(should_write);

        // 実際に書き込んで結果を検証
        let _ = std::fs::write(&tmp, "session-A");
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-A");

        let _ = std::fs::remove_file(&tmp);
    }
}
