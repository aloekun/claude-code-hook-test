//! Post-merge feedback workflow runner (ADR-030 Phase B).
//!
//! 旧 pending file 機構 (ADR-029) を置き換え、takt workflow `post-merge-feedback`
//! を同期実行する決定論的経路を提供する。
//!
//! 入力:
//!
//! - PipelineContext (pr_number, owner_repo)
//!
//! 副作用:
//!
//! - `.takt/post-merge-feedback-context.json` — workflow が読む PR メタデータ
//! - `.takt/post-merge-feedback-transcript.jsonl` — 時刻 range filter 済セッション履歴
//! - takt workflow を spawn (`pnpm exec takt -w post-merge-feedback ...`)
//! - 成功時: `.claude/feedback-reports/<pr>.md` を生成 (takt 出力をコピー)
//! - 失敗時: `.claude/feedback-reports/<pr>.md.failed` marker を残す (soft fail)
//!
//! 失敗時も exit code は変えない (merge は完了済み)。L2 recovery で後続ターンに
//! UserPromptSubmit hook が拾う想定。

mod context;
mod markers;
mod pr_metadata;
mod takt;
mod transcript;

pub use markers::write_failed_marker;
pub use pr_metadata::fetch_pr_diff_summary;
pub use transcript::project_transcript_dir;

use context::{find_latest_prepush_reports_dir, write_context_file};
use markers::{
    check_concurrent_run_guard, cleanup_failed_marker, write_pending_marker_logged,
    FailedMarkerGuard,
};
use pr_metadata::{fetch_pr_time_range, PrTimeRange};
use takt::{copy_feedback_report, run_takt_workflow, TAKT_TIMEOUT_SECS};
use transcript::filter_transcripts;

use std::fs;
use std::path::{Path, PathBuf};

/// 出力ファイルの相対パス (リポジトリルートからの相対)
pub const FEEDBACK_DIR: &str = ".claude/feedback-reports";
pub const CONTEXT_PATH: &str = ".takt/post-merge-feedback-context.json";
pub const TRANSCRIPT_PATH: &str = ".takt/post-merge-feedback-transcript.jsonl";

/// post-merge-feedback workflow の入力。
pub struct FeedbackInput<'a> {
    pub pr_number: u64,
    pub owner_repo: &'a str,
    /// リポジトリルート (`.takt/`, `.claude/` の親)。通常は `std::env::current_dir()`。
    pub repo_root: PathBuf,
    /// transcript ファイルが置かれるディレクトリ (`~/.claude/projects/<project-id>/`)。
    /// `None` なら transcript filter をスキップする (空 jsonl を出力)。
    pub transcript_source_dir: Option<PathBuf>,
}

/// 全工程を実行する高水準エントリポイント。
///
/// 失敗時は `Err(reason)` を返す。caller は `write_failed_marker` で marker を残す前提。
/// 成功時は `Ok(report_path)` (生成された feedback report の絶対パス)。
///
/// **Reconciliation 設計 (ADR-030 Phase B post-fix)**:
/// `run_takt_workflow` の戻り値に関わらず最後に `copy_feedback_report` を必ず試す。
/// 理由: Windows の `child.kill()` は takt の descendants を殺せないため、Rust が
/// timeout で kill 後も takt が orphan として走り続けて report を完成させるケースが
/// 観測された (PR #78 で kill 後 2 分 13 秒で feedback-report.md 完成)。
/// takt が exit=non-zero でも report が出ていれば成功扱いとする。
pub fn run(input: &FeedbackInput) -> Result<PathBuf, String> {
    let context_path = input.repo_root.join(CONTEXT_PATH);
    let transcript_path = input.repo_root.join(TRANSCRIPT_PATH);

    check_concurrent_run_guard(&context_path)?;

    write_pending_marker_logged(&input.repo_root, input.pr_number);
    let mut marker_guard = FailedMarkerGuard::new(&input.repo_root, input.pr_number);

    let range = fetch_pr_time_range(input.pr_number, input.owner_repo)
        .map_err(|e| format!("PR 時刻 range 取得失敗: {}", e))?;

    let written = prepare_transcript(
        input.transcript_source_dir.as_deref(),
        &range,
        &transcript_path,
    )?;
    eprintln!(
        "[merge-pipeline] [feedback] transcript filter 完了 ({} entries → {})",
        written,
        transcript_path.display()
    );

    let prepush_dir = find_latest_prepush_reports_dir(&input.repo_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    write_context_file(
        &context_path,
        input.pr_number,
        input.owner_repo,
        &range,
        TRANSCRIPT_PATH,
        &prepush_dir,
    )?;

    let takt_ok = run_takt_workflow(&input.repo_root, input.pr_number, TAKT_TIMEOUT_SECS);
    if !takt_ok {
        eprintln!(
            "[merge-pipeline] [feedback] takt が失敗/timeout を返しました — orphan が \
             report を完成させた可能性があるため reconciliation を試みます"
        );
    }

    let result = reconcile_takt_output(&input.repo_root, input.pr_number, takt_ok);
    if result.is_ok() {
        marker_guard.disarm();
    }
    result
}

/// transcript jsonl を filter (source dir 既知時) または空ファイル書込 (source dir 不在時) する。
///
/// 戻り値は書き込んだ行数 (空ファイル時は 0)。source dir 不在のケースは facet 側の
/// 「データなし」分岐に流すため、エラーではなく 0 行で成功扱いとする。
fn prepare_transcript(
    transcript_source_dir: Option<&Path>,
    range: &PrTimeRange,
    transcript_path: &Path,
) -> Result<usize, String> {
    match transcript_source_dir {
        Some(dir) => filter_transcripts(dir, range, transcript_path)
            .map_err(|e| format!("transcript filter 失敗: {}", e)),
        None => {
            if let Some(parent) = transcript_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(transcript_path, "");
            Ok(0)
        }
    }
}

/// takt 完了後の report コピーと reconciliation。
///
/// `takt_ok = false` でも orphan takt が report を完成させた可能性があるため必ず copy を
/// 試す。成功時に `.failed` marker を cleanup、失敗時は cause prefix 付きで Err を返す。
fn reconcile_takt_output(
    repo_root: &Path,
    pr_number: u64,
    takt_ok: bool,
) -> Result<PathBuf, String> {
    match copy_feedback_report(repo_root, pr_number) {
        Ok(report) => {
            if !takt_ok {
                eprintln!(
                    "[merge-pipeline] [feedback] reconciliation 成功: takt が \
                     timeout/失敗扱いだったが orphan が report を完成させていた"
                );
            }
            cleanup_failed_marker(repo_root, pr_number);
            Ok(report)
        }
        Err(copy_err) => {
            let cause = if takt_ok {
                "takt 成功扱いだが report 不在"
            } else {
                "takt 失敗/timeout かつ report 不在"
            };
            Err(format!("{}: {}", cause, copy_err))
        }
    }
}
