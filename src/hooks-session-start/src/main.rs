//! SessionStart hook — セッション ID を環境変数とファイルに伝播する + 各種 nudge
//!
//! Claude Code の SessionStart イベントで発火し、以下の経路で session 起動準備を行う:
//!
//!   1. $CLAUDE_ENV_FILE に export 文を追記 → Bash ツールから参照可能
//!   2. .claude/.session-id ファイルに書き出し → 子プロセス (exe) から参照可能
//!   3. Orphan run reaper (ADR-030 §L2): `reaper` module
//!   4. Working copy staleness nudge: `staleness` module
//!   5. Weekly review reminder (ADR-031 Phase C): `weekly_review` module
//!   6. Monthly review reminder (ADR-062, WP-12 step 2/3): `monthly_review` module
//!
//! 旧 3. PR monitor catch-up (Bb-3) は WP-17 PR 3 で撤去 — park/wakeup モデルの廃止に伴い
//! 「park 失効の救済」という役割ごと dead code になった (ADR-018 amendment 参照)。
//!
//! 各 nudge の発火は `lib-telemetry` (ADR-055) に `warn` として記録され、ROI 棚卸しの
//! 観測基盤 (`.claude/telemetry/firings-*.jsonl`) に載る (fail-open)。
//!
//! .session-id ファイルは「同一 ID スキップ」方式:
//!   - 既に同じ session_id が書かれていれば何もしない (冪等)
//!   - 異なる ID (新セッション or サブセッション) の場合は上書きする

use lib_hook_output::SingleLineMessage;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

mod hooks_config;
mod jj_helpers;
mod monthly_review;
mod past_time;
mod reaper;
mod staleness;
mod weekly_review;

use hooks_config::read_hooks_config;
use monthly_review::compute_monthly_review_reminder_nudge;
use reaper::compute_reaper_nudge;
use staleness::{compute_staleness_nudge, compute_workspace_stale_nudge};
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

/// `additionalContext` (session_id + 任意の nudge 群: reaper / staleness /
/// workspace_stale / weekly review) と任意の `systemMessage` を組み立て、Claude Code に返す JSON を
/// stdout に書き出す。各 nudge の追記と telemetry 記録はヘルパーに委譲する。
/// serde_json で組み立てることで session_id 内の特殊文字を安全にエスケープする。
fn emit_session_start_output(session_id: &str) {
    let mut context = format!("CLAUDE_CODE_SESSION_ID={}", session_id);
    let mut system_message: Option<SingleLineMessage> = None;
    let now_unix = current_unix_secs();
    if let Ok(cwd) = std::env::current_dir() {
        system_message = append_cwd_nudges(&mut context, session_id, &cwd, now_unix);
    }
    let output = build_session_start_json(&context, system_message.as_ref());
    println!("{}", output);
}

/// cwd 依存の nudge 群 (reaper / staleness / workspace_stale / weekly review / monthly review) を
/// `context` に追記し、発火時は telemetry に記録する。weekly / monthly review はユーザー可視の
/// systemMessage を伴い、両方発火した場合は 1 行に合成して返す (systemMessage スロットは 1 つのため。
/// どちらも発火しなければ `None`)。
fn append_cwd_nudges(
    context: &mut String,
    session_id: &str,
    cwd: &Path,
    now_unix: i64,
) -> Option<SingleLineMessage> {
    if let Some(reaper_nudge) = compute_reaper_nudge(cwd, now_unix) {
        context.push_str("\n\n");
        context.push_str(&reaper_nudge);
        record_nudge_firing("reaper", session_id);
    }
    let hooks_config = read_hooks_config(cwd);
    let session_start = hooks_config.session_start.as_ref()?;
    if let Some(staleness_config) = session_start.staleness.as_ref() {
        if let Some(staleness_nudge) = compute_staleness_nudge(cwd, staleness_config) {
            context.push_str("\n\n");
            context.push_str(&staleness_nudge);
            record_nudge_firing("staleness", session_id);
        }
        if let Some(stale_nudge) = compute_workspace_stale_nudge(staleness_config) {
            context.push_str("\n\n");
            context.push_str(&stale_nudge);
            record_nudge_firing("workspace_stale", session_id);
        }
    }
    append_review_reminder_nudges(context, session_id, cwd, session_start, now_unix)
}

/// weekly / monthly review reminder を評価して `context` に追記し、発火した systemMessage を
/// 合成して返す。両 reminder は同型 (additionalContext + 任意 systemMessage) で、systemMessage
/// スロットが出力 JSON に 1 つのため合成する。両 config は独立に opt-in される (片方だけ enable 可)。
fn append_review_reminder_nudges(
    context: &mut String,
    session_id: &str,
    cwd: &Path,
    session_start: &hooks_config::SessionStartConfig,
    now_unix: i64,
) -> Option<SingleLineMessage> {
    let mut system_messages: Vec<SingleLineMessage> = Vec::new();
    if let Some(weekly_config) = session_start.weekly_review_reminder.as_ref() {
        if let Some(weekly_nudge) =
            compute_weekly_review_reminder_nudge(cwd, weekly_config, now_unix)
        {
            context.push_str("\n\n");
            context.push_str(&weekly_nudge.additional_context);
            record_nudge_firing("weekly_review_reminder", session_id);
            system_messages.extend(weekly_nudge.system_message);
        }
    }
    if let Some(monthly_config) = session_start.monthly_review_reminder.as_ref() {
        if let Some(monthly_nudge) =
            compute_monthly_review_reminder_nudge(cwd, monthly_config, now_unix)
        {
            context.push_str("\n\n");
            context.push_str(&monthly_nudge.additional_context);
            record_nudge_firing("monthly_review_reminder", session_id);
            system_messages.extend(monthly_nudge.system_message);
        }
    }
    combine_system_messages(system_messages)
}

/// 複数の nudge が返した systemMessage を 1 行に合成する (ADR-059: systemMessage スロットは
/// 出力 JSON に 1 つのため)。
///
/// 0 件 → `None`、1 件 → そのまま、複数 → ` / ` 区切りで連結して 1 つの `SingleLineMessage` に
/// する (連結後も単一行不変条件は `SingleLineMessage::new` が構造的に保証する)。
fn combine_system_messages(messages: Vec<SingleLineMessage>) -> Option<SingleLineMessage> {
    match messages.len() {
        0 => None,
        1 => messages.into_iter().next(),
        _ => Some(SingleLineMessage::new(
            messages
                .iter()
                .map(SingleLineMessage::as_str)
                .collect::<Vec<_>>()
                .join(" / "),
        )),
    }
}

/// nudge の発火を telemetry (ADR-055) に記録する (fail-open)。
///
/// `id` は nudge 種別 (`weekly_review_reminder` / `reaper` /
/// `staleness` / `workspace_stale`)。nudge は助言出力のため decision は一律 `Warn`
/// (「発火の重み」軸であり、実際に停止したかではない。jj-op-verify の非 block warn と同性質)。
/// 記録失敗・opt-in OFF は lib-telemetry 内部で握りつぶすため hook 本来の出力を妨げない。
fn record_nudge_firing(id: &str, session_id: &str) {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "hooks-session-start",
        kind: lib_telemetry::FiringKind::Hook,
        id,
        decision: lib_telemetry::Decision::Warn,
        session_id: Some(session_id),
    });
}

/// SessionStart hook の stdout JSON を組み立てる純粋関数 (ADR-059)。
///
/// `context` は常に `hookSpecificOutput.additionalContext` (モデル可視) に載せる。
/// `system_message` が `Some` のときのみトップレベル `systemMessage` (ユーザー可視) を付与し、
/// `None` のときは従来どおり `systemMessage` を省いた JSON を返す。単一行不変条件は
/// `SingleLineMessage` が保証する (ADR-059)。
fn build_session_start_json(
    context: &str,
    system_message: Option<&SingleLineMessage>,
) -> serde_json::Value {
    let mut output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    });
    if let Some(message) = system_message {
        output["systemMessage"] = serde_json::Value::String(message.as_str().to_string());
    }
    output
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
    fn build_session_start_json_omits_system_message_when_none() {
        let output = build_session_start_json("ctx-only", None);
        assert_eq!(
            output["hookSpecificOutput"]["hookEventName"],
            "SessionStart"
        );
        assert_eq!(output["hookSpecificOutput"]["additionalContext"], "ctx-only");
        assert!(
            output.get("systemMessage").is_none(),
            "system_message = None のときトップレベル systemMessage は付与しない"
        );
    }

    #[test]
    fn build_session_start_json_includes_system_message_when_some() {
        let msg = SingleLineMessage::new("週次レビュー: 実行記録なし");
        let output = build_session_start_json("ctx", Some(&msg));
        assert_eq!(output["systemMessage"], "週次レビュー: 実行記録なし");
        assert_eq!(output["hookSpecificOutput"]["additionalContext"], "ctx");
        assert_eq!(
            output["hookSpecificOutput"]["hookEventName"],
            "SessionStart"
        );
    }

    #[test]
    fn combine_system_messages_none_when_empty() {
        assert!(combine_system_messages(Vec::new()).is_none());
    }

    #[test]
    fn combine_system_messages_passes_single_through_unchanged() {
        let msg = combine_system_messages(vec![SingleLineMessage::new("週次レビュー: 実行記録なし")])
            .expect("1 件はそのまま Some");
        assert_eq!(msg.as_str(), "週次レビュー: 実行記録なし");
    }

    #[test]
    fn combine_system_messages_joins_multiple_on_one_line() {
        let msg = combine_system_messages(vec![
            SingleLineMessage::new("週次レビュー: 実行記録なし"),
            SingleLineMessage::new("月次レビュー: 実行記録なし"),
        ])
        .expect("複数件は合成して Some");
        assert_eq!(
            msg.as_str(),
            "週次レビュー: 実行記録なし / 月次レビュー: 実行記録なし"
        );
        assert!(
            !msg.as_str().contains('\n'),
            "合成後も単一行不変条件を満たす (SingleLineMessage が保証)"
        );
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
