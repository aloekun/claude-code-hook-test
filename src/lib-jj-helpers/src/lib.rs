//! jj helper utilities shared across cli-* crates.
//!
//! ADR-021 (jj 変更検出の原則、特に原則 5 の bookmark 検出) と
//! ADR-024 (共通 jj ヘルパーライブラリ、本採用) で定めた shared primitives
//! の実装。
//!
//! # 設計方針
//!
//! - **副作用 (jj subprocess、log 出力) は呼び出し側から注入**
//! - **stderr ハンドリングは `StderrMode` で選択**: cli-pr-monitor は `Silent`
//!   (CI ログの汚染回避)、cli-merge-pipeline は `Piped` (失敗原因の診断)
//! - **ログ prefix は呼び出し側の `log_info` 関数に委譲**: 各クレート固有の
//!   prefix (`[post-pr-monitor]` / `[merge-pipeline]` 等) を崩さない
//!
//! # モジュール構成
//!
//! - [`bookmarks`]: trunk 判定 / revset 走査 / bookmark 取得 (ローカル + リモート追跡)
//! - [`workspace`]: jj workspace layout の解釈 (`GIT_DIR` 導出 / メイン root 解決、ADR-045)
//! - [`pipeline_lock`]: 実行中 pipeline と Stop hook 品質ゲートの相互排他 (順位 280)
//!
//! [`bookmarks`] と [`workspace`] の公開 API はクレート直下へ再エクスポートしており、
//! `lib_jj_helpers::get_jj_bookmarks` のような従来のパスがそのまま使える。
//! [`pipeline_lock`] は再エクスポートせず、モジュールパス
//! (`lib_jj_helpers::pipeline_lock::hold_pipeline_lock`) で使う。
//!
//! # 公開 API
//!
//! - [`TRUNK_BOOKMARKS`] / [`is_trunk_bookmark`]: trunk 系 bookmark 判定 (3 クレート共有)
//! - [`BOOKMARK_SEARCH_REVSETS`]: `@` / `@-` / `@--` の優先順位リスト
//! - [`StderrMode`]: jj サブプロセスの stderr 方針
//! - [`parse_bookmark_list_output`]: `jj log` テンプレート出力のパース (pure)
//! - [`query_bookmarks_at`] / [`query_remote_bookmarks_at`]: 指定 revset の bookmark 取得 (I/O)
//! - [`select_from_revsets`] / [`select_with_remote_fallback`]: revset リストを優先順に試す pure function
//! - [`get_jj_bookmarks`]: 上記を組み合わせた high-level エントリポイント
//! - [`get_jj_bookmarks_with_remote_fallback`] / [`BookmarkSearch`]: リモート専用 bookmark も
//!   探索対象に含める版 (bot が remote に作った PR の検出、順位 397)
//! - [`resolve_git_dir`] / [`inject_git_dir_for_gh`]: 非 colocated jj workspace
//!   での gh 用 `GIT_DIR` 導出と自動注入 (ADR-045 恒久対策候補 1)
//! - [`resolve_main_workspace_root`]: secondary jj workspace から canonical な (メイン)
//!   workspace root を解決 (gitignore 済み untracked 状態ファイルの workspace 分裂対策、ADR-045)

pub mod bookmarks;
pub mod pipeline_lock;
pub mod workspace;

pub use bookmarks::{
    classify_advance_target, get_jj_bookmarks, get_jj_bookmarks_with_remote_fallback,
    is_trunk_bookmark, parse_bookmark_list_output, query_bookmarks_at,
    query_remote_bookmarks_at, select_from_revsets, select_with_remote_fallback, AdvanceTarget,
    BookmarkSearch, StderrMode, ADVANCE_TARGET_REVSET, BOOKMARK_SEARCH_REVSETS,
    REMOTE_BOOKMARK_SEARCH_REVSETS,
    TRUNK_BOOKMARKS,
};
pub use workspace::{
    inject_git_dir_for_gh, is_inside_workspace, list_workspace_roots, resolve_git_dir,
    resolve_main_workspace_root, GitDirResolution,
};
