//! セッションの種類の判定 (ADR-091 § takt のセッションを除く)。
//!
//! takt は同じプロジェクトの設定で Claude Code を起動するため、takt のセッションでも Stop hook が
//! 発火する。takt の出力は「## Result: APPROVE」のような英語の決まった見出しで始まり、判定に
//! かけると誤って block する (2026-11 の実データで takt の最後の出力 609 件中 161 件)。
//!
//! takt のセッションは、最初のユーザー発言が takt の指示の決まった形で始まる。transcript の先頭から
//! 最初のユーザー発言を探し、その書き出しで見分ける。takt は Claude に見分けのための環境変数を
//! 渡さず、`entrypoint` も起動元のものを引き継ぐため、ほかに手がかりが無い。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// takt が Claude に渡す指示の書き出し (takt 0.35.3、ADR-017 で版を固定している)。
///
/// - `## Execution Context`: 各 step の実行 (phase 1)
/// - `**Review is already complete. Evaluate`: 判定 (phase 3)
pub(crate) const TAKT_PROMPT_PREFIXES: [&str; 2] = [
    "## Execution Context\n",
    "**Review is already complete. Evaluate",
];

/// 最初のユーザー発言を探す行数の上限。先頭の SessionStart の出力などを読み飛ばすのに足りる数。
const MAX_SCAN_LINES: usize = 200;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SessionKind {
    Takt,
    Interactive,
    /// transcript を読めない、または上限までにユーザー発言が見つからない。
    Unknown,
}

pub(crate) fn session_kind(transcript_path: &Path) -> SessionKind {
    let Ok(file) = File::open(transcript_path) else {
        return SessionKind::Unknown;
    };
    let lines = BufReader::new(file).lines().map_while(Result::ok);
    classify_lines(lines.take(MAX_SCAN_LINES))
}

fn classify_lines(lines: impl Iterator<Item = String>) -> SessionKind {
    for line in lines {
        let Some(text) = first_user_text(&line) else {
            continue;
        };
        return if TAKT_PROMPT_PREFIXES.iter().any(|p| text.trim_start().starts_with(p)) {
            SessionKind::Takt
        } else {
            SessionKind::Interactive
        };
    }
    SessionKind::Unknown
}

/// 1 行が「人か takt が書いたユーザー発言」なら、その本文を返す。
///
/// 除くもの: meta エントリ (hook の出力など)、サイドチェーン、ツール結果、`<` で始まるハーネスの
/// 差し込み (`<ide_opened_file>` など)。
fn first_user_text(line: &str) -> Option<String> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    if entry.get("type")?.as_str()? != "user" {
        return None;
    }
    let flag = |key: &str| entry.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    if flag("isMeta") || flag("isSidechain") {
        return None;
    }
    let content = entry.get("message")?.get("content")?;
    let text = match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            if blocks.iter().any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result")) {
                return None;
            }
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => return None,
    };
    let trimmed = text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('<') {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> String {
        serde_json::json!({"type": "user", "message": {"role": "user", "content": text}}).to_string()
    }

    fn classify(lines: &[String]) -> SessionKind {
        classify_lines(lines.iter().cloned())
    }

    #[test]
    fn takt_step_prompt_is_takt() {
        let lines = [user("\n## Execution Context\n- Working Directory: C:\\work\n")];
        assert_eq!(classify(&lines), SessionKind::Takt);
    }

    #[test]
    fn takt_judge_prompt_is_takt() {
        let lines = [user("**Review is already complete. Evaluate the report below.**")];
        assert_eq!(classify(&lines), SessionKind::Takt);
    }

    #[test]
    fn human_prompt_is_interactive() {
        let lines = [user("品質向上に貢献するタスクをピックアップしてください。")];
        assert_eq!(classify(&lines), SessionKind::Interactive);
    }

    /// IDE が差し込む `<ide_opened_file>` は読み飛ばし、その後の人の発言で決まる。
    #[test]
    fn harness_injection_is_skipped() {
        let lines = [
            user("<ide_opened_file>The user opened the file x</ide_opened_file>"),
            user("## Execution Context\n"),
        ];
        assert_eq!(classify(&lines), SessionKind::Takt);
    }

    #[test]
    fn meta_and_tool_result_entries_are_skipped() {
        let meta = serde_json::json!({"type": "user", "isMeta": true, "message": {"content": "## Execution Context\n"}}).to_string();
        let tool_result = serde_json::json!({"type": "user", "message": {"content": [{"type": "tool_result", "content": "x"}]}}).to_string();
        let lines = [meta, tool_result, user("確認してください。")];
        assert_eq!(classify(&lines), SessionKind::Interactive);
    }

    #[test]
    fn text_blocks_are_read() {
        let entry = serde_json::json!({"type": "user", "message": {"content": [{"type": "text", "text": "## Execution Context\n"}]}}).to_string();
        assert_eq!(classify(&[entry]), SessionKind::Takt);
    }

    #[test]
    fn no_user_prompt_is_unknown() {
        let lines = ["not json".to_string(), serde_json::json!({"type": "assistant"}).to_string()];
        assert_eq!(classify(&lines), SessionKind::Unknown);
    }

    #[test]
    fn missing_file_is_unknown() {
        assert_eq!(session_kind(Path::new("no/such/transcript.jsonl")), SessionKind::Unknown);
    }
}
