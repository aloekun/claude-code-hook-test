//! SessionStart hook — セッション ID を環境変数とファイルに伝播する + PR monitor catch-up
//!
//! Claude Code の SessionStart イベントで発火し、以下の経路で session 起動準備を行う:
//!
//!   1. $CLAUDE_ENV_FILE に export 文を追記 → Bash ツールから参照可能
//!   2. .claude/.session-id ファイルに書き出し → 子プロセス (exe) から参照可能
//!   3. PR monitor catch-up: `pr_monitor` module
//!   4. Orphan run reaper (ADR-030 §L2): `reaper` module
//!   5. Working copy staleness nudge: `staleness` module
//!   6. Weekly review reminder (ADR-031 Phase C): `weekly_review` module
//!
//! .session-id ファイルは「同一 ID スキップ」方式:
//!   - 既に同じ session_id が書かれていれば何もしない (冪等)
//!   - 異なる ID (新セッション or サブセッション) の場合は上書きする

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

mod hooks_config;
mod jj_helpers;
mod past_time;
mod pr_monitor;
mod reaper;
mod staleness;
mod weekly_review;

use hooks_config::read_hooks_config;
use pr_monitor::{compute_catchup_nudge, pr_monitor_state_path, read_parked_state};
use reaper::compute_reaper_nudge;
use staleness::compute_staleness_nudge;
use weekly_review::compute_weekly_review_reminder_nudge;

/// SessionStart hook の stdin JSON (必要なフィールドのみ)
#[derive(Deserialize)]
struct HookInput {
    session_id: Option<String>,
}

/// session-id ファイルのパス (.claude/.session-id)
fn session_id_file_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(".session-id")
}

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn read_stdin_hook_input() -> Option<HookInput> {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return None;
    }
    serde_json::from_str(&input).ok()
}

fn extract_non_empty_session_id(hook_input: HookInput) -> Option<String> {
    let id = hook_input.session_id?;
    let trimmed = id.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn propagate_session_id_to_env_file(session_id: &str) {
    if let Ok(env_file) = std::env::var("CLAUDE_ENV_FILE") {
        write_to_env_file(&env_file, session_id);
    }
}

fn persist_session_id_to_file(session_id: &str) {
    let sid_path = session_id_file_path();
    let should_write = match std::fs::read_to_string(&sid_path) {
        Ok(existing) => existing.trim() != session_id,
        Err(_) => true,
    };
    if should_write {
        let _ = std::fs::write(&sid_path, session_id);
    }
}

fn main() {
    let Some(hook_input) = read_stdin_hook_input() else {
        std::process::exit(0);
    };
    let Some(session_id) = extract_non_empty_session_id(hook_input) else {
        std::process::exit(0);
    };

    propagate_session_id_to_env_file(&session_id);
    persist_session_id_to_file(&session_id);

    emit_session_start_output(&session_id);
}

/// `additionalContext` (session_id + 任意の PR monitor catch-up nudge + 任意の reaper nudge) を
/// 組み立て、Claude Code に返す JSON を stdout に書き出す。
/// serde_json で組み立てることで session_id 内の特殊文字を安全にエスケープする。
fn emit_session_start_output(session_id: &str) {
    let mut context = format!("CLAUDE_CODE_SESSION_ID={}", session_id);
    let now_unix = current_unix_secs();
    if let Some(state) = read_parked_state(&pr_monitor_state_path()) {
        if let Some(nudge) = compute_catchup_nudge(&state, now_unix) {
            context.push_str("\n\n");
            context.push_str(&nudge);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(reaper_nudge) = compute_reaper_nudge(&cwd, now_unix) {
            context.push_str("\n\n");
            context.push_str(&reaper_nudge);
        }
        let hooks_config = read_hooks_config(&cwd);
        if let Some(staleness_config) = hooks_config
            .session_start
            .as_ref()
            .and_then(|s| s.staleness.as_ref())
        {
            if let Some(staleness_nudge) = compute_staleness_nudge(&cwd, staleness_config) {
                context.push_str("\n\n");
                context.push_str(&staleness_nudge);
            }
        }
        if let Some(weekly_config) = hooks_config
            .session_start
            .as_ref()
            .and_then(|s| s.weekly_review_reminder.as_ref())
        {
            if let Some(weekly_nudge) =
                compute_weekly_review_reminder_nudge(&cwd, weekly_config, now_unix)
            {
                context.push_str("\n\n");
                context.push_str(&weekly_nudge);
            }
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

    if let Ok(content) = std::fs::read_to_string(env_file) {
        if content.contains(marker) {
            return;
        }
    }

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
    fn write_to_env_file_creates_and_writes_with_shell_quote() {
        let tmp = std::env::temp_dir().join(format!(
            "test-env-file-session-start-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);

        write_to_env_file(tmp.to_str().unwrap(), "test-session-123");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("CLAUDE_CODE_SESSION_ID"));
        assert!(content.contains("'test-session-123'"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn write_to_env_file_skips_second_write_for_same_marker() {
        let tmp = std::env::temp_dir().join(format!("test-env-file-skip-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);

        write_to_env_file(tmp.to_str().unwrap(), "first");
        let first = std::fs::read_to_string(&tmp).unwrap();

        write_to_env_file(tmp.to_str().unwrap(), "second");
        let second = std::fs::read_to_string(&tmp).unwrap();

        assert_eq!(first, second);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn extract_non_empty_session_id_rejects_whitespace_only() {
        let json = r#"{"session_id": "   ", "hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(extract_non_empty_session_id(input).is_none());
    }

    #[test]
    fn extract_non_empty_session_id_accepts_trimmed_value() {
        let json = r#"{"session_id": "  abc-123  ", "hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            extract_non_empty_session_id(input),
            Some("abc-123".to_string())
        );
    }

    #[test]
    fn extract_non_empty_session_id_returns_none_when_field_missing() {
        let json = r#"{"hook_event_name": "SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(extract_non_empty_session_id(input).is_none());
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

    #[test]
    fn session_id_file_new_file_is_written() {
        let tmp = std::env::temp_dir().join(format!("test-sid-new-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);

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

        let existing = std::fs::read_to_string(&tmp).unwrap();
        let should_write = existing.trim() != "session-B";
        assert!(should_write);
        let _ = std::fs::write(&tmp, "session-B");

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-B");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn session_id_file_empty_is_written() {
        let tmp = std::env::temp_dir().join(format!("test-sid-empty-{}", std::process::id()));
        let _ = std::fs::write(&tmp, "");

        let existing = std::fs::read_to_string(&tmp).unwrap();
        let should_write = existing.trim() != "session-A";
        assert!(should_write);

        let _ = std::fs::write(&tmp, "session-A");
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "session-A");

        let _ = std::fs::remove_file(&tmp);
    }
}
