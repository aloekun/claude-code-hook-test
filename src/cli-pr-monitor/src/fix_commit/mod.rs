//! 分離型 fix commit の pre-create と description 生成。
//!
//! ADR-022 例外条項 (2026-04-20): 自動生成された修正を独立した child commit として
//! 分離する場合に限り、その child commit への description 付与を許可する。
//! 元 commit (= 人間が意図を込めた初回 PR commit) の description は改変しない。
//!
//! pre-takt で `jj new -m "..."` により空 child を作成し、takt が `@` を amend する
//! ことで fix 内容が自動的に child commit へ入る仕組み。

mod abandon;
mod description;
mod sweep;

pub(crate) use abandon::{create_fix_commit, try_abandon_empty_fix_commit};
pub(crate) use description::FixCommitState;
pub(crate) use sweep::sweep_empty_commits_in_pr_range;
