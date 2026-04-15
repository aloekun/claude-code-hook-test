//! Post-PR Monitor (スタンドアロン exe)
//!
//! PR 作成と監視を一貫して行うスタンドアロン CLI。
//! push-pipeline と同じ「ガード + 専用コマンド」パターンで動作する。
//!
//! モード:
//!   デフォルト (PR 作成): gh pr create を実行 → daemon 起動 → CronCreate 指示を stdout 出力
//!     pnpm create-pr -- --title "..." --body "..."
//!
//!   --monitor-only: PR が存在すれば daemon 起動、なければ exit 0
//!     pnpm push 完了後にチェインで呼ばれる
//!
//!   --daemon: バックグラウンドで check-ci-coderabbit.exe をポーリングし state file を更新
//!     PR Create / Monitor-Only から自動スポーンされる
//!
//!   --mark-notified: state file の notified フラグを true にする
//!     Claude が結果を処理した後に呼ばれる
//!
//! 終了コード:
//!   0 - 正常終了
//!   1 - gh pr create 失敗 (PR 作成モードのみ)

mod config;
mod log;
mod runner;
mod stages;
mod state;
mod util;

use std::path::PathBuf;

use stages::{run_create_pr, run_daemon, run_mark_notified, run_monitor_only};
use state::state_file_path;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--daemon") {
        let state_file = args
            .iter()
            .position(|a| a == "--state-file")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .unwrap_or_else(state_file_path);
        std::process::exit(run_daemon(&state_file));
    }

    if args.iter().any(|a| a == "--mark-notified") {
        std::process::exit(run_mark_notified());
    }

    if args.iter().any(|a| a == "--monitor-only") {
        std::process::exit(run_monitor_only());
    }

    // -- 以降の引数を gh pr create に転送
    let gh_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        args[1..].to_vec()
    };

    std::process::exit(run_create_pr(&gh_args));
}
