//! 非主要拡張子の test coverage を**拡張子ごと**に判定する純関数群 (順位 498)。
//!
//! # 何を保証するか (保証の 3 層のうち第 1 層)
//!
//! rule が `extensions` に宣言した非主要拡張子それぞれについて、`other_ext_tests` に宣言された
//! テストのうち**その拡張子のファイルを通す検出テスト**が最低 1 件あること。
//!
//! - 拡張子は宣言から読まず、**テスト本体から読み取る**。宣言 (「このテストは ts 用」) を
//!   別に持つと、宣言と中身が食い違っても気づけない
//! - 読み取るのは `write_file(_, "name.ext", _)` と `violation_lines(_, "ext", _)` の第 2 引数の
//!   文字列リテラルだけ。コメント行は数えない
//! - 検出テストはテスト名に `detects` を含むもの (宣言済みテストは全件この規約に従い、
//!   assertion の形とも一致することを 2026-10-10 に確認した)
//! - rule が宣言していない拡張子を通すテスト (対象外の拡張子で発火しないことを確かめる
//!   テスト) は宣言に残してよい。カバレッジには数えない
//!
//! # 保証しないこと
//!
//! - 数えたテストが `config/custom-lint-rules.toml` の rule を使っているか (テスト側に
//!   rule を書き写していないか)。ADR-081 の写経禁止は別の検査が担う (未実装、台帳に起票済み)
//! - テストの assertion が十分か。それはテスト自身の責務 (第 2 層)
//!
//! I/O は持たない。ファイルの走査は [`super::coverage`] が行い、ソース文字列を渡す。

use std::collections::{BTreeSet, HashMap};

/// テスト関数名 → 関数本体。
///
/// 本体は `fn` 行から、その行と**同じインデントの `}` 行**までとする。文字列リテラル中の
/// 波括弧を数えずに済み、rustfmt された関数で境界がずれない。同名の関数が複数ある
/// 場合は本体を連結する (どれが宣言の対象か区別できないので、取りこぼさない側に倒す)。
pub(super) fn index_fn_bodies(sources: &[String]) -> HashMap<String, String> {
    let fn_line =
        regex::Regex::new(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<(]")
            .unwrap();
    let mut bodies: HashMap<String, String> = HashMap::new();
    for source in sources {
        let lines: Vec<&str> = source.lines().collect();
        for (start, line) in lines.iter().enumerate() {
            let Some(cap) = fn_line.captures(line) else {
                continue;
            };
            let closing = format!("{}}}", &cap[1]);
            let end = lines[start + 1..]
                .iter()
                .position(|l| *l == closing)
                .map_or(lines.len(), |offset| start + 1 + offset + 1);
            let body = lines[start..end].join("\n");
            bodies
                .entry(cap[2].to_string())
                .and_modify(|existing| {
                    existing.push('\n');
                    existing.push_str(&body);
                })
                .or_insert(body);
        }
    }
    bodies
}

/// テスト本体が通すファイルの拡張子 (小文字、先頭のドット無し)。
///
/// 複合拡張子 (`a.d.ts`) は最後の要素だけを見る。拡張子の無いファイル名は数えない。
pub(super) fn exercised_extensions(body: &str) -> BTreeSet<String> {
    let code: String = body
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let write_file = regex::Regex::new(r#"\bwrite_file\(\s*[^,]*?,\s*"([^"]*)""#).unwrap();
    let violation_lines =
        regex::Regex::new(r#"\bviolation_lines\(\s*[^,()]*,\s*"([^"]*)""#).unwrap();
    let from_file_names = write_file
        .captures_iter(&code)
        .filter_map(|cap| cap[1].rsplit_once('.').map(|(_, ext)| ext.to_string()));
    let from_ext_args = violation_lines
        .captures_iter(&code)
        .map(|cap| cap[1].trim_start_matches('.').to_string());
    from_file_names
        .chain(from_ext_args)
        .map(|ext| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .collect()
}

/// 検出テストか。テスト名に `detects` を含むものだけを数える。
pub(super) fn is_detection_test(name: &str) -> bool {
    name.contains("detects")
}

/// 1 つの rule について、非主要拡張子ごとの不足と、拡張子を読み取れない宣言テストを返す。
///
/// 宣言されたテストの関数が見つからない場合はここでは報告しない (関数の実在は
/// `rule_test_coverage_check` が検査している。二重に報告しない)。
pub(super) fn non_main_extension_gaps(
    rule_id: &str,
    non_main_exts: &[String],
    declared_tests: &[String],
    bodies: &HashMap<String, String>,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for test in declared_tests {
        let Some(body) = bodies.get(test) else {
            continue;
        };
        let exts = exercised_extensions(body);
        if exts.is_empty() {
            gaps.push(format!(
                "rule `{rule_id}` の宣言テスト `{test}` から拡張子を読み取れません \
                 (`write_file` / `violation_lines` の第 2 引数に拡張子付きの文字列リテラルが無い)。\
                 拡張子を読める書き方に揃えてください"
            ));
        } else if is_detection_test(test) {
            covered.extend(exts);
        }
    }
    for ext in non_main_exts {
        if !covered.contains(&ext.to_ascii_lowercase()) {
            gaps.push(format!(
                "rule `{rule_id}` は非主要拡張子 `{ext}` を宣言していますが、`.{ext}` を通す検出テスト \
                 (名前に `detects` を含み、`other_ext_tests` に宣言したもの) がありません"
            ));
        }
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(name, body)| ((*name).to_string(), (*body).to_string())).collect()
    }

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn extensions_are_read_from_both_helpers() {
        let body = r#"
            let file = write_file(dir.path(), "note.MD", "x");
            assert_eq!(violation_lines(RULE, "mjs", src), vec![1]);
            assert_eq!(violation_lines(RULE, ".Ts", src), vec![1]);
        "#;
        let expected: BTreeSet<String> = ["md", "mjs", "ts"].map(String::from).into();
        assert_eq!(exercised_extensions(body), expected);
    }

    #[test]
    fn a_helper_call_spanning_lines_is_read() {
        let body = "let file = write_file(\n    dir.path(),\n    \"x.jsonc\",\n    \"{}\",\n);";
        assert_eq!(exercised_extensions(body), ["jsonc".to_string()].into());
    }

    #[test]
    fn compound_extensions_use_the_last_part() {
        assert_eq!(
            exercised_extensions(r#"write_file(d, "types.d.ts", "")"#),
            ["ts".to_string()].into()
        );
    }

    /// コメントや helper 以外の文字列に出てくる拡張子は数えない。
    #[test]
    fn comments_and_other_strings_are_not_counted() {
        let body = r#"
            // write_file(dir.path(), "example.json", "x");
            let label = "example.json";
            let file = dir.path().join("test.ts");
        "#;
        assert!(exercised_extensions(body).is_empty());
    }

    #[test]
    fn file_names_without_an_extension_are_not_counted() {
        assert!(exercised_extensions(r#"write_file(d, "Makefile", "")"#).is_empty());
    }

    #[test]
    fn a_function_body_ends_at_the_closing_brace_of_the_same_indent() {
        let source = "fn a_detects() {\n    let s = \"}\";\n    write_file(d, \"x.md\", s);\n}\n\nfn b() {\n    write_file(d, \"y.ts\", \"\");\n}\n";
        let index = index_fn_bodies(&[source.to_string()]);
        assert_eq!(exercised_extensions(&index["a_detects"]), ["md".to_string()].into());
        assert_eq!(exercised_extensions(&index["b"]), ["ts".to_string()].into());
    }

    #[test]
    fn each_declared_extension_needs_its_own_detection_test() {
        let index = bodies(&[("x_detects_in_jsonc", r#"write_file(d, "a.jsonc", "")"#)]);
        let gaps = non_main_extension_gaps(
            "r",
            &names(&["jsonc", "json"]),
            &names(&["x_detects_in_jsonc"]),
            &index,
        );
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(gaps[0].contains("`json`"), "{gaps:?}");
    }

    /// 1 つのテストが複数の拡張子を通すなら、そのすべてを満たす。
    #[test]
    fn one_test_may_cover_several_extensions() {
        let index = bodies(&[(
            "x_detects_both",
            r#"write_file(d, "a.json", ""); write_file(d, "b.jsonc", "");"#,
        )]);
        let gaps = non_main_extension_gaps("r", &names(&["jsonc", "json"]), &names(&["x_detects_both"]), &index);
        assert!(gaps.is_empty(), "{gaps:?}");
    }

    /// 発火しないことを確かめるテストは、その拡張子のカバレッジにならない。
    #[test]
    fn a_non_detection_test_does_not_count() {
        let index = bodies(&[("x_skips_comment_in_sh", r#"violation_lines(R, "sh", src)"#)]);
        let gaps = non_main_extension_gaps("r", &names(&["sh"]), &names(&["x_skips_comment_in_sh"]), &index);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
    }

    /// rule が宣言していない拡張子を通すテストは、宣言に残してよい (カバレッジには数えない)。
    #[test]
    fn a_test_for_an_undeclared_extension_is_allowed() {
        let index = bodies(&[
            ("x_detects_in_ps1", r#"write_file(d, "a.ps1", "")"#),
            ("x_only_targets_ps1", r#"write_file(d, "a.ts", "")"#),
        ]);
        let gaps = non_main_extension_gaps(
            "r",
            &names(&["ps1"]),
            &names(&["x_detects_in_ps1", "x_only_targets_ps1"]),
            &index,
        );
        assert!(gaps.is_empty(), "{gaps:?}");
    }

    /// 拡張子を読み取れない宣言テストは黙って無視せず報告する (fail-closed)。
    #[test]
    fn a_declared_test_whose_extension_cannot_be_read_is_reported() {
        let index = bodies(&[
            ("x_detects_in_ts", r#"write_file(d, "a.ts", "")"#),
            ("x_detects_inline", r#"let file = dir.path().join("a.ts");"#),
        ]);
        let gaps = non_main_extension_gaps(
            "r",
            &names(&["ts"]),
            &names(&["x_detects_in_ts", "x_detects_inline"]),
            &index,
        );
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(gaps[0].contains("x_detects_inline") && gaps[0].contains("読み取れません"), "{gaps:?}");
    }

    #[test]
    fn extensions_are_compared_case_insensitively() {
        let index = bodies(&[("x_detects", r#"write_file(d, "A.PS1", "")"#)]);
        let gaps = non_main_extension_gaps("r", &names(&["Ps1"]), &names(&["x_detects"]), &index);
        assert!(gaps.is_empty(), "{gaps:?}");
    }
}
