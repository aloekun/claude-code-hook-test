//! Merge Pipeline ランナー (スタンドアロン exe)
//!
//! pnpm merge-pr から呼び出され、PR のマージとローカル同期を実行します。
//! hooks-config.toml の [merge_pipeline] セクションから設定を読み込みます。
//!
//! 処理フロー:
//!   1. jj bookmark から現在の PR を自動検出
//!   2. [merge_pipeline.pre_steps] を順次実行（マージ前チェック）
//!   3. gh pr merge --squash を実行
//!   4. jj git fetch && jj new master でローカル同期
//!   5. [merge_pipeline.post_steps] を順次実行（学び提案等の拡張ポイント）
//!
//! 終了コード:
//!   0 - マージ成功 & ローカル同期完了
//!   1 - マージ失敗 / PR 検出失敗
//!   2 - 設定エラー

mod config;
mod feedback;
mod github;
mod pipeline;

fn main() {
    lib_jj_helpers::inject_git_dir_for_gh(pipeline::log_info);
    std::process::exit(pipeline::run_pipeline());
}
