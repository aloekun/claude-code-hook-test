//! Post-PR Monitor
//!
//! PR 作成と CI/CodeRabbit 監視を一貫して行うスタンドアロン CLI。
//! WP-17 PR 3 で single-shot モデルへ移行 (旧 Bb-2 の CronCreate park モデルは廃止 —
//! ADR-018 amendment)。checker を 1 回呼び、terminal action か「未確定 (pending_review /
//! rate_limited)」の報告で必ず終了する。未確定 PR の後続イベントは GitHub Actions 経路
//! (pr-monitor workflow の Phase A/B) が常時処理し、ローカルの時限 wakeup は使わない。
//!
//! モード:
//!   デフォルト (PR 作成): gh pr create → single-shot check → 報告
//!     pnpm create-pr -- --title "..." --body "..."
//!
//!   --monitor-only: PR が存在すれば single-shot check → 報告
//!     pnpm push 完了後にチェインで呼ばれる。手動再実行も可 (同一 PR + 同一 head なら
//!     state の時刻窓アンカーを継続する)
//!
//!   --mark-notified: state file の notified フラグを true にする
//!     Claude が結果を処理した後に呼ばれる
//!
//!   --prepare-pr-body: stdin の PR body を `.tmp-pr-body.md` (CWD) に書き出しパスを stdout に返す
//!     --prepare-pr-body-cleanup: `.tmp-pr-body.md` を削除する (旧 prepare-pr-body.ps1、WP-14)
//!
//! 終了コード:
//!   0 - 正常終了 (pending_review / rate_limited の保留報告を含む)
//!   1 - gh pr create 失敗 (PR 作成モードのみ) / prepare-pr-body の入力空・IO 失敗

mod classifier_runner;
mod config;
mod fix_commit;
mod lock;
mod log;
mod prepare_pr_body;
mod runner;
mod stages;
mod state;
mod util;

use prepare_pr_body::{run_prepare_pr_body, run_prepare_pr_body_cleanup};
use stages::{run_create_pr, run_mark_notified, run_monitor_only};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--prepare-pr-body-cleanup") {
        std::process::exit(run_prepare_pr_body_cleanup());
    }
    if args.iter().any(|a| a == "--prepare-pr-body") {
        std::process::exit(run_prepare_pr_body());
    }

    lib_jj_helpers::inject_git_dir_for_gh(log::log_info);

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
