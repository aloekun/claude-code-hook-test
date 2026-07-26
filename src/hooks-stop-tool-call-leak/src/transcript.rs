//! transcript JSONL の tail 解析。
//!
//! Stop hook 入力の `transcript_path` が指すセッション JSONL を末尾から走査し、
//! 「最後の main-session assistant エントリが leak か」と「連続 leak 回数」を求める。
//!
//! 連続 leak カウントの設計 (ADR-053 §ループ防止):
//! - leak 検知で block すると Claude が再試行し、再 leak し得る (実データで確認済み)。
//!   block は `max_consecutive_blocks` 回まで許容し、超えたら fail-open する。
//! - block reason は `isMeta: true` の user エントリとして transcript に記録される
//!   (実データで確認済み) ため、isMeta user はチェーンを切らずにスキップする。
//! - 実ユーザーの発話 (isMeta でない user エントリ、tool_result を除く) は
//!   新しい試行の起点とみなしてチェーンをリセットする。

use crate::detect::{extract_tool_name, text_block_has_leak};
use serde_json::Value;

/// 末尾走査の対象エントリ数上限 (後方走査の暴走防止)
const MAX_SCAN_ENTRIES: usize = 50;

/// tail 走査の結果
pub(crate) struct TailScan {
    /// 末尾から連続する leak assistant エントリ数 (0 = 最後の assistant は正常)
    pub(crate) consecutive_leaks: u32,
    /// 最新の leak から抽出したツール名 (block reason での提示用)
    pub(crate) last_tool_name: Option<String>,
}

/// JSONL 文字列の末尾 `tail_lines` 行をパースする。パース不能な行はスキップ。
pub(crate) fn parse_tail_entries(content: &str, tail_lines: usize) -> Vec<Value> {
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(tail_lines);
    lines[start..]
        .iter()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// main セッションの assistant エントリか (sidechain = サブエージェントを除外)
fn is_main_assistant(entry: &Value) -> bool {
    entry.get("type").and_then(Value::as_str) == Some("assistant")
        && entry.get("isSidechain").and_then(Value::as_bool) != Some(true)
}

/// ハーネスが API リトライ失敗時に記録する合成 assistant エントリか (ADR-061)。
///
/// v2.1.206 実測: ツール呼び出しの parse 失敗 → 内部リトライも失敗した turn は
/// `isApiErrorMessage: true` かつ `message.model == "<synthetic>"` の擬似 assistant
/// エントリ ("The model's tool call could not be parsed (retry also failed).") で
/// 終端する。この合成エントリは `type: "assistant"` だが leak を持たないため、素の
/// `scan_tail` では「非 leak の最終 assistant」としてチェーンを打ち切り、直前の実
/// leak を取り逃がす (副因)。isMeta user と同様、チェーンを切らずスキップ対象とする。
/// 一般 API エラー (529 Overloaded / session limit 等) も本条件に該当するが、スキップ
/// されるだけで leak 判定には影響しない (直前が leak でなければチェーンは伸びない)。
fn is_synthetic(entry: &Value) -> bool {
    let api_error = entry.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
    let synthetic_model =
        entry.pointer("/message/model").and_then(Value::as_str) == Some("<synthetic>");
    api_error || synthetic_model
}

/// assistant エントリの text block 群を返す。
///
/// `message.content` は通常 block 配列だが、文字列形式にもフォールバック対応する。
fn assistant_text_blocks(entry: &Value) -> Vec<&str> {
    match entry.pointer("/message/content") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect(),
        _ => Vec::new(),
    }
}

/// user エントリの content に tool_result block が含まれるか
fn user_content_is_tool_result(entry: &Value) -> bool {
    match entry.pointer("/message/content") {
        Some(Value::Array(blocks)) => blocks
            .iter()
            .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")),
        _ => false,
    }
}

/// 連続 leak チェーンを打ち切る「実ユーザーの発話」か。
///
/// isMeta エントリ (Stop hook feedback / ハーネス自動注入) と tool_result は
/// チェーンを切らない。実ユーザーの発話は新しい試行の起点なのでリセットする。
fn is_chain_breaking_user_entry(entry: &Value) -> bool {
    if entry.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    if entry.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    if entry.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    !user_content_is_tool_result(entry)
}

/// エントリ列を末尾から走査し、連続 leak 数と最新 leak のツール名を求める。
///
/// 走査規則:
/// - 実ユーザーの発話に到達したら打ち切り (チェーンリセット)
/// - assistant 以外 (queue-operation / isMeta user / tool_result 等) はスキップ
/// - 合成 assistant エントリ (hard-fail、ADR-061) はチェーンを切らずスキップ
/// - 非 leak の実 assistant に到達したら打ち切り
pub(crate) fn scan_tail(entries: &[Value]) -> TailScan {
    let mut consecutive_leaks = 0u32;
    let mut last_tool_name: Option<String> = None;
    for entry in entries.iter().rev().take(MAX_SCAN_ENTRIES) {
        if is_chain_breaking_user_entry(entry) {
            break;
        }
        if !is_main_assistant(entry) {
            continue;
        }
        if is_synthetic(entry) {
            continue;
        }
        let blocks = assistant_text_blocks(entry);
        if !blocks.iter().any(|text| text_block_has_leak(text)) {
            break;
        }
        if consecutive_leaks == 0 {
            last_tool_name = blocks.iter().find_map(|text| extract_tool_name(text));
        }
        consecutive_leaks += 1;
    }
    TailScan {
        consecutive_leaks,
        last_tool_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LEAK_TEXT: &str = "court\n<invoke name=\"Bash\">\n<parameter name=\"command\">pnpm push</parameter>\n</invoke>";

    fn assistant_text_entry(text: &str) -> Value {
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}})
    }

    fn assistant_tool_use_entry() -> Value {
        json!({"type": "assistant", "message": {"content": [
            {"type": "text", "text": "実行します。"},
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}
        ]}})
    }

    fn meta_user_entry(text: &str) -> Value {
        json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": text}})
    }

    fn real_user_entry(text: &str) -> Value {
        json!({"type": "user", "message": {"role": "user", "content": text}})
    }

    fn tool_result_entry() -> Value {
        json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}})
    }

    /// ハーネスの hard-fail 合成エントリ (ADR-061、828764ce line 274/286 実測構造)。
    fn synthetic_entry() -> Value {
        json!({"type": "assistant", "isApiErrorMessage": true, "message": {
            "model": "<synthetic>",
            "content": [{"type": "text",
                "text": "The model's tool call could not be parsed (retry also failed)."}]
        }})
    }

    fn to_jsonl(entries: &[Value]) -> String {
        entries
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parse_tail_entries_skips_malformed_lines() {
        let content = format!("not-json{{\n{}", assistant_text_entry("hello"));
        let entries = parse_tail_entries(&content, 200);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn parse_tail_entries_respects_tail_limit() {
        let all: Vec<Value> = (0..10).map(|i| json!({"type": "x", "i": i})).collect();
        let entries = parse_tail_entries(&to_jsonl(&all), 3);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["i"], 7);
    }

    #[test]
    fn scan_detects_single_leak_with_tool_name() {
        let entries = vec![assistant_tool_use_entry(), assistant_text_entry(LEAK_TEXT)];
        let scan = scan_tail(&entries);
        assert_eq!(scan.consecutive_leaks, 1);
        assert_eq!(scan.last_tool_name.as_deref(), Some("Bash"));
    }

    #[test]
    fn scan_returns_zero_for_normal_last_assistant() {
        let entries = vec![
            assistant_text_entry(LEAK_TEXT),
            assistant_text_entry("完了しました。"),
        ];
        let scan = scan_tail(&entries);
        assert_eq!(scan.consecutive_leaks, 0);
    }

    #[test]
    fn scan_returns_zero_for_tool_use_only_entry() {
        let entries = vec![assistant_tool_use_entry()];
        assert_eq!(scan_tail(&entries).consecutive_leaks, 0);
    }

    #[test]
    fn scan_counts_consecutive_leaks_across_meta_user_entries() {
        let entries = vec![
            assistant_text_entry(LEAK_TEXT),
            meta_user_entry("Stop hook feedback: 再実行してください"),
            assistant_text_entry(LEAK_TEXT),
        ];
        assert_eq!(scan_tail(&entries).consecutive_leaks, 2);
    }

    #[test]
    fn scan_resets_chain_at_real_user_entry() {
        let entries = vec![
            assistant_text_entry(LEAK_TEXT),
            real_user_entry("再実行してください"),
            assistant_text_entry(LEAK_TEXT),
        ];
        assert_eq!(scan_tail(&entries).consecutive_leaks, 1);
    }

    #[test]
    fn scan_does_not_break_chain_at_tool_result() {
        let entries = vec![
            assistant_text_entry(LEAK_TEXT),
            tool_result_entry(),
            assistant_text_entry(LEAK_TEXT),
        ];
        assert_eq!(scan_tail(&entries).consecutive_leaks, 2);
    }

    #[test]
    fn scan_skips_sidechain_assistant_entries() {
        let sidechain = json!({"type": "assistant", "isSidechain": true,
            "message": {"content": [{"type": "text", "text": "サブエージェント出力"}]}});
        let entries = vec![assistant_text_entry(LEAK_TEXT), sidechain];
        let scan = scan_tail(&entries);
        assert_eq!(scan.consecutive_leaks, 1);
        assert_eq!(scan.last_tool_name.as_deref(), Some("Bash"));
    }

    #[test]
    fn scan_skips_non_message_entries() {
        let entries = vec![
            assistant_text_entry(LEAK_TEXT),
            json!({"type": "queue-operation", "operation": "enqueue"}),
            json!({"type": "last-prompt", "lastPrompt": "..."}),
        ];
        assert_eq!(scan_tail(&entries).consecutive_leaks, 1);
    }

    #[test]
    fn scan_handles_string_content_assistant() {
        let entry = json!({"type": "assistant", "message": {"content": LEAK_TEXT}});
        assert_eq!(scan_tail(&[entry]).consecutive_leaks, 1);
    }

    #[test]
    fn scan_handles_empty_entries() {
        assert_eq!(scan_tail(&[]).consecutive_leaks, 0);
    }

    #[test]
    fn is_synthetic_detects_both_markers() {
        assert!(is_synthetic(&synthetic_entry()));
        assert!(
            is_synthetic(&json!({"type": "assistant", "isApiErrorMessage": true})),
            "isApiErrorMessage のみでも合成と判定する"
        );
        assert!(
            is_synthetic(&json!({"type": "assistant", "message": {"model": "<synthetic>"}})),
            "model == <synthetic> のみでも合成と判定する"
        );
        assert!(
            !is_synthetic(&assistant_text_entry(LEAK_TEXT)),
            "通常 assistant は合成でない"
        );
    }

    #[test]
    fn scan_skips_synthetic_and_detects_preceding_leak() {
        let entries = vec![assistant_text_entry(LEAK_TEXT), synthetic_entry()];
        let scan = scan_tail(&entries);
        assert_eq!(
            scan.consecutive_leaks, 1,
            "828764ce 1 回目 (leak→合成 で turn 終端): 合成はチェーンを切らず直前の leak を検知"
        );
        assert_eq!(scan.last_tool_name.as_deref(), Some("Bash"));
    }

    #[test]
    fn scan_zero_when_synthetic_follows_normal_assistant() {
        let entries = vec![assistant_text_entry("作業を続けます。"), synthetic_entry()];
        assert_eq!(
            scan_tail(&entries).consecutive_leaks,
            0,
            "一般 API エラー (overloaded 等) が正常応答の後: 合成の直前が leak でなければ伸びない"
        );
    }

    #[test]
    fn scan_leak_chain_breaks_at_real_user_across_synthetic() {
        let entries = vec![
            assistant_text_entry(LEAK_TEXT),
            synthetic_entry(),
            real_user_entry("テキストで出力されて止まっています"),
            meta_user_entry("The previous response failed to produce a valid tool call."),
            assistant_text_entry(LEAK_TEXT),
            synthetic_entry(),
        ];
        assert_eq!(
            scan_tail(&entries).consecutive_leaks,
            1,
            "828764ce 2 回目 (leak→合成→実 user→isMeta→leak→合成): 実 user がチェーン起点をリセット"
        );
    }
}
