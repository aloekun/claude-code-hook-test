//! 空 diff で終わった agent の「なぜ止めたか」を 1 行だけ取り出す (I/O は [`read_reason`] のみ)。
//!
//! # なぜ要るのか
//!
//! implement 後の停止のうち `guard=failure` (変更なし) は、**完了済み / 詳細エントリの矛盾 /
//! 宣言が決められない / 権限不能** の 4 種が同じ 1 行に落ちる。理由を知っている唯一の主体は
//! agent 自身だが、workflow は SDK 出力を `hidden for security` で捨てていた。2026-09-05 の
//! 順位 356 (2 ターン・0 変更) は、ターン数と秒数から推論するしかなかった。
//!
//! # 公開面への出し方 (ADR-072 決定 14)
//!
//! agent の自由記述をそのままログへ出さない。**agent に固定接頭辞 [`STOP_PREFIX`] で始まる
//! 1 行を書かせ、その 1 行だけ**を [`lib_ledger::screen_for_public_output`] (制御文字除去・
//! バッククォート置換・200 文字切り詰め) に通して出す。接頭辞が無ければ本文には触れず
//! 「理由行なし」とだけ言う — それ自体が「指示が届いていない」という情報になる。
//!
//! # 色には影響しない
//!
//! 読めない / 無い / 形式違いのいずれも fail-open で、run の色 (exit code) は従来どおり
//! `publish` / `handoff` だけが決める ([`crate::classify`])。ここは説明行を 1 本足すだけ。

use lib_ledger::screen_for_public_output;

/// agent が停止理由を書くときの先頭行の接頭辞。**workflow の agent プロンプトと一致していること**
/// ([`crate::tests::the_agent_prompt_asks_for_the_stop_prefix`] が実ファイルと照合する)。
///
/// 照合は**トークン境界**で行う — 接頭辞の直後は行末か空白でなければならず、
/// `[NIGHTLY_AGENT_STOP]理由` のような連結は受けない ([`classify_final_text`])。
pub const STOP_PREFIX: &str = "[NIGHTLY_AGENT_STOP]";

/// [`screen_for_public_output`] が「中身が無い」ときに返す固定文。
///
/// 接頭辞の後ろが空白・制御文字・不可視文字だけだと screening 後にこの文になる。それを
/// `Stated` として公開すると「理由が書かれている」と誤読されるため、`Unstated` へ倒す。
/// 値は `lib-ledger` の `screening.rs` と結合しており、[`tests::the_empty_screening_sentinel_is_pinned`]
/// が実関数の出力と照合して drift を止める。
const EMPTY_SCREENED: &str = "(内容なし)";

/// 取り出した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentReason {
    /// 接頭辞付きの 1 行があった。中身は screening 済み。
    Stated(String),
    /// agent の最終メッセージはあるが、接頭辞の行が無い (指示が届いていない / 従っていない)。
    Unstated,
    /// execution file を読めない / 解釈できない。中身は原因。
    Unreadable(String),
    /// workflow が execution file を渡していない (env 未設定)。何も出さない。
    NotProvided,
}

/// execution file の JSON 本文から停止理由を取り出す (I/O なし)。
///
/// claude-code-action は SDK メッセージを **JSON 配列**で書く (`base-action/src/execution-file.ts`
/// の `JSON.stringify(messages, null, 2)`)。assistant メッセージは
/// `{"type":"assistant","message":{"content":[{"type":"text","text":...}, ...]}}` の形で、
/// **最後の text ブロック**が agent の締めの言葉になる。
pub fn extract_reason(json_text: &str) -> AgentReason {
    let messages: serde_json::Value = match serde_json::from_str(json_text) {
        Ok(value) => value,
        Err(e) => return AgentReason::Unreadable(format!("JSON を解釈できません: {e}")),
    };
    let Some(entries) = messages.as_array() else {
        return AgentReason::Unreadable("JSON が配列ではありません".to_string());
    };
    let Some(last_text) = last_assistant_text(entries) else {
        return AgentReason::Unreadable("assistant の text メッセージが 1 件も無い".to_string());
    };
    classify_final_text(&last_text)
}

/// 全 assistant メッセージの text ブロックのうち、最後の空でないものを返す。
fn last_assistant_text(entries: &[serde_json::Value]) -> Option<String> {
    let mut texts = entries
        .iter()
        .filter(|entry| entry.get("type").and_then(serde_json::Value::as_str) == Some("assistant"))
        .flat_map(|entry| {
            entry
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|block| block.get("type").and_then(serde_json::Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
        .filter(|text| !text.trim().is_empty());
    texts.next_back().map(str::to_string)
}

/// 最終メッセージの**先頭の内容行**だけを見る。
///
/// 先頭行に限るのは、接頭辞をメッセージのどこかに含めばよいとすると、本文の途中で
/// 「ここからが理由」と言い張る行を後から足せてしまうため。契約は「先頭行」の 1 点に置く。
///
/// # 受理の幅 (CodeRabbit #482 を受けて明示)
///
/// - **先頭の空行と行頭の空白は許す。** markdown 整形で 1 行目が空く / 字下げされることは
///   あり、そこで理由を捨てると本機能の目的 (理由を拾う) に反する。空行より前には何も
///   無いので、後から足した行が「先頭」を名乗る経路にはならない
/// - **接頭辞の直後はトークン境界を要求する。** `[NIGHTLY_AGENT_STOP]理由` のような連結は
///   `Unstated` — 契約どおりの区切りが無く、`[NIGHTLY_AGENT_STOP]X` のような別トークンとも
///   区別できない
/// - **理由が空なら `Unstated`。** 空白・制御文字・不可視文字だけの理由は screening 後に
///   [`EMPTY_SCREENED`] になる。それを `Stated` として出すと「書いてある」と誤読される
fn classify_final_text(text: &str) -> AgentReason {
    let Some(first_line) = text.lines().find(|line| !line.trim().is_empty()) else {
        return AgentReason::Unstated;
    };
    let Some(rest) = first_line.trim_start().strip_prefix(STOP_PREFIX) else {
        return AgentReason::Unstated;
    };
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return AgentReason::Unstated;
    }
    let screened = screen_for_public_output(rest);
    if screened == EMPTY_SCREENED {
        return AgentReason::Unstated;
    }
    AgentReason::Stated(screened)
}

/// env で渡された execution file を読んで取り出す (副作用: ファイル読み取りのみ)。
///
/// **失敗は色に影響しない** (fail-open)。ここは助言層で、
/// [ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md) が fail-closed を
/// ゲート関数に限っている。
pub fn read_reason(execution_file_env: &str) -> AgentReason {
    let path = execution_file_env.trim();
    if path.is_empty() {
        return AgentReason::NotProvided;
    }
    match std::fs::read_to_string(path) {
        Ok(text) => extract_reason(&text),
        Err(e) => AgentReason::Unreadable(format!("execution file を読めません ({path}): {e}")),
    }
}

/// 説明行。`NotProvided` は何も出さない (env を渡していない古い呼び手の出力を変えない)。
pub fn render_line(reason: &AgentReason) -> Option<String> {
    match reason {
        AgentReason::Stated(text) => Some(format!("[NIGHTLY_HANDOFF] agent の停止理由: {text}")),
        AgentReason::Unstated => Some(format!(
            "[NIGHTLY_HANDOFF] agent は停止理由を書きませんでした (最終メッセージの先頭行が {STOP_PREFIX} で始まっていない)。"
        )),
        AgentReason::Unreadable(why) => Some(format!(
            "[NIGHTLY_HANDOFF] agent の停止理由を読めませんでした ({why})。色には影響しません。"
        )),
        AgentReason::NotProvided => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execution(texts: &[&str]) -> String {
        let entries: Vec<serde_json::Value> = texts
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "assistant",
                    "message": { "role": "assistant", "content": [{ "type": "text", "text": t }] }
                })
            })
            .collect();
        serde_json::to_string(&entries).expect("json")
    }

    /// **2026-09-05 の順位 356 で欲しかった形。** 接頭辞付きの先頭行だけが出る。
    #[test]
    fn a_prefixed_first_line_is_the_reason() {
        let json = execution(&[
            "まず台帳を読みます",
            "[NIGHTLY_AGENT_STOP] 注釈が挙げる staleness.rs は working-copy staleness で本タスクと無関係\n詳細: ...",
        ]);
        assert_eq!(
            extract_reason(&json),
            AgentReason::Stated(
                "注釈が挙げる staleness.rs は working-copy staleness で本タスクと無関係".to_string()
            )
        );
    }

    /// 接頭辞が無ければ本文には触れず「理由行なし」とだけ言う。
    #[test]
    fn a_final_message_without_the_prefix_is_unstated_and_not_quoted() {
        let json = execution(&["実装内容が一意に決まらないため変更せず終了します"]);
        assert_eq!(extract_reason(&json), AgentReason::Unstated);
        let line = render_line(&AgentReason::Unstated).expect("行がある");
        assert!(!line.contains("一意に決まらない"), "本文を引用している: {line}");
        assert!(line.contains(STOP_PREFIX), "直し方 (接頭辞) が書かれていない: {line}");
    }

    /// **接頭辞だけ・空白だけ・不可視文字だけの理由は `Unstated`** (CodeRabbit #482)。
    /// 「(内容なし)」を `Stated` として出すと、理由が書かれていると誤読される。
    #[test]
    fn an_empty_reason_after_the_prefix_is_unstated() {
        for text in [
            "[NIGHTLY_AGENT_STOP]",
            "[NIGHTLY_AGENT_STOP] ",
            "[NIGHTLY_AGENT_STOP]    \n次の行に本文",
            "[NIGHTLY_AGENT_STOP] \u{200B}\u{202E}",
        ] {
            assert_eq!(extract_reason(&execution(&[text])), AgentReason::Unstated, "{text:?}");
        }
    }

    /// **接頭辞の直後はトークン境界。** 区切りの無い連結は契約どおりではないので採らない
    /// (`[NIGHTLY_AGENT_STOP]X` のような別トークンとも区別できない)。
    #[test]
    fn a_prefix_without_a_separator_is_unstated() {
        assert_eq!(
            extract_reason(&execution(&["[NIGHTLY_AGENT_STOP]理由を続けて書いた"])),
            AgentReason::Unstated
        );
        assert_eq!(
            extract_reason(&execution(&["[NIGHTLY_AGENT_STOP]X 別トークン"])),
            AgentReason::Unstated
        );
    }

    /// **先頭の空行と行頭の空白は許す** — markdown 整形で起こる形で、そこで理由を捨てると
    /// 本機能の目的に反する。空白は複数でもよい。
    #[test]
    fn leading_blank_lines_and_indentation_are_tolerated() {
        assert_eq!(
            extract_reason(&execution(&["\n\n   [NIGHTLY_AGENT_STOP]   字下げと空行つき"])),
            AgentReason::Stated("字下げと空行つき".to_string())
        );
    }

    /// [`EMPTY_SCREENED`] は `lib-ledger` の `screen_for_public_output` の固定文と結合している。
    /// ここで実関数の出力と照合し、あちらが文言を変えたら落ちるようにする。
    #[test]
    fn the_empty_screening_sentinel_is_pinned() {
        assert_eq!(screen_for_public_output(""), EMPTY_SCREENED);
        assert_eq!(screen_for_public_output("  \u{200B} "), EMPTY_SCREENED);
    }

    /// 接頭辞は**先頭行**に限る。途中の行で名乗っても採らない。
    #[test]
    fn the_prefix_is_only_honoured_on_the_first_line() {
        let json = execution(&["結論から書きます\n[NIGHTLY_AGENT_STOP] 後から足した理由"]);
        assert_eq!(extract_reason(&json), AgentReason::Unstated);
    }

    /// 理由は screening を通る (ADR-072 決定 14 と同じ関数): 制御文字は落ち、
    /// バッククォートは置換され、200 文字で切れる。
    #[test]
    fn the_reason_is_screened_before_it_reaches_the_log() {
        let long = "あ".repeat(300);
        let json = execution(&[&format!("[NIGHTLY_AGENT_STOP] `code`\u{200B}{long}")]);
        let AgentReason::Stated(text) = extract_reason(&json) else {
            panic!("Stated になるはず");
        };
        assert!(!text.contains('`'), "バッククォートが残っている: {text}");
        assert!(!text.contains('\u{200B}'), "不可視文字が残っている");
        assert!(text.chars().count() <= 200, "切り詰められていない: {}", text.chars().count());
        assert!(text.contains("以下略"), "{text}");
    }

    /// 壊れた / 空の execution file は「読めない」であって「理由なし」ではない (両者を混ぜると
    /// 「agent が書かなかった」という誤った結論に倒れる)。
    #[test]
    fn malformed_or_empty_input_is_unreadable_not_unstated() {
        assert!(matches!(extract_reason("not json"), AgentReason::Unreadable(_)));
        assert!(matches!(extract_reason("{}"), AgentReason::Unreadable(_)));
        assert!(matches!(extract_reason("[]"), AgentReason::Unreadable(_)));
        let only_system = r#"[{"type":"system","subtype":"init"},{"type":"result","subtype":"success"}]"#;
        assert!(matches!(extract_reason(only_system), AgentReason::Unreadable(_)));
    }

    /// env 未設定は何も出さない (渡していない呼び手の出力を変えない)。
    #[test]
    fn a_missing_env_renders_nothing() {
        assert_eq!(read_reason(""), AgentReason::NotProvided);
        assert_eq!(read_reason("   "), AgentReason::NotProvided);
        assert_eq!(render_line(&AgentReason::NotProvided), None);
    }

    /// 無いパスは「読めない」に倒し、原因にパスを含める。
    #[test]
    fn a_missing_file_is_unreadable_with_the_path_in_the_reason() {
        let AgentReason::Unreadable(why) = read_reason("/nonexistent/claude-execution-output.json")
        else {
            panic!("Unreadable になるはず");
        };
        assert!(why.contains("nonexistent"), "{why}");
    }

    /// text ブロックが複数メッセージに散っていても、**最後の空でない text** を採る。
    #[test]
    fn the_last_non_empty_text_block_wins() {
        let entries = serde_json::json!([
            {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read"}]}},
            {"type":"assistant","message":{"content":[{"type":"text","text":"[NIGHTLY_AGENT_STOP] 早い理由"}]}},
            {"type":"assistant","message":{"content":[{"type":"text","text":"   "}]}}
        ]);
        assert_eq!(
            extract_reason(&entries.to_string()),
            AgentReason::Stated("早い理由".to_string())
        );
    }
}
