//! Post-PR Monitor
//!
//! PR 作成と CI/CodeRabbit 監視を一貫して行うスタンドアロン CLI。
//! Bb-2 で single-iteration + CronCreate park モデルに移行。
//!
//! モード:
//!   デフォルト (PR 作成): gh pr create → 初回 review_recheck park → (wakeup で) takt 分析
//!     pnpm create-pr -- --title "..." --body "..."
//!
//!   --monitor-only: PR が存在すれば single-iteration check → (wakeup なら) park / 終端
//!     pnpm push 完了後および CronCreate wakeup でチェインで呼ばれる
//!
//!   --mark-notified: state file の notified フラグを true にする
//!     Claude が結果を処理した後に呼ばれる
//!
//! 終了コード:
//!   0 - 正常終了 (park 含む、PARK signal は stdout に出力済)
//!   1 - gh pr create 失敗 (PR 作成モードのみ)

mod classifier_runner;
mod config;
mod fix_commit;
mod lock;
mod log;
mod runner;
mod stages;
mod state;
mod util;

use stages::{run_create_pr, run_mark_notified, run_monitor_only};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--mark-notified") {
        std::process::exit(run_mark_notified());
    }

    if args.iter().any(|a| a == "--monitor-only") {
        std::process::exit(run_monitor_only());
    }

    let gh_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        args[1..].to_vec()
    };

    std::process::exit(run_create_pr(&gh_args));
}
