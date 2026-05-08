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
use stages::{run_diff, run_lint_screen, run_push, run_quality_gate, run_takt, DiffResult};

const EXIT_SUCCESS: i32 = 0;
const EXIT_QUALITY_GATE_FAILURE: i32 = 1;
const EXIT_TAKT_FAILURE: i32 = 2;
const EXIT_PUSH_FAILURE: i32 = 3;
const EXIT_CONFIG_ERROR: i32 = 4;
const EXIT_DIFF_FAILURE: i32 = 5;

/// diff stage を実行し lint-screen を呼び出す。
/// Ok(skip_takt) で成功、 Err(exit_code) で pipeline 中断。
fn run_diff_and_lint_screen(config: &config::Config) -> Result<bool, i32> {
    let Some(diff_config) = &config.diff else {
        return Ok(false);
    };
    let diff_path = match run_diff(diff_config) {
        DiffResult::HasContent => diff_config.output_path.as_str(),
        DiffResult::Empty => {
            log_info("diff が空のためレビューをスキップして push に進みます。");
            return Ok(true);
        }
        DiffResult::Error => {
            log_info("パイプライン中断: diff 取得失敗。");
            return Err(EXIT_DIFF_FAILURE);
        }
    };
    if let Some(lint_screen_config) = &config.lint_screen {
        run_lint_screen(lint_screen_config, diff_path);
    }
    Ok(false)
}

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

    if !run_quality_gate(&config.quality_gate) {
        log_info("パイプライン中断: quality_gate 失敗。問題を修正して再実行してください。");
        return EXIT_QUALITY_GATE_FAILURE;
    }

    let skip_takt = match run_diff_and_lint_screen(&config) {
        Ok(skip) => skip,
        Err(code) => return code,
    };

    if !skip_takt && !run_takt(&config.takt) {
        log_info("パイプライン中断: takt ワークフロー失敗。");
        return EXIT_TAKT_FAILURE;
    }

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
