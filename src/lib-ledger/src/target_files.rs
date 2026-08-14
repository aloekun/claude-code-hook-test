//! 台帳の「対象ファイル」セルを機械可読なパス集合へ解釈する層。
//!
//! # なぜ厳格な書式を要求するのか
//!
//! 後続の後始末機構は「宣言された成果物がすべて変更されたか」で完了を判定する。判定材料が
//! 曖昧なままだと、**一部の成果物だけで完了と誤判定する**。実際に夜間 PR
//! [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) は lint rule の fixture
//! だけを追加して CI green でマージされ、rule 本体が無いまま完了扱いになりかけた。
//!
//! 2026-08-14 に現行 10 行を実査したところ、次の 2 型が機械照合に耐えなかった:
//!
//! - **裸のファイル名**: `` `main.rs` `` だけが書かれ、どの crate の main.rs か台帳から決まらない
//! - **引用符の無い成果物**: 「+ fixtures」のような散文。抽出できないため、**その成果物が
//!   欠けていても検証を通過する** (= #394 と同じ失敗モード)
//!
//! そこで「注釈を除いた本体は、リポジトリ相対パスのバッククォート引用と `+` だけ」を
//! 契約とし、外れたセルは解釈せず [`Err`] にする。曖昧さを許して読み飛ばすと、
//! 通過してはいけない実装が通過する側へ倒れる ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
//!
//! # 書式
//!
//! ```text
//! `src/a/b.rs` + `docs/c.md`（注釈は自由記述）+ `tests/fixtures/{bad,good}/`
//! ```
//!
//! - 丸括弧 (全角『（）』/ 半角) の中は**注釈**として全体を無視する。バッククォートを
//!   含んでもよい (例: 順位 284 の 「（`mod tests`、既存 `xxx` 拡張）」)
//! - 注釈を除いた本体に現れてよいのは、バッククォート引用・`+`・空白のみ
//! - 引用の中身はリポジトリ相対パス。先頭セグメントは [`ALLOWED_ROOTS`] のいずれか
//! - `{a,b}` は展開する。展開結果すべてが要求対象になる

use std::collections::BTreeSet;

/// リポジトリ相対パスとして認める先頭セグメント。
///
/// 裸のファイル名 (`main.rs`) を弾くための allowlist。ここに無いパスは「どこの何か
/// 決まらない」とみなしてエラーにする。リポジトリに新しい top-level ディレクトリを
/// 足したらここへ追加する。
const ALLOWED_ROOTS: &[&str] = &[
    ".claude",
    ".github",
    ".takt",
    "docs",
    "scripts",
    "src",
    "templates",
    "tests",
];

/// 「対象ファイル」セルからリポジトリ相対パスを取り出す。
///
/// 戻り値は重複を畳んだ昇順。`Err` は「このセルは機械照合に使えない」の意味で、
/// 呼び手は該当順位の自動削除を見送って人間へ回すこと。
pub fn parse_target_files(cell: &str) -> Result<Vec<String>, String> {
    let body = strip_annotations(cell)?;
    let quoted = extract_quoted_spans(&body)?;
    if quoted.is_empty() {
        return Err("パスが 1 件も宣言されていません".to_string());
    }
    let mut paths = BTreeSet::new();
    for span in quoted {
        for path in expand_braces(&span)? {
            validate_path(&path)?;
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}

/// 丸括弧の注釈を落とす。全角『（）』と半角 `()` の両方を扱う。
///
/// 対応が取れない括弧はエラーにする。閉じ忘れを黙って許すと、以降のセル全体が注釈として
/// 消え「パス 0 件」になり、**宣言が空 = 何も要求しない**という最も緩い判定へ倒れる。
fn strip_annotations(cell: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in cell.chars() {
        match ch {
            '（' | '(' => depth += 1,
            '）' | ')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("閉じ括弧が余分です: {cell:?}"))?;
            }
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("括弧が閉じていません: {cell:?}"));
    }
    Ok(out)
}

/// 本体からバッククォート引用を取り出す。
///
/// 引用の外に空白と `+` 以外が現れたらエラー。これが「引用符の無い成果物」(順位 334 の
/// 「+ fixtures」) を検出する唯一の砦で、ここを緩めると宣言漏れが素通りする。
fn extract_quoted_spans(body: &str) -> Result<Vec<String>, String> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut separated = true;
    for ch in body.chars() {
        if ch == '`' {
            if in_quote {
                spans.push(std::mem::take(&mut current));
                separated = false;
            } else if !separated {
                return Err(format!(
                    "成果物どうしは `+` で区切ってください (空白や連結だけでは不可): {body:?}"
                ));
            }
            in_quote = !in_quote;
            continue;
        }
        if in_quote {
            current.push(ch);
        } else if ch == '+' {
            separated = true;
        } else if !ch.is_whitespace() {
            return Err(format!(
                "引用の外に文字 {ch:?} があります (成果物はすべてバッククォートで囲むこと): {body:?}"
            ));
        }
    }
    if in_quote {
        return Err(format!("バッククォートが閉じていません: {body:?}"));
    }
    Ok(spans)
}

/// `{a,b}` を展開する。入れ子は扱わない (台帳に現れず、許すと組合せ爆発の判断が要る)。
fn expand_braces(span: &str) -> Result<Vec<String>, String> {
    let Some(open) = span.find('{') else {
        if span.contains('}') {
            return Err(format!("`}}` に対応する `{{` がありません: {span:?}"));
        }
        return Ok(vec![span.to_string()]);
    };
    let close = span
        .find('}')
        .ok_or_else(|| format!("`{{` が閉じていません: {span:?}"))?;
    if close < open {
        return Err(format!("`{{` と `}}` の順序が逆です: {span:?}"));
    }
    let head = &span[..open];
    let tail = &span[close + 1..];
    if tail.contains('{') || tail.contains('}') {
        return Err(format!("展開記法が 2 組以上あります: {span:?}"));
    }
    let inner = &span[open + 1..close];
    if inner.contains('{') {
        return Err(format!("展開記法が入れ子になっています: {span:?}"));
    }
    let alternatives: Vec<&str> = inner.split(',').map(str::trim).collect();
    if alternatives.iter().any(|a| a.is_empty()) {
        return Err(format!("展開記法に空の要素があります: {span:?}"));
    }
    Ok(alternatives
        .into_iter()
        .map(|a| format!("{head}{a}{tail}"))
        .collect())
}

/// リポジトリ相対パスとして妥当か検査する。
fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("空のパスがあります".to_string());
    }
    if path.contains('\\') {
        return Err(format!(
            "パス区切りは `/` に統一してください (Windows 形式は不可): {path:?}"
        ));
    }
    if path.starts_with('/') {
        return Err(format!("絶対パスは使えません: {path:?}"));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(format!("`..` を含むパスは使えません: {path:?}"));
    }
    let root = path.split('/').next().unwrap_or_default();
    if !ALLOWED_ROOTS.contains(&root) {
        return Err(format!(
            "リポジトリ相対パスではありません (先頭が {ALLOWED_ROOTS:?} のいずれでもない): {path:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(cell: &str) -> Vec<String> {
        parse_target_files(cell).unwrap_or_else(|e| panic!("{cell:?} が解釈できない: {e}"))
    }

    #[test]
    fn single_path() {
        assert_eq!(ok("`src/a/b.rs`"), vec!["src/a/b.rs"]);
    }

    #[test]
    fn multiple_paths_joined_by_plus() {
        assert_eq!(
            ok("`src/a.rs` + `docs/b.md`"),
            vec!["docs/b.md", "src/a.rs"],
            "戻り値は昇順"
        );
    }

    /// 注釈は括弧ごと落ちる。中にバッククォートがあっても宣言として数えない
    /// (順位 284 の「（`mod tests`、既存 `xxx` 拡張）」が実例)。
    #[test]
    fn annotations_are_ignored_even_when_they_contain_backticks() {
        assert_eq!(
            ok("`src/a.rs`（`mod tests`、既存 `some_test_fn` 拡張）"),
            vec!["src/a.rs"]
        );
    }

    #[test]
    fn half_width_parentheses_are_also_annotations() {
        assert_eq!(ok("`src/a.rs`(60・68 行)"), vec!["src/a.rs"]);
    }

    /// 実データ: 順位 340 のファイル名途中での展開。
    #[test]
    fn brace_expansion_inside_a_file_name() {
        assert_eq!(
            ok("`src/check-ci-coderabbit/src/{decide,main}.rs`"),
            vec![
                "src/check-ci-coderabbit/src/decide.rs",
                "src/check-ci-coderabbit/src/main.rs"
            ]
        );
    }

    /// 実データ: 順位 334 の正規化後の形 (ディレクトリ末尾 + 展開)。
    #[test]
    fn brace_expansion_on_directories() {
        assert_eq!(
            ok("`tests/fixtures/incidents/{bad,good}/`"),
            vec![
                "tests/fixtures/incidents/bad/",
                "tests/fixtures/incidents/good/"
            ]
        );
    }

    /// **本 module の存在理由**: 引用符の無い成果物を宣言漏れとして検出する。
    /// これを許すと「+ fixtures」が消え、fixture 無しの実装が完了判定を通る (#394 型)。
    #[test]
    fn unquoted_artifact_is_an_error() {
        let error = parse_target_files("`.claude/custom-lint-rules.toml` + fixtures")
            .expect_err("引用符の無い成果物は弾く");
        assert!(error.contains("引用の外"), "{error}");
    }

    /// 裸のファイル名はどのディレクトリか決まらない (順位 272 / 179 の正規化前の形)。
    #[test]
    fn bare_file_name_is_an_error() {
        let error = parse_target_files("`main.rs`").expect_err("裸のファイル名は弾く");
        assert!(error.contains("リポジトリ相対パス"), "{error}");
    }

    #[test]
    fn windows_separator_is_an_error() {
        assert!(parse_target_files(r"`src\a\b.rs`").is_err());
    }

    #[test]
    fn absolute_path_and_parent_traversal_are_errors() {
        assert!(parse_target_files("`/etc/passwd`").is_err());
        assert!(parse_target_files("`src/../../etc/passwd`").is_err());
    }

    /// 宣言 0 件は「何も要求しない」= 最も緩い判定になるため、空セルはエラー。
    #[test]
    fn empty_declaration_is_an_error() {
        assert!(parse_target_files("").is_err());
        assert!(parse_target_files("（注釈だけ）").is_err());
    }

    #[test]
    fn unbalanced_delimiters_are_errors() {
        assert!(strip_annotations("`a`（閉じない").is_err());
        assert!(strip_annotations("`a`）余分").is_err());
        assert!(parse_target_files("`src/a.rs").is_err(), "引用が閉じない");
        assert!(parse_target_files("`src/{a.rs`").is_err(), "展開が閉じない");
        assert!(parse_target_files("`src/a},b.rs`").is_err(), "対応しない閉じ括弧");
    }

    #[test]
    fn nested_or_multiple_brace_groups_are_errors() {
        assert!(parse_target_files("`src/{a,b}/{c,d}.rs`").is_err());
    }

    /// 入れ子の `{` は展開部の中に残り、`src/{b.rs` という壊れたパスを生む。
    /// `validate_path` は `{` を見ないため、そのまま「変更されていない成果物」として
    /// 要求され続け、原因が読み取れない失敗になる。展開の時点で止める。
    #[test]
    fn brace_group_containing_another_open_brace_is_an_error() {
        let error = parse_target_files("`src/{a,{b}.rs`").expect_err("入れ子は弾く");
        assert!(error.contains("入れ子"), "{error}");
    }

    /// 契約は「複数の成果物は `+` で並べる」。空白区切り・連結を受理していると、
    /// 実装が文書より緩い状態になり、書式の説明が事実と食い違う。
    #[test]
    fn adjacent_quotes_without_a_plus_separator_are_errors() {
        for body in ["`src/a.rs` `docs/b.md`", "`src/a.rs``docs/b.md`"] {
            let error = parse_target_files(body).expect_err("{body:?} は `+` 区切りが要る");
            assert!(error.contains("`+` で区切って"), "{body:?}: {error}");
        }
    }

    /// `+` の前後の空白は自由。区切りが在ることだけを要求する。
    #[test]
    fn plus_separator_tolerates_surrounding_whitespace() {
        assert_eq!(
            ok("`src/a.rs`+`docs/b.md`"),
            vec!["docs/b.md", "src/a.rs"],
            "空白なしの `+` も受理する"
        );
    }

    #[test]
    fn empty_brace_alternative_is_an_error() {
        assert!(parse_target_files("`src/{a,}.rs`").is_err());
    }

    /// 同じパスを 2 回宣言しても要求は 1 件。
    #[test]
    fn duplicate_paths_are_folded() {
        assert_eq!(ok("`src/a.rs` + `src/a.rs`"), vec!["src/a.rs"]);
    }
}
