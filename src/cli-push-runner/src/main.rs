//! Push Runner — takt ベースの pre-push パイプライン
//!
//! pnpm push から呼び出され、以下のステージを実行する:
//!   Stage -1:  bookmark_check — 非 trunk bookmark の存在を確認 (順位 2)
//!   Stage 0:   scratch_file_warning — `__*` 等の scratch ファイル混入を検査 (順位 1)
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
//!   6 - scratch_file_warning 検出 (override env で bypass 可能)
//!   7 - bookmark_check 非 trunk bookmark 未設定
//!   8 - pr_size_check が block_threshold 超過 (override env で bypass 可能)

mod config;
mod log;
mod runner;
mod stages;

use std::time::Instant;

use config::{load_config, resolve_takt_workflow};
use log::log_info;
use stages::{
    run_bookmark_check, run_diff, run_lint_screen, run_pr_size_check, run_push, run_quality_gate,
    run_scratch_file_warning, run_takt, DiffResult,
};

const EXIT_SUCCESS: i32 = 0;
const EXIT_QUALITY_GATE_FAILURE: i32 = 1;
const EXIT_TAKT_FAILURE: i32 = 2;
const EXIT_PUSH_FAILURE: i32 = 3;
const EXIT_CONFIG_ERROR: i32 = 4;
const EXIT_DIFF_FAILURE: i32 = 5;
const EXIT_SCRATCH_FILE_WARNING: i32 = 6;
const EXIT_BOOKMARK_MISSING: i32 = 7;
const EXIT_PR_SIZE_EXCEEDED: i32 = 8;

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

/// quality_gate より前の事前チェック (bookmark / scratch file / pr size) を実行する。
/// 成功時は検出した非 trunk bookmark 名 (push stage の `-b` 組み立て用) を返し、
/// 失敗時は exit code を Err で返して pipeline を中断する。
fn run_pre_checks(config: &config::Config) -> Result<Vec<String>, i32> {
    let Some(detected_bookmarks) = run_bookmark_check() else {
        log_info(
            "パイプライン中断: 非 trunk bookmark が見つかりません。\
             `jj bookmark create <name> -r @` で bookmark を作成して再実行してください。",
        );
        return Err(EXIT_BOOKMARK_MISSING);
    };
    if !run_scratch_file_warning(config.scratch_file_warning.as_ref()) {
        log_info(
            "パイプライン中断: scratch ファイル検出。`.gitignore` 修正 / ファイル削除 / \
             `SCRATCH_FILE_WARNING_OVERRIDE=1` のいずれかで再実行してください。",
        );
        return Err(EXIT_SCRATCH_FILE_WARNING);
    }
    if !run_pr_size_check(config.pr_size_check.as_ref()) {
        log_info(
            "パイプライン中断: PR diff サイズが block_threshold を超過。\
             PR 分割 / 閾値調整 / `PR_SIZE_CHECK_OVERRIDE=1` のいずれかで再実行してください。",
        );
        return Err(EXIT_PR_SIZE_EXCEEDED);
    }
    Ok(detected_bookmarks)
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

    let _pipeline_lock = lib_jj_helpers::pipeline_lock::hold_pipeline_lock("push", log_info);

    let has_diff = config.diff.is_some();
    let workflow = resolve_takt_workflow(&config);
    log_info(&format!(
        "パイプライン開始: bookmark → scratch → quality_gate → {} takt ({}) → push",
        if has_diff { "diff →" } else { "" },
        workflow,
    ));

    let detected_bookmarks = match run_pre_checks(&config) {
        Ok(bookmarks) => bookmarks,
        Err(code) => return code,
    };

    if !run_quality_gate(&config.quality_gate) {
        log_info("パイプライン中断: quality_gate 失敗。問題を修正して再実行してください。");
        return EXIT_QUALITY_GATE_FAILURE;
    }

    let skip_takt = match run_diff_and_lint_screen(&config) {
        Ok(skip) => skip,
        Err(code) => return code,
    };

    if !skip_takt && !run_takt(&config.takt, &workflow) {
        log_info("パイプライン中断: takt ワークフロー失敗。");
        return EXIT_TAKT_FAILURE;
    }

    if !run_push(&config.push, &detected_bookmarks) {
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
