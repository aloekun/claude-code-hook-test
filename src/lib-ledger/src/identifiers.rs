//! 台帳の内容欄が名指す識別子を取り出し、宣言先から見た状態へ分類する層 (I/O なし)。
//!
//! [`crate::deployed_ledger`] の検査 B が使う。索引の作り方は [`crate::repo_index`]。

use std::collections::BTreeSet;

/// 内容欄が名指す識別子の、宣言先から見た状態。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IdentifierState {
    /// 宣言先に在る。健全。
    Declared,
    /// **宣言先には無いが、リポジトリの他所には在る** = 漂流。
    Drifted,
    /// リポジトリのどこにも無い = これから作る成果物。検査対象外。
    NotYetCreated,
}

/// 内容欄のバッククォート引用から、照合対象の識別子を取り出す (I/O なし)。
///
/// **Rust 識別子の形だけを採る。** 内容欄のバッククォートには識別子以外も入る —
/// 型の一部 (`Option::None`)、CLI 引数 (`--pr 0`)、パス (`src/foo.rs`)、
/// 文字列リテラル (`"custom-block"`)。2026-08-25 の実測では、素朴に全部を識別子として
/// 扱うと 30 行中 12 行が偽陽性になり、この絞り込みで 3 行まで落ちた。
///
/// 末尾の `(...)` は落とす (`some_fn(&str)` → `some_fn`)。**例に実在の識別子を書かないこと** —
/// 本ファイル自身がリポジトリ索引に入るため、台帳が名指す識別子をここへ書くと
/// 「これから作る」行が「リポジトリに在る」= 漂流へ誤分類される (2026-08-25 に実際に踏んだ)。
/// 3 文字以下は一般語との衝突が多いので採らない。
pub(crate) fn content_identifiers(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for span in backtick_spans(content) {
        let bare = match span.split_once('(') {
            Some((head, _)) => head,
            None => span.as_str(),
        };
        if bare.len() > 3 && is_rust_identifier(bare) {
            out.push(bare.to_string());
        }
    }
    out
}

/// バッククォートで囲まれた区間を列挙する (I/O なし)。閉じていない引用は捨てる。
pub(crate) fn backtick_spans(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        out.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    out
}

pub(crate) fn is_rust_identifier(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 識別子 1 件を分類する (I/O なし。ファイルの中身は呼び出し元が読んで渡す)。
///
/// **「リポジトリのどこにも無い」を漂流に数えない**のが要点。台帳は*これからやる作業*を
/// 書く場所なので、内容欄が名指す識別子には「既存コード (漂流の signal)」と
/// 「これから作るもの (ただの予定)」が混ざる。両者を構文では見分けられないが、
/// **リポジトリ全体に在るかどうか**が決定的な差になる。
pub(crate) fn classify_identifier(identifier: &str, declared: &str, repository: &str) -> IdentifierState {
    if contains_token(declared, identifier) {
        IdentifierState::Declared
    } else if contains_token(repository, identifier) {
        IdentifierState::Drifted
    } else {
        IdentifierState::NotYetCreated
    }
}

/// `haystack` が `identifier` を**トークンとして**含むか (I/O なし)。
///
/// 素の [`str::contains`] は部分一致なので、`render_row` が `render_rows` に当たる。
/// これは**漂流を見逃す**向きに効く — 宣言先から `render_row` が消えても、同じファイルに
/// `render_rows` が残っていれば `Declared` と読んでしまう (CodeRabbit #447)。
/// 前後が識別子文字でないことを確かめて、接頭辞・接尾辞一致を弾く。
pub(crate) fn contains_token(haystack: &str, identifier: &str) -> bool {
    let bytes = haystack.as_bytes();
    let width = identifier.len();
    haystack.match_indices(identifier).any(|(start, _)| {
        let before_ok = start == 0 || !is_identifier_byte(bytes[start - 1]);
        let after = start + width;
        let after_ok = after >= bytes.len() || !is_identifier_byte(bytes[after]);
        before_ok && after_ok
    })
}

pub(crate) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// 注意欄の照合除外マーカー。
///
/// 書式: `照合除外: ` + バッククォート識別子 + `（理由）`。**理由は必須**で、
/// 空だと [`Err`] に倒す — 理由の無い除外は「なぜ通しているか分からない穴」になり、
/// 検査を骨抜きにする経路がそこだけ無検査になる。
///
/// 除外を台帳の行に置くのは、行を削除すれば除外も一緒に消えるため。テスト側の allowlist に
/// 置くと、台帳から行が消えても除外だけが残って腐る。
pub(crate) const REVIEW_EXCLUSION_MARKER: &str = "照合除外:";

/// 注意欄から照合除外の識別子を読む (I/O なし)。理由が無ければ `Err`。
pub(crate) fn parse_review_exclusions(note: &str) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for chunk in note.split(REVIEW_EXCLUSION_MARKER).skip(1) {
        let Some(identifier) = backtick_spans(chunk).into_iter().next() else {
            return Err(format!(
                "{REVIEW_EXCLUSION_MARKER} の後にバッククォート引用の識別子がありません"
            ));
        };
        let after_ident = chunk
            .split_once(&format!("`{identifier}`"))
            .map(|(_, tail)| tail)
            .unwrap_or("");
        if reason_of(after_ident).is_empty() {
            return Err(format!(
                "照合除外 `{identifier}` に理由 (全角丸括弧) がありません"
            ));
        }
        out.insert(identifier);
    }
    Ok(out)
}

/// 除外マーカー直後の全角丸括弧から理由を読む (I/O なし)。
pub(crate) fn reason_of(after_identifier: &str) -> String {
    let Some(open) = after_identifier.find('（') else {
        return String::new();
    };
    let tail = &after_identifier[open + '（'.len_utf8()..];
    let Some(close) = tail.find('）') else {
        return String::new();
    };
    tail[..close].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 識別子でないバッククォートは照合対象にしない (2026-08-25 実測の偽陽性 7 種)。
    #[test]
    fn only_rust_identifiers_are_collected_from_the_content_cell() {
        let content = "`Option::None` と `--pr 0` と `src/foo.rs` と `\"custom-block\"` を直す";
        assert!(content_identifiers(content).is_empty(), "{:?}", content_identifiers(content));
    }

    /// 末尾の引数リストは落として本体だけを採る。
    #[test]
    fn a_trailing_argument_list_is_stripped() {
        assert_eq!(content_identifiers("`render_row(&str)` を pub 化"), vec!["render_row"]);
    }

    /// 3 文字以下は一般語と衝突するので採らない。
    #[test]
    fn very_short_identifiers_are_ignored() {
        assert!(content_identifiers("`id` と `run` を直す").is_empty());
    }

    /// 閉じていないバッククォートで後続を巻き込まない。
    #[test]
    fn an_unclosed_backtick_does_not_swallow_the_rest() {
        assert_eq!(content_identifiers("`alpha` と `beta"), vec!["alpha"]);
    }

    /// **検査 B の核**: 宣言先に無く、リポジトリの他所に在れば漂流。
    #[test]
    fn an_identifier_missing_from_the_declared_path_but_present_elsewhere_is_drift() {
        assert_eq!(
            classify_identifier("check_todo_staleness", "fn main() {}", "fn check_todo_staleness()"),
            IdentifierState::Drifted
        );
    }

    /// **これから作る識別子は漂流ではない** — 台帳は未着手の作業を書く場所なので、
    /// ここを漂流に数えると未着手行がすべて赤くなる (実測 30 行中 12 行)。
    #[test]
    fn an_identifier_absent_from_the_whole_repository_is_not_yet_created() {
        assert_eq!(
            classify_identifier("brand_new_helper", "fn main() {}", "fn main() {}"),
            IdentifierState::NotYetCreated
        );
    }

    /// **接頭辞一致で漂流を見逃さない** (CodeRabbit #447)。素の `contains` だと
    /// `render_row` が `render_rows` に当たり、宣言先から消えていても `Declared` と読む。
    #[test]
    fn a_longer_name_sharing_the_prefix_does_not_count_as_a_match() {
        assert_eq!(
            classify_identifier("render_row", "fn render_rows() {}", "fn render_row() {}"),
            IdentifierState::Drifted
        );
    }

    /// 接尾辞側も同じ。`row_id` が `first_row_id` に当たってはいけない。
    #[test]
    fn a_longer_name_sharing_the_suffix_does_not_count_as_a_match() {
        assert!(!contains_token("let first_row_id = 1;", "row_id"));
    }

    /// 識別子文字でない区切り (`::` / `(` / 行頭行末) は境界として通す。
    #[test]
    fn non_identifier_neighbours_are_valid_boundaries() {
        assert!(contains_token("std::env::current_dir()", "current_dir"));
        assert!(contains_token("current_dir", "current_dir"));
        assert!(contains_token("fn current_dir(", "current_dir"));
    }

    /// 同じ行に接頭辞一致と真の一致が混在しても検出できる。
    #[test]
    fn a_true_match_after_a_prefix_match_is_still_found() {
        assert!(contains_token("render_rows(); render_row();", "render_row"));
    }

    #[test]
    fn an_identifier_present_at_the_declared_path_is_declared() {
        assert_eq!(
            classify_identifier("alpha", "fn alpha() {}", "fn alpha() {}"),
            IdentifierState::Declared
        );
    }

    #[test]
    fn a_note_without_a_marker_excludes_nothing() {
        assert_eq!(parse_review_exclusions("ふつうの注意書き"), Ok(BTreeSet::new()));
    }

    #[test]
    fn a_marker_with_a_reason_excludes_the_identifier() {
        let note = "検出対象の説明。照合除外: `current_dir`（lint rule の検出対象であって成果物ではない）";
        assert_eq!(
            parse_review_exclusions(note),
            Ok(BTreeSet::from(["current_dir".to_string()]))
        );
    }

    /// **理由の無い除外は拒否する。** 通すと「なぜ通しているか分からない穴」が残り、
    /// 検査を骨抜きにする経路がそこだけ無検査になる。
    #[test]
    fn a_marker_without_a_reason_is_rejected() {
        let error = parse_review_exclusions("照合除外: `current_dir`").unwrap_err();
        assert!(error.contains("理由"), "{error}");
    }

    #[test]
    fn a_marker_without_an_identifier_is_rejected() {
        let error = parse_review_exclusions("照合除外: current_dir（理由）").unwrap_err();
        assert!(error.contains("識別子"), "{error}");
    }
}
