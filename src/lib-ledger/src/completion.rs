//! 台帳タスクの「実装が完了しているか」を決定論的に判定する層。
//!
//! # 何をもって完了とするか
//!
//! 台帳の「対象ファイル」列が宣言する成果物**すべて**が、その変更で触られていること。
//! 一部でも欠けていれば未完了とする。
//!
//! この判定基準は実際の失敗から来ている。夜間 PR
//! [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) は lint rule の
//! 5 成果物のうち fixture 2 件だけを追加して CI green でマージされ、rule 本体が無いまま
//! 完了扱いになりかけた。「宣言のうち 1 つでも触れていれば完了」では同じ穴が開く。
//!
//! # 判定できない場合は完了にしない
//!
//! 対象ファイル列が機械可読の契約を満たさない場合 ([`super::parse_target_files`] が `Err`)、
//! 「検証不能」を返し完了とは言わない。曖昧さを完了側へ倒すと、通過してはいけない実装が
//! 通過する ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
//!
//! # ディレクトリ宣言の扱い
//!
//! 末尾が `/` の宣言はディレクトリを指し、その配下のいずれかが変更されていれば充足とする。
//! `{bad,good}` のような展開は [`super::parse_target_files`] が既に展開しており、
//! **展開結果それぞれが独立に充足を要求される** (bad だけ触って good を放置した実装は
//! 未完了になる)。

use std::collections::BTreeSet;

/// 1 順位ぶんの判定結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// 宣言された成果物がすべて変更されている。
    Complete,
    /// 宣言のうち触れられていないものがある。中身は未充足のパス (昇順)。
    Incomplete { missing: Vec<String> },
    /// 対象ファイル列を機械可読に解釈できず、判定そのものができない。
    Unverifiable { reason: String },
}

impl Completion {
    /// 後始末 (台帳からの削除) を許してよいか。
    ///
    /// [`Completion::Unverifiable`] は「安全とは言えない」であり `false` に倒す。
    pub fn allows_cleanup(&self) -> bool {
        matches!(self, Completion::Complete)
    }
}

/// 「対象ファイル」セルの宣言と、変更されたファイル一覧を突き合わせる。
///
/// `changed_files` はリポジトリ相対パス (`/` 区切り) の一覧。
pub fn evaluate(target_files_cell: &str, changed_files: &BTreeSet<String>) -> Completion {
    let declared = match super::parse_target_files(target_files_cell) {
        Ok(paths) => paths,
        Err(reason) => return Completion::Unverifiable { reason },
    };
    let missing: Vec<String> = declared
        .into_iter()
        .filter(|declared_path| !is_covered(declared_path, changed_files))
        .collect();
    if missing.is_empty() {
        Completion::Complete
    } else {
        Completion::Incomplete { missing }
    }
}

/// 宣言 1 件が変更一覧に含まれるか。
///
/// ディレクトリ宣言 (末尾 `/`) は前方一致、ファイル宣言は完全一致。前方一致を
/// ファイル宣言にも広げてはならない — `src/a.rs` の宣言が `src/a.rs.bak` の変更で
/// 充足してしまう。
fn is_covered(declared: &str, changed_files: &BTreeSet<String>) -> bool {
    if let Some(prefix) = declared.strip_suffix('/') {
        let prefix_with_separator = format!("{prefix}/");
        return changed_files
            .iter()
            .any(|changed| changed.starts_with(&prefix_with_separator));
    }
    changed_files.contains(declared)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn all_declared_files_changed_is_complete() {
        let result = evaluate(
            "`src/a.rs` + `docs/b.md`",
            &changed(&["src/a.rs", "docs/b.md"]),
        );
        assert_eq!(result, Completion::Complete);
        assert!(result.allows_cleanup());
    }

    /// #394 の再現: 宣言 5 件のうち fixture 2 件だけを触った変更は未完了。
    #[test]
    fn partial_implementation_is_incomplete() {
        let cell = "`.claude/custom-lint-rules.toml` + \
                    `src/hooks-post-tool-linter/src/custom_rules/rule_tests_extras.rs` + \
                    `tests/fixtures/incidents/{bad,good}/`";
        let result = evaluate(
            cell,
            &changed(&[
                "tests/fixtures/incidents/bad/x.toml",
                "tests/fixtures/incidents/good/x.toml",
            ]),
        );
        match result {
            Completion::Incomplete { missing } => assert_eq!(
                missing,
                vec![
                    ".claude/custom-lint-rules.toml",
                    "src/hooks-post-tool-linter/src/custom_rules/rule_tests_extras.rs"
                ]
            ),
            other => panic!("未完了と判定されない: {other:?}"),
        }
        assert!(!evaluate(cell, &changed(&["tests/fixtures/incidents/bad/x.toml"])).allows_cleanup());
    }

    /// 展開結果は独立に要求される。片方だけでは完了にしない。
    #[test]
    fn brace_expansion_requires_every_alternative() {
        let cell = "`tests/fixtures/incidents/{bad,good}/`";
        match evaluate(cell, &changed(&["tests/fixtures/incidents/bad/x.toml"])) {
            Completion::Incomplete { missing } => {
                assert_eq!(missing, vec!["tests/fixtures/incidents/good/"]);
            }
            other => panic!("片方だけで完了になっている: {other:?}"),
        }
    }

    #[test]
    fn directory_declaration_is_satisfied_by_any_file_beneath_it() {
        assert_eq!(
            evaluate("`tests/fixtures/`", &changed(&["tests/fixtures/deep/x.toml"])),
            Completion::Complete
        );
    }

    /// ディレクトリ宣言の前方一致が兄弟ディレクトリまで拾わないこと。
    /// `tests/fixtures/` の宣言が `tests/fixtures-old/x` で充足してはならない。
    #[test]
    fn directory_declaration_does_not_match_a_sibling_with_the_same_prefix() {
        match evaluate("`tests/fixtures/`", &changed(&["tests/fixtures-old/x.toml"])) {
            Completion::Incomplete { missing } => assert_eq!(missing, vec!["tests/fixtures/"]),
            other => panic!("兄弟ディレクトリで充足している: {other:?}"),
        }
    }

    /// ファイル宣言は完全一致。前方一致にすると `.bak` 等で充足してしまう。
    #[test]
    fn file_declaration_requires_an_exact_match() {
        match evaluate("`src/a.rs`", &changed(&["src/a.rs.bak"])) {
            Completion::Incomplete { missing } => assert_eq!(missing, vec!["src/a.rs"]),
            other => panic!("前方一致で充足している: {other:?}"),
        }
    }

    /// 解釈できないセルは完了にしない (削除も許さない)。
    #[test]
    fn unparseable_cell_is_unverifiable_and_blocks_cleanup() {
        let result = evaluate("`.claude/a.toml` + fixtures", &changed(&[".claude/a.toml"]));
        match &result {
            Completion::Unverifiable { reason } => assert!(reason.contains("引用の外"), "{reason}"),
            other => panic!("検証不能にならない: {other:?}"),
        }
        assert!(!result.allows_cleanup());
    }

    #[test]
    fn no_changes_at_all_is_incomplete() {
        match evaluate("`src/a.rs`", &changed(&[])) {
            Completion::Incomplete { missing } => assert_eq!(missing, vec!["src/a.rs"]),
            other => panic!("変更ゼロで完了になっている: {other:?}"),
        }
    }

    /// 宣言外のファイルを一緒に変更していても完了判定は変わらない
    /// (「宣言を満たすか」だけを見る。余計な変更の是非は人間のレビュー範囲)。
    #[test]
    fn extra_unrelated_changes_do_not_affect_the_verdict() {
        assert_eq!(
            evaluate("`src/a.rs`", &changed(&["src/a.rs", "README.md"])),
            Completion::Complete
        );
    }
}
