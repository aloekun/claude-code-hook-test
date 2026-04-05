//! SessionStart hook — セッション ID を環境変数とファイルに伝播する
//!
//! Claude Code の SessionStart イベントで発火し、以下の2つの経路で
//! session_id を伝播する:
//!
//!   1. $CLAUDE_ENV_FILE に export 文を追記 → Bash ツールから参照可能
//!   2. .claude/.session-id ファイルに書き出し → 子プロセス (exe) から参照可能
//!
//! .session-id ファイルは「同一 ID スキップ」方式:
//!   - 既に同じ session_id が書かれていれば何もしない (冪等)
//!   - 異なる ID (新セッション or サブセッション) の場合は上書きする
//!
//! 現在 hooks-post-pr-monitor は daemon + state file 方式に移行済みのため
//! .session-id を直接参照しないが、将来の拡張用にこの仕組みは維持する。

use serde::Deserialize;
use std::io::Read;
use std::path::Path;

/// SessionStart hook の stdin JSON (必要なフィールドのみ)
#[derive(Deserialize)]
struct HookInput {
    session_id: Option<String>,
}

/// session-id ファイルのパス (.claude/.session-id)
fn session_id_file_path() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(".session-id")
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

    // 3. additionalContext で Claude のコンテキストにも注入
    // serde_json で組み立てることで session_id 内の特殊文字を安全にエスケープ
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": format!("CLAUDE_CODE_SESSION_ID={}", session_id),
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
        let tmp = std::env::temp_dir().join(format!(
            "test-sid-new-{}",
            std::process::id()
        ));
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
        let tmp = std::env::temp_dir().join(format!(
            "test-sid-same-{}",
            std::process::id()
        ));
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
        let tmp = std::env::temp_dir().join(format!(
            "test-sid-diff-{}",
            std::process::id()
        ));
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

    #[test]
    fn session_id_file_empty_is_written() {
        let tmp = std::env::temp_dir().join(format!(
            "test-sid-empty-{}",
            std::process::id()
        ));
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
