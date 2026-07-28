//! UserPromptSubmit 回収層の出力組み立て (ADR-061 主因)。
//!
//! hard-fail 経路 (合成エントリで turn がエラー終了) では Stop hook が発火しないため、
//! 直後の UserPromptSubmit で leak を検知し、正規のツール呼び出しでの再実行を促す。
//! Stop hook (block) と異なり UserPromptSubmit では **`decision: block` を出してはならない**
//! (ユーザーの prompt 自体が拒否される)。additionalContext (モデル向け) と任意の
//! systemMessage (ユーザー可視 1 行、ADR-059) の非ブロッキング 2 チャネルで通知する。

use serde::Serialize;

/// additionalContext / systemMessage 先頭に付す検出タグ (他 hook の命名規約に倣う)。
const TAG: &str = "[TOOL_CALL_LEAK_RECOVERY]";

/// ツール名が抽出できなかった場合の表示。
const UNKNOWN_TOOL: &str = "不明";

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: &'static str,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

#[derive(Serialize)]
struct RecoveryOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
    #[serde(rename = "systemMessage", skip_serializing_if = "Option::is_none")]
    system_message: Option<String>,
}

/// additionalContext (モデル向け、複数行可) を組み立てる。
///
/// ADR-053 の block reason と同旨で「テキストに XML を書き直すな」を明示する
/// (ハーネス自身の "Please retry the tool call now." 注入は実データで再 leak を防げ
/// なかったため、leak 固有の禁止事項を補強する)。末尾に ADR-059 defense-in-depth の
/// 「ユーザーに一言伝えよ」指示を含め、systemMessage 非表示環境でもモデル経由で届ける。
fn build_additional_context(tool_name: Option<&str>) -> String {
    let tool = tool_name.unwrap_or(UNKNOWN_TOOL);
    format!(
        "{TAG}\n\
         直前の turn でツール呼び出し (ツール名: {tool}) がテキスト領域に生の XML として\
         出力され、実行されないまま turn がエラー終了しました (tool call leak)。この経路は\
         Stop hook が発火しないため、本メッセージで回収しています。\n\n\
         対処: 直前に意図したツール呼び出しを、正規のツール呼び出し機構で直ちに再実行して\
         ください。応答テキストに XML を書き直してはいけません。\n\n\
         また、この不具合が起きたことをセッション最初の応答でユーザーに一言伝えてください \
         (systemMessage 非表示環境向けの defense-in-depth、ADR-059)。"
    )
}

/// systemMessage (ユーザー可視 1 行、ADR-059)。表示ノイズ抑制のため `\n` を含めない。
fn build_system_message(tool_name: Option<&str>) -> String {
    let tool = tool_name.unwrap_or(UNKNOWN_TOOL);
    format!(
        "ツール呼び出し ({tool}) がテキスト出力され未実行のまま turn 終了したため、再実行を促しました (tool-call-leak recovery)"
    )
}

/// 回収 JSON 文字列を組み立てる。`emit_system_message` が真のときのみトップレベル
/// systemMessage を付す (ADR-059 config opt-in)。serialize 失敗時は `None` (fail-open)。
pub(crate) fn render(tool_name: Option<&str>, emit_system_message: bool) -> Option<String> {
    let output = RecoveryOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "UserPromptSubmit",
            additional_context: build_additional_context(tool_name),
        },
        system_message: emit_system_message.then(|| build_system_message(tool_name)),
    };
    serde_json::to_string(&output).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn additional_context_starts_with_tag_and_names_tool() {
        let ctx = build_additional_context(Some("Bash"));
        assert!(
            ctx.starts_with("[TOOL_CALL_LEAK_RECOVERY]\n"),
            "検出タグが 1 行目: {ctx}"
        );
        assert!(ctx.contains("Bash"), "ツール名を明示: {ctx}");
    }

    #[test]
    fn additional_context_forbids_rewriting_xml() {
        let ctx = build_additional_context(Some("Read"));
        assert!(
            ctx.contains("XML を書き直しては"),
            "テキストへの XML 再出力を明示的に禁止する (ADR-053 と同旨): {ctx}"
        );
    }

    #[test]
    fn additional_context_falls_back_to_unknown_tool() {
        assert!(build_additional_context(None).contains(UNKNOWN_TOOL));
    }

    #[test]
    fn additional_context_includes_defense_in_depth_user_notice() {
        assert!(
            build_additional_context(Some("Bash")).contains("ユーザーに一言"),
            "systemMessage 非表示環境向けの ADR-059 defense-in-depth 指示を含む"
        );
    }

    #[test]
    fn system_message_is_single_line() {
        let msg = build_system_message(Some("Bash"));
        assert!(!msg.contains('\n'), "systemMessage は 1 行に限定 (ADR-059): {msg}");
        assert!(msg.contains("Bash"));
    }

    #[test]
    fn render_includes_system_message_when_enabled() {
        let json = render(Some("Bash"), true).expect("serialize");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
        assert!(v["hookSpecificOutput"]["additionalContext"].is_string());
        assert!(
            v["systemMessage"].is_string(),
            "opt-in が真なら systemMessage を付す"
        );
    }

    #[test]
    fn render_omits_system_message_when_disabled() {
        let json = render(Some("Bash"), false).expect("serialize");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(
            v.get("systemMessage").is_none(),
            "opt-in が偽なら systemMessage を付さない (additionalContext のみ)"
        );
        assert!(v["hookSpecificOutput"]["additionalContext"].is_string());
    }

    #[test]
    fn render_never_emits_block_decision() {
        let json = render(Some("Bash"), true).expect("serialize");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(
            v.get("decision").is_none(),
            "UserPromptSubmit では decision:block を絶対に出さない"
        );
    }
}
