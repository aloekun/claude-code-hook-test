//! SessionStart hook — セッション ID を環境変数とファイルに伝播する
//!
//! Claude Code の SessionStart イベントで発火し、以下の2つの経路で
//! session_id を伝播する:
//!
//!   1. $CLAUDE_ENV_FILE に export 文を追記 → Bash ツールから参照可能
//!   2. .claude/.session-id ファイルに書き出し → 子プロセス (exe) から参照可能
//!
//! これにより hooks-post-pr-monitor.exe が `claude -p --resume <session_id>`
//! でメインセッションに確実に接続できる。
//!
//! 重要: .session-id ファイルは「先勝ち」方式。既にファイルが存在する場合は
//! 上書きしない。これにより `claude -p` で起動されるサブセッション (review:ai 等)
//! がメインセッションの ID を上書きするのを防ぐ。

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
    // 先勝ち方式: 既にファイルが存在し中身が空でなければ上書きしない
    // これにより claude -p のサブセッションがメインセッション ID を上書きするのを防ぐ
    let sid_path = session_id_file_path();
    let should_write = match std::fs::read_to_string(&sid_path) {
        Ok(existing) => existing.trim().is_empty(),
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
}
