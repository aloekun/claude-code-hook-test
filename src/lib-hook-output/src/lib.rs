//! Hook JSON 出力のうち「1 行であることが必須のチャネル」(ADR-059 systemMessage) 向けの
//! 検証済み文字列型を提供する共有 crate。
//!
//! # 背景 (ADR-059)
//! systemMessage は「ユーザー可視の 1 行サマリー」チャネルで、複数行は additionalContext が
//! 担当する。従来この単一行不変条件は各 producer の `format!` + per-site の
//! `assert!(!msg.contains('\n'))` に依存しており、`\r` の見落とし (PR #326 で CodeRabbit が指摘、
//! 別 hook の weekly_review.rs にも同型ギャップが現存した) のように site ごとに再発し得た。
//!
//! [`SingleLineMessage`] は構築時に改行類をサニタイズし、systemMessage フィールドの型を
//! これに固定することで「多行を emit する」ことを構造的に不可能にする (ADR-042 の
//! ルール→仕組み化)。

use serde::Serialize;

/// 1 行であることが保証された文字列 (systemMessage 等のチャネル用、ADR-059)。
///
/// 構築時に改行類 (`\r\n` / `\n` / `\r`) を単一空白へサニタイズするため、内部値は必ず
/// 改行を含まない。`#[serde(transparent)]` により JSON 上は素の文字列として出力される
/// (systemMessage の wire 形式は従来どおり string)。
///
/// サニタイズは「本来 1 行であるべきチャネルへ、将来 producer が動的値 (ファイルパス /
/// エラー文字列等) を補間して誤って改行を混ぜた」場合の安全網。production は多行を emit せず、
/// 開発時 (debug) は [`SingleLineMessage::new`] の `debug_assert` が producer の混入を surface する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SingleLineMessage(String);

impl SingleLineMessage {
    /// 任意文字列から構築する。改行類はサニタイズされ、結果は必ず 1 行になる。
    ///
    /// debug ビルドでは入力に改行が含まれると `debug_assert` で fail する (producer 側で
    /// 1 行に保つべきという契約の明示)。release ビルドではサニタイズで fail-open に救済する。
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        debug_assert!(
            !raw.contains('\n') && !raw.contains('\r'),
            "SingleLineMessage: producer は 1 行を保つべき (改行はサニタイズで救済): {raw:?}"
        );
        Self(sanitize_single_line(&raw))
    }

    /// 内部の 1 行文字列を借用で返す。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 内部の 1 行文字列を所有権ごと取り出す。
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for SingleLineMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 改行類 (`\r\n` / `\n` / `\r`) を単一空白へ置換して 1 行化する。
///
/// `\r\n` を先に単一空白へ畳んでから残りの lone `\r` / `\n` を置換する (CRLF を二重空白に
/// しない)。tab や連続空白は単一行性に影響しないため保持する (最小限のサニタイズで予測可能性を優先)。
fn sanitize_single_line(s: &str) -> String {
    s.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_lf() {
        assert_eq!(sanitize_single_line("a\nb"), "a b");
    }

    #[test]
    fn sanitize_replaces_cr() {
        assert_eq!(sanitize_single_line("a\rb"), "a b");
    }

    #[test]
    fn sanitize_replaces_crlf_as_single_space() {
        assert_eq!(
            sanitize_single_line("a\r\nb"),
            "a b",
            "CRLF は二重空白にせず単一空白にする"
        );
    }

    #[test]
    fn sanitize_handles_mixed_and_multiple_breaks() {
        let out = sanitize_single_line("a\nb\r\nc\rd");
        assert_eq!(out, "a b c d");
        assert!(!out.contains('\n') && !out.contains('\r'));
    }

    #[test]
    fn sanitize_preserves_inner_whitespace() {
        assert_eq!(
            sanitize_single_line("hello  world\ttab"),
            "hello  world\ttab",
            "改行以外の空白 (連続空白 / tab) は単一行性に無関係なため保持する"
        );
    }

    #[test]
    fn new_keeps_clean_string_unchanged() {
        let m = SingleLineMessage::new("週次レビュー: 実行記録なし");
        assert_eq!(m.as_str(), "週次レビュー: 実行記録なし");
    }

    #[test]
    fn display_and_into_string_expose_inner() {
        let m = SingleLineMessage::new("abc");
        assert_eq!(m.to_string(), "abc");
        assert_eq!(m.into_string(), "abc");
    }

    #[test]
    fn serializes_transparently_as_plain_string() {
        let m = SingleLineMessage::new("ライン");
        assert_eq!(serde_json::to_string(&m).unwrap(), "\"ライン\"");
    }

    #[test]
    fn serializes_inside_option_and_struct() {
        #[derive(Serialize)]
        struct Out {
            #[serde(rename = "systemMessage", skip_serializing_if = "Option::is_none")]
            system_message: Option<SingleLineMessage>,
        }
        let some = Out {
            system_message: Some(SingleLineMessage::new("x")),
        };
        assert_eq!(
            serde_json::to_string(&some).unwrap(),
            r#"{"systemMessage":"x"}"#
        );
        let none = Out {
            system_message: None,
        };
        assert_eq!(serde_json::to_string(&none).unwrap(), "{}");
    }
}
