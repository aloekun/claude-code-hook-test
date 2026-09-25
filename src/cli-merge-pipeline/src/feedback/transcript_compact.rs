//! filter 済 transcript の 1 エントリを `analyze-session` facet が読む形へ縮約する。
//!
//! # なぜ縮約するか (insights 2026-09-25 / PR #517 実測)
//!
//! 時刻 range filter だけでは 1 PR 分が **2.14 MB / 449 行 (1 行最大 85 KB)** になり、
//! haiku の facet は全体を読み切れない。どこを読み飛ばすかが実行ごとに無作為に決まり、
//! 読み切れないことが解析スクリプトの書き出し (`.takt/analyze_transcript*.py` の残骸) も誘う。
//!
//! 内訳の実測と、それぞれを捨てたときに失うもの:
//!
//! | 要素 | 量 | 扱い |
//! |---|---|---|
//! | 最上位の `toolUseResult` | 478 KB | 捨てる。`message.content` の tool_result と同内容の重複 |
//! | `thinking` block | 282 KB | 捨てる。暗号化済みで本文は空 (ADR-030 § transcript の制約) |
//! | `uuid` / `cwd` / `message.usage` 等の付帯データ | 残りの大半 | 捨てる。facet のスキーマが使うのは [`KEPT_ENTRY_FIELDS`] だけ |
//! | エラーの tool_result (`is_error`) | 6 件 / 1.9 KB | **全文残す** |
//! | 正常な tool_result | 中央値 335 字、4 KB 超は 19 件 | 長いものだけ先頭と末尾を残す |
//!
//! 縮約後は 457 KB (79% 減)。**失うのは長い正常出力の中ほどだけ**で、その場所は省略マーカーで明示される。
//!
//! # なぜ先頭だけでなく末尾も残すか
//!
//! cargo test の失敗要約やスタックトレースは出力の**末尾**に出る。先頭だけを残すと、
//! facet が拾うべき「何が失敗したか」を捨ててしまう。
//!
//! # 触らないもの
//!
//! - `tool_use` の入力: global rules (`~/.claude/**`) の編集は VCS に出ず、transcript の
//!   Edit / Write 入力でしか見えない (ADR-030 § Global rules editing の追跡)。
//! - 文字列の user content: ユーザーの指示そのもので、知見抽出の主対象。

use serde_json::{Map, Value};

/// 出力に残すエントリのフィールド。`analyze-session` facet のスキーマと一致させる。
const KEPT_ENTRY_FIELDS: [&str; 4] = ["type", "timestamp", "sessionId", "message"];

/// 捨てる content block の type。
const DROPPED_BLOCK_TYPES: [&str; 2] = ["thinking", "redacted_thinking"];

/// これを超える正常な tool_result の本文は中ほどを省略する (文字数、バイト数ではない)。
const TOOL_RESULT_MAX_CHARS: usize = 2000;
const TOOL_RESULT_HEAD_CHARS: usize = 1000;
const TOOL_RESULT_TAIL_CHARS: usize = 1000;

/// 画像 block の置き換え先。base64 はテキストとして読む facet には意味を持たない。
const IMAGE_PLACEHOLDER: &str = "[画像省略]";

/// transcript の 1 エントリを縮約する。
pub(crate) fn compact_entry(entry: &Value) -> Value {
    let mut out = Map::new();
    for key in KEPT_ENTRY_FIELDS {
        let Some(value) = entry.get(key) else {
            continue;
        };
        let value = if key == "message" {
            compact_message(value)
        } else {
            value.clone()
        };
        out.insert(key.to_string(), value);
    }
    Value::Object(out)
}

/// `message` から `role` と `content` だけを残す (`model` / `usage` / `id` 等は捨てる)。
///
/// オブジェクトでない `message` は想定外の形なので、判断せずそのまま通す。
fn compact_message(message: &Value) -> Value {
    if !message.is_object() {
        return message.clone();
    }
    let mut out = Map::new();
    if let Some(role) = message.get("role") {
        out.insert("role".to_string(), role.clone());
    }
    if let Some(content) = message.get("content") {
        out.insert("content".to_string(), compact_content(content));
    }
    Value::Object(out)
}

/// 文字列 content (ユーザーの発話) はそのまま、block 配列は block ごとに縮約する。
fn compact_content(content: &Value) -> Value {
    let Some(blocks) = content.as_array() else {
        return content.clone();
    };
    Value::Array(
        blocks
            .iter()
            .filter(|block| !DROPPED_BLOCK_TYPES.contains(&block_type(block)))
            .map(compact_block)
            .collect(),
    )
}

/// 1 block を縮約する。画像は置き換え、正常な tool_result は本文を縮める。
///
/// **エラーの tool_result には手を付けない。** 失敗の内容こそ facet が拾う知見であり、
/// 実測では 6 件 / 1.9 KB と小さいため全文残しても量に響かない。
fn compact_block(block: &Value) -> Value {
    match block_type(block) {
        "image" => image_placeholder(),
        "tool_result" if !is_error(block) => {
            let mut block = block.clone();
            if let Some(content) = block.get_mut("content") {
                *content = compact_tool_result_content(content);
            }
            block
        }
        _ => block.clone(),
    }
}

/// tool_result の本文は文字列か、text / image block の配列のどちらか。
fn compact_tool_result_content(content: &Value) -> Value {
    match content {
        Value::String(text) => Value::String(trim_middle(text)),
        Value::Array(blocks) => Value::Array(
            blocks
                .iter()
                .map(|block| match block_type(block) {
                    "image" => image_placeholder(),
                    "text" => {
                        let mut block = block.clone();
                        if let Some(Value::String(text)) = block.get_mut("text") {
                            *text = trim_middle(text);
                        }
                        block
                    }
                    _ => block.clone(),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn block_type(block: &Value) -> &str {
    block.get("type").and_then(Value::as_str).unwrap_or("")
}

fn is_error(block: &Value) -> bool {
    block.get("is_error").and_then(Value::as_bool) == Some(true)
}

fn image_placeholder() -> Value {
    serde_json::json!({ "type": "text", "text": IMAGE_PLACEHOLDER })
}

/// [`TOOL_RESULT_MAX_CHARS`] を超える文字列の中ほどを省略マーカーに置き換える。
///
/// **文字単位で切る。** 出力には日本語が多く、バイト位置でスライスすると文字境界で panic する。
fn trim_middle(text: &str) -> String {
    let total = text.chars().count();
    if total <= TOOL_RESULT_MAX_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(TOOL_RESULT_HEAD_CHARS).collect();
    let tail_start = text
        .char_indices()
        .nth(total - TOOL_RESULT_TAIL_CHARS)
        .map_or(text.len(), |(index, _)| index);
    let omitted = total - TOOL_RESULT_HEAD_CHARS - TOOL_RESULT_TAIL_CHARS;
    format!("{head}\n[… {omitted} 文字省略 …]\n{}", &text[tail_start..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool_result(content: Value, is_error: bool) -> Value {
        json!({
            "type": "user",
            "timestamp": "2026-04-25T09:00:00.000Z",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "is_error": is_error,
                    "content": content,
                }],
            },
        })
    }

    /// 縮約後の最初の tool_result block の本文。
    fn tool_result_content(entry: &Value) -> &Value {
        &entry["message"]["content"][0]["content"]
    }

    #[test]
    fn keeps_only_the_fields_the_facet_reads() {
        let entry = json!({
            "type": "assistant",
            "timestamp": "2026-04-25T09:00:00.000Z",
            "sessionId": "s-1",
            "uuid": "u-1",
            "parentUuid": "p-1",
            "cwd": "/repo",
            "toolUseResult": { "stdout": "duplicated" },
            "message": {
                "role": "assistant",
                "model": "m",
                "usage": { "input_tokens": 1 },
                "content": [{ "type": "text", "text": "hello" }],
            },
        });

        let compacted = compact_entry(&entry);

        assert_eq!(
            compacted,
            json!({
                "type": "assistant",
                "timestamp": "2026-04-25T09:00:00.000Z",
                "sessionId": "s-1",
                "message": {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "hello" }],
                },
            })
        );
    }

    #[test]
    fn drops_thinking_blocks_and_keeps_the_rest_in_order() {
        let entry = json!({
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [
                    { "type": "thinking", "thinking": "", "signature": "sig" },
                    { "type": "redacted_thinking", "data": "x" },
                    { "type": "text", "text": "answer" },
                    { "type": "tool_use", "name": "Bash", "input": { "command": "ls" } },
                ],
            },
        });

        let compacted = compact_entry(&entry);

        assert_eq!(
            compacted["message"]["content"],
            json!([
                { "type": "text", "text": "answer" },
                { "type": "tool_use", "name": "Bash", "input": { "command": "ls" } },
            ])
        );
    }

    #[test]
    fn keeps_an_error_tool_result_in_full_however_long() {
        let long = "e".repeat(TOOL_RESULT_MAX_CHARS * 3);

        let compacted = compact_entry(&tool_result(json!(long), true));

        assert_eq!(tool_result_content(&compacted), &json!(long));
    }

    #[test]
    fn keeps_a_tool_result_at_the_limit_unchanged() {
        let at_limit = "a".repeat(TOOL_RESULT_MAX_CHARS);

        let compacted = compact_entry(&tool_result(json!(at_limit), false));

        assert_eq!(tool_result_content(&compacted), &json!(at_limit));
    }

    #[test]
    fn trims_the_middle_of_a_tool_result_one_char_over_the_limit() {
        let over = format!(
            "{}M{}",
            "h".repeat(TOOL_RESULT_HEAD_CHARS),
            "t".repeat(TOOL_RESULT_TAIL_CHARS)
        );

        let compacted = compact_entry(&tool_result(json!(over), false));

        let expected = format!(
            "{}\n[… 1 文字省略 …]\n{}",
            "h".repeat(TOOL_RESULT_HEAD_CHARS),
            "t".repeat(TOOL_RESULT_TAIL_CHARS)
        );
        assert_eq!(tool_result_content(&compacted), &json!(expected));
    }

    /// 末尾に出る失敗要約を残すこと (先頭だけ残す設計への退行を止める)。
    #[test]
    fn keeps_the_failure_summary_at_the_end_of_a_long_output() {
        let output = format!(
            "running 300 tests\n{}\ntest result: FAILED. 1 failed",
            "ok\n".repeat(2000)
        );

        let compacted = compact_entry(&tool_result(json!(output), false));

        let text = tool_result_content(&compacted).as_str().unwrap();
        assert!(text.starts_with("running 300 tests"), "{text}");
        assert!(text.ends_with("test result: FAILED. 1 failed"), "{text}");
        assert!(text.contains("文字省略"), "{text}");
    }

    /// 日本語をバイト位置で切ると文字境界で panic する。文字数で切ること。
    #[test]
    fn trims_multibyte_text_on_char_boundaries() {
        let text = format!("{}{}", "前".repeat(1500), "後".repeat(1500));

        let compacted = compact_entry(&tool_result(json!(text), false));

        let expected = format!(
            "{}\n[… 1000 文字省略 …]\n{}",
            "前".repeat(TOOL_RESULT_HEAD_CHARS),
            "後".repeat(TOOL_RESULT_TAIL_CHARS)
        );
        assert_eq!(tool_result_content(&compacted), &json!(expected));
    }

    #[test]
    fn trims_text_blocks_and_replaces_images_inside_an_array_tool_result() {
        let long = "x".repeat(TOOL_RESULT_MAX_CHARS + 500);
        let content = json!([
            { "type": "text", "text": long },
            { "type": "image", "source": { "type": "base64", "data": "AAAA" } },
        ]);

        let compacted = compact_entry(&tool_result(content, false));

        let expected_text = format!(
            "{}\n[… 500 文字省略 …]\n{}",
            "x".repeat(TOOL_RESULT_HEAD_CHARS),
            "x".repeat(TOOL_RESULT_TAIL_CHARS)
        );
        assert_eq!(
            tool_result_content(&compacted),
            &json!([
                { "type": "text", "text": expected_text },
                { "type": "text", "text": IMAGE_PLACEHOLDER },
            ])
        );
    }

    #[test]
    fn replaces_an_image_block_in_the_message_content() {
        let entry = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "data": "AAAA" } },
                    { "type": "text", "text": "この画面を見て" },
                ],
            },
        });

        let compacted = compact_entry(&entry);

        assert_eq!(
            compacted["message"]["content"],
            json!([
                { "type": "text", "text": IMAGE_PLACEHOLDER },
                { "type": "text", "text": "この画面を見て" },
            ])
        );
    }

    /// ユーザーの発話は知見抽出の主対象なので、長くても切らない。
    #[test]
    fn keeps_a_long_string_user_prompt_in_full() {
        let prompt = "指示".repeat(TOOL_RESULT_MAX_CHARS * 2);
        let entry = json!({ "type": "user", "message": { "role": "user", "content": prompt } });

        let compacted = compact_entry(&entry);

        assert_eq!(compacted["message"]["content"], json!(prompt));
    }

    /// global rules の編集は Write / Edit の入力でしか見えない (ADR-030)。切らない。
    #[test]
    fn keeps_a_long_tool_use_input_in_full() {
        let file_body = "r".repeat(TOOL_RESULT_MAX_CHARS * 3);
        let tool_use = json!({
            "type": "tool_use",
            "name": "Write",
            "input": { "file_path": "~/.claude/rules/x.md", "content": file_body },
        });
        let entry = json!({
            "type": "assistant",
            "message": { "role": "assistant", "content": [tool_use.clone()] },
        });

        let compacted = compact_entry(&entry);

        assert_eq!(compacted["message"]["content"], json!([tool_use]));
    }
}
