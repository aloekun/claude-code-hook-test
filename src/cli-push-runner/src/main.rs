//! Push Runner — takt ベースの pre-push パイプライン
//!
//! pnpm push から呼び出され、以下のステージを実行する:
//!   Stage -1:  bookmark_check — 非 trunk bookmark の存在を確認 (順位 2)
//!   Stage 0:   scratch_file_warning — `__*` 等の scratch ファイル混入を検査 (順位 1)
//!   Stage 0.5: docs_only_routing — PR 範囲が docs-only なら Rust の gate group を skip (T11)
//!   Stage 1:   quality_gate — TOML で定義されたコマンド群をグループ間で並列実行
//!   Stage 1.5: diff         — jj diff を取得しファイルに書き出し（reviewers が Read で参照）
//!   Stage 2:   takt         — AI レビュー（reviewers → fix loop）
//!   Stage 2.5: post_takt_regate — takt fix が作業コピーを変えたら quality_gate を再実行 (T12)
//!   Stage 3:   push         — jj git push
//!
//! push 成功後は pnpm スクリプトチェーンにより cli-pr-monitor が起動される。
//!
//! 終了コード:
//!   0 - 全ステージ成功
//!   1 - quality_gate 失敗 (takt 後の post_takt_regate による再実行失敗を含む)
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
use log::{log_info, timed};
use stages::{
    run_bookmark_check, run_diff, run_docs_only_routing, run_lint_screen, run_post_takt_regate,
    run_pr_size_check, run_push, run_quality_gate, run_scratch_file_warning, run_takt, DiffResult,
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

/// diff stage の結果。takt / post-takt re-gate を走らせるかを表す。
enum DiffGate {
    /// diff が空 → takt も re-gate も不要 (push へ直行)
    SkipTakt,
    /// takt を実行する。`pre_diff` は post-takt re-gate の変化検出用 snapshot
    /// (`[diff]` 未設定 / 読込失敗時は None → re-gate は fail-closed で再実行)。
    RunTakt { pre_diff: Option<String> },
}

/// diff stage を実行し lint-screen を呼び出す。
/// Ok(DiffGate) で成功、 Err(exit_code) で pipeline 中断。
///
/// takt 実行前の diff snapshot を **takt 起動前に**メモリへ確保する (T12): fix が
/// `[diff] output_path` を上書きするため、比較用の pre 状態は takt 前に読み取る必要がある。
fn run_diff_and_lint_screen(config: &config::Config) -> Result<DiffGate, i32> {
    let Some(diff_config) = &config.diff else {
        return Ok(DiffGate::RunTakt { pre_diff: None });
    };
    let diff_path = match run_diff(diff_config) {
        DiffResult::HasContent => diff_config.output_path.as_str(),
        DiffResult::Empty => {
            log_info("diff が空のためレビューをスキップして push に進みます。");
            return Ok(DiffGate::SkipTakt);
        }
        DiffResult::Error => {
            log_info("パイプライン中断: diff 取得失敗。");
            return Err(EXIT_DIFF_FAILURE);
        }
    };
    if let Some(lint_screen_config) = &config.lint_screen {
        run_lint_screen(lint_screen_config, diff_path);
    }
    let pre_diff = std::fs::read_to_string(diff_path).ok();
    Ok(DiffGate::RunTakt { pre_diff })
}

/// quality_gate より前の事前チェック (bookmark / scratch file / pr size) を実行する。
/// 成功時は検出した非 trunk bookmark 名 (push stage の `-b` 組み立て用) を返し、
/// 失敗時は exit code を Err で返して pipeline を中断する。
fn run_pre_checks(config: &config::Config) -> Result<Vec<String>, i32> {
    let Some(detected_bookmarks) = run_bookmark_check() else {
        log_info("パイプライン中断: push 可能な bookmark がありません (対処は直前のログを参照)。");
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

/// takt workflow 名を解決し、パイプライン開始ログを出力する。workflow 名を返す。
/// (config の読込は呼び出し側で済んでいる前提。)
fn start_pipeline(config: &config::Config) -> String {
    let has_diff = config.diff.is_some();
    let workflow = resolve_takt_workflow(config);
    log_info(&format!(
        "パイプライン開始: bookmark → docs_only_routing → quality_gate → {} takt ({}) → push",
        if has_diff { "diff →" } else { "" },
        workflow,
    ));
    workflow
}

/// takt (AI レビュー → fix loop) と post-takt re-gate (T12) を実行する。
/// diff が空 (`DiffGate::SkipTakt`) の場合は両方 skip する。
/// Ok(()) で続行、Err(exit_code) で pipeline 中断。
fn run_takt_and_regate(
    config: &config::Config,
    workflow: &str,
    diff_gate: &DiffGate,
) -> Result<(), i32> {
    let DiffGate::RunTakt { pre_diff } = diff_gate else {
        return Ok(());
    };
    if !timed("takt", || run_takt(&config.takt, workflow)) {
        log_info("パイプライン中断: takt ワークフロー失敗。");
        return Err(EXIT_TAKT_FAILURE);
    }
    if !timed("post_takt_regate", || {
        run_post_takt_regate(config, pre_diff.as_deref())
    }) {
        log_info(
            "パイプライン中断: post-takt re-gate 失敗。takt fix がテスト / lint を壊した \
             可能性があります。問題を修正して再実行してください。",
        );
        return Err(EXIT_QUALITY_GATE_FAILURE);
    }
    Ok(())
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

    let workflow = start_pipeline(&config);

    let detected_bookmarks = match timed("pre_checks", || run_pre_checks(&config)) {
        Ok(bookmarks) => bookmarks,
        Err(code) => return code,
    };

    let skip_groups = timed("docs_only_routing", || {
        run_docs_only_routing(config.docs_only_routing.as_ref())
    });

    if !timed("quality_gate", || {
        run_quality_gate(&config.quality_gate, &skip_groups)
    }) {
        log_info("パイプライン中断: quality_gate 失敗。問題を修正して再実行してください。");
        return EXIT_QUALITY_GATE_FAILURE;
    }

    let diff_gate = match timed("diff", || run_diff_and_lint_screen(&config)) {
        Ok(gate) => gate,
        Err(code) => return code,
    };

    if let Err(code) = run_takt_and_regate(&config, &workflow, &diff_gate) {
        return code;
    }

    if !timed("push", || run_push(&config.push, &detected_bookmarks)) {
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
