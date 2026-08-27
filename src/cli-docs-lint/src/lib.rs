//! cli-docs-lint — docs/ 整合性チェッカー
//!
//! 順位 95 (preamble file count 自動照合) と順位 96 (Markdown cross-reference
//! validator) を統合した CLI。push-runner-config.toml の quality_gate.lint
//! group から `pnpm lint:docs` 経由で実行される。
//!
//! 検査内容:
//! - **preamble**: `docs/todoN.md` の preamble に書かれた Kanji 数詞 (X つ) が
//!   実 `docs/todo*.md` ファイル数と一致するか
//! - **cross-ref**: `docs/**/*.md` 内の relative link が directory-aware で
//!   resolve できるか (broken link 検出)
//!
//! PR #133 で検出された 2 種類の docs 整合性問題を機械的に再発防止する。

pub mod cross_ref;
pub mod docs_files;
pub mod entry_pairing;
pub mod preamble;
pub mod priority_inversion;

use std::fmt;

/// 単一の違反を表す共通型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

#[cfg(test)]
mod shared_summary_definition_tests {
    //! 順位 table の prefix 定義が **1 箇所** ([`docs_files::SUMMARY_FILE_PREFIX`]) であることを、
    //! 3 validator の**挙動**で固定する。
    //!
    //! 値の一致 (`assert_eq!(A, B)`) では固定できない — 統合前は 3 箇所とも同じ値だったが、
    //! 片方だけ書き換えられた瞬間に validator の走査範囲がずれる、という形の事故だった。
    //! ここでは**新しい分割 part (`todo-summary3.md`) を 3 validator がそろって認識する**
    //! ことを見る (defect-convergence-plan.md § Phase F の F1)。

    use super::{entry_pairing, preamble, priority_inversion};

    const TABLE_HEADER: &str = "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                                |---|---|---|---|---|---|\n";

    /// 3 番目の分割 part を持つ docs ディレクトリ。
    ///
    /// - `todo-summary3.md` だけが 順位 20 を宣言し、その詳細エントリは **無い**
    ///   (entry_pairing の方向 A が拾うべき状態)
    /// - 順位 20 (Tier 1) が 順位 30 (Tier 2) に依存する (priority_inversion が拾うべき状態)
    /// - `todo1.md` の preamble は summary を除いた件数 (1 つ) を宣言する
    ///   (preamble が `todo-summary3.md` を summary と数えて初めて通る)
    fn docs_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let files = [
            (
                "todo-summary.md",
                format!("{TABLE_HEADER}| 30 | 🔧 Tier 2 | **C** | todo1.md | S | なし |\n"),
            ),
            (
                "todo-summary3.md",
                format!("{TABLE_HEADER}| 20 | 🚀 Tier 1 | **B** | todo1.md | S | 順位 30 待ち |\n"),
            ),
            (
                "todo1.md",
                "# TODO\n\n\
                 > 新セッションでは一つすべてを確認すること (todo.md)。\n\n\
                 ### 順位 30: C\n\n> **動機**: x\n\n#### 完了基準\n\n- done\n"
                    .to_string(),
            ),
        ];
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).expect("write");
        }
        dir
    }

    #[test]
    fn entry_pairing_reads_the_third_summary_part() {
        let dir = docs_dir();
        let violations = entry_pairing::check(dir.path()).expect("check");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].file.contains("todo-summary3.md"), "{:?}", violations[0]);
        assert!(violations[0].message.contains("順位 20"), "{:?}", violations[0]);
    }

    #[test]
    fn priority_inversion_reads_the_third_summary_part() {
        let dir = docs_dir();
        let violations = priority_inversion::check(dir.path()).expect("check");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].file.contains("todo-summary3.md"), "{:?}", violations[0]);
    }

    #[test]
    fn preamble_counts_the_third_summary_part_as_a_summary() {
        let dir = docs_dir();
        let violations = preamble::check(dir.path()).expect("check");
        assert!(violations.is_empty(), "{violations:?}");
    }
}
