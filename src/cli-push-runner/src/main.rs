//! Push Runner — takt ベースの pre-push パイプライン
//!
//! pnpm push から呼び出され、以下のステージを実行する:
//!   Stage 1:   quality_gate — TOML で定義されたコマンド群をグループ間で並列実行
//!   Stage 1.5: diff         — jj diff を取得しファイルに書き出し（reviewers が Read で参照）
//!   Stage 2:   takt         — AI レビュー（reviewers → fix loop）
//!   Stage 3:   push         — jj git push
//!
//! push 成功後は pnpm スクリプトチェーンにより cli-pr-monitor が起動される。
//!
//! 終了コード:
//!   0 - 全ステージ成功
//!   1 - quality_gate 失敗
//!   2 - takt ワークフロー失敗
//!   3 - push 失敗
//!   4 - 設定エラー
//!   5 - diff 取得失敗

mod config;
mod log;
mod runner;
mod stages;

use std::time::Instant;

use config::load_config;
use log::log_info;
use stages::{run_diff, run_push, run_quality_gate, run_takt, DiffResult};

const EXIT_SUCCESS: i32 = 0;
const EXIT_QUALITY_GATE_FAILURE: i32 = 1;
const EXIT_TAKT_FAILURE: i32 = 2;
const EXIT_PUSH_FAILURE: i32 = 3;
const EXIT_CONFIG_ERROR: i32 = 4;
const EXIT_DIFF_FAILURE: i32 = 5;

fn run_pipeline() -> i32 {
    let start = Instant::now();

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            log_info(&format!("設定エラー: {}", e));
            return EXIT_CONFIG_ERROR;
        }
    };

    let has_diff = config.diff.is_some();
    log_info(&format!(
        "パイプライン開始: quality_gate → {} takt ({}) → push",
        if has_diff { "diff →" } else { "" },
        config.takt.workflow,
    ));

    // Stage 1: quality_gate
    if !run_quality_gate(&config.quality_gate) {
        log_info("パイプライン中断: quality_gate 失敗。問題を修正して再実行してください。");
        return EXIT_QUALITY_GATE_FAILURE;
    }

    // Stage 1.5: diff
    let mut skip_takt = false;
    if let Some(diff_config) = &config.diff {
        match run_diff(diff_config) {
            DiffResult::HasContent => {}
            DiffResult::Empty => {
                log_info("diff が空のためレビューをスキップして push に進みます。");
                skip_takt = true;
            }
            DiffResult::Error => {
                log_info("パイプライン中断: diff 取得失敗。");
                return EXIT_DIFF_FAILURE;
            }
        }
    }

    // Stage 2: takt
    if !skip_takt && !run_takt(&config.takt) {
        log_info("パイプライン中断: takt ワークフロー失敗。");
        return EXIT_TAKT_FAILURE;
    }

    // Stage 3: push
    if !run_push(&config.push) {
        log_info("パイプライン中断: push 失敗。");
        return EXIT_PUSH_FAILURE;
    }

    let elapsed = start.elapsed();
    log_info(&format!("パイプライン完了 ({:.0}s)", elapsed.as_secs_f64()));
    EXIT_SUCCESS
}

fn main() {
    std::process::exit(run_pipeline());
}
