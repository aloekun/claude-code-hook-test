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
pub mod preamble;

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
