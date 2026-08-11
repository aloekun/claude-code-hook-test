//! takt workflow の同期実行と report コピー。
//!
//! `pnpm exec takt -w post-merge-feedback ...` を spawn し、完了後に **対象 PR の** run dir の
//! `feedback-report.md` を `.claude/feedback-reports/<pr>.md` にコピーする。run dir の特定は
//! [`crate::feedback::run_registry`] が `meta.json` の task label で PR を照合して行う。

use crate::feedback::context::{TAKT_TASK_PREFIX, TAKT_WORKFLOW};
use crate::feedback::run_registry;
use crate::feedback::FEEDBACK_DIR;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// takt 実行のデフォルトタイムアウト (20 分)
///
/// 観測実績 (PR #77: 14m21s、PR #78: 12m13s) の parallel 構成想定値 (~7m30s) に
/// 対し 2x の安全係数を取った暫定値。analyze-session の所要時間は transcript 量で
/// スケールするため、長期 PR では再評価が必要 (ADR-030 §レイテンシ 参照)。
pub const TAKT_TIMEOUT_SECS: u64 = 1200;

/// orphan run reaper (ADR-030 §L2) の閾値秒数。`TAKT_TIMEOUT_SECS` + 余裕 5 分。
///
/// 正常 run は `TAKT_TIMEOUT_SECS` (1200s) 以内に completed / failed のいずれかに
/// 遷移するため、本値 (1500s) を超えても `status: "running"` のまま放置されている
/// run は abrupt 終了 (kill -9 / SIGKILL / power loss / OOM Killer) で in-process Drop
/// guard を経由せず死んだとみなす。`TAKT_TIMEOUT_SECS` 変更時に本値も自動追随する。
///
/// 本 const は canonical 参照値として保持し、out-of-process reaper 実装の
/// `hooks-session-start::ORPHAN_THRESHOLD_SECS` は同 literal `1500` を pin する
/// (両 crate の test で drift 検出)。
#[allow(dead_code)]
pub const ORPHAN_THRESHOLD_SECS: u64 = TAKT_TIMEOUT_SECS + 300;

const _: () = assert!(
    ORPHAN_THRESHOLD_SECS > TAKT_TIMEOUT_SECS,
    "orphan threshold must exceed TAKT_TIMEOUT_SECS to avoid false-positive reaping of legitimately-running takt workflows"
);
const _: () = assert!(
    ORPHAN_THRESHOLD_SECS == TAKT_TIMEOUT_SECS + 300,
    "ORPHAN_THRESHOLD_SECS must track TAKT_TIMEOUT_SECS + 300s margin (ADR-030 §L2 reaper threshold)"
);

/// run_takt_workflow のポーリング間隔 (ms)
const POLL_INTERVAL_MS: u64 = 500;

/// takt workflow を spawn し、終了まで待つ。
///
/// stdio は inherit (push-runner / pr-monitor と同じパターン)。
/// timeout 経過時は kill して false を返す。
pub fn run_takt_workflow(repo_root: &Path, pr_number: u64, timeout_secs: u64) -> bool {
    let task_label = format!("{}{}", TAKT_TASK_PREFIX, pr_number);
    let mut child = match Command::new("pnpm")
        .args(["exec", "takt", "-w", TAKT_WORKFLOW, "-t", &task_label])
        .current_dir(repo_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let exited_success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status.success()),
            Ok(None) if std::time::Instant::now() >= deadline => break None,
            Err(_) => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS)),
        }
    };

    match exited_success {
        Some(success) => success,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            false
        }
    }
}

/// report 出現待ちの試行回数と間隔 (順位 388)。
///
/// takt が exit した直後は report の書き込みが OS のバッファに残っていることがあり、
/// #367 では「takt 成功扱いだが report 不在」の marker が出たあとに run dir を見ると
/// 実体が**存在していた**。長く待つ意味は無い (完了済み run が後から report を生やすことは
/// 無い) ので、flush 待ちに足りる最小限だけ再試行する。
const REPORT_WAIT_ATTEMPTS: u32 = 5;
const REPORT_WAIT_INTERVAL_MS: u64 = 200;

/// takt 完了後、**対象 PR の** run dir の `feedback-report.md` を
/// `.claude/feedback-reports/<pr>.md` にコピーする。
///
/// run dir は lex-latest ではなく `meta.json` の task label で **PR 番号を照合して**選ぶ
/// (順位 398 / 388)。連続マージ中は自分より新しい別 PR の run dir が末尾に来るため、
/// lex-latest では**別 PR のレポートを自分の `<pr>.md` へコピー**しうる。
pub fn copy_feedback_report(repo_root: &Path, pr_number: u64) -> Result<PathBuf, String> {
    let runs_dir = run_registry::runs_dir(repo_root);
    let run = run_registry::latest_run_for_pr(&runs_dir, pr_number).ok_or_else(|| {
        format!(
            "PR #{} の post-merge-feedback run dir が見つかりません ({} を走査)",
            pr_number,
            runs_dir.display()
        )
    })?;

    let source = run.report_path(repo_root);
    if !wait_for_report(&source) {
        return Err(format!(
            "feedback-report.md が見つかりません: {} (run: {})",
            source.display(),
            run_registry::describe(&run)
        ));
    }

    let target_dir = repo_root.join(FEEDBACK_DIR);
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("feedback dir 作成失敗 {}: {}", target_dir.display(), e))?;
    let target = target_dir.join(format!("{}.md", pr_number));
    fs::copy(&source, &target).map_err(|e| {
        format!(
            "コピー失敗 {} → {}: {}",
            source.display(),
            target.display(),
            e
        )
    })?;
    Ok(target)
}

/// report が現れるまで短く待つ。現れれば `true`。
fn wait_for_report(source: &Path) -> bool {
    for attempt in 0..REPORT_WAIT_ATTEMPTS {
        if source.is_file() {
            return true;
        }
        if attempt + 1 < REPORT_WAIT_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(REPORT_WAIT_INTERVAL_MS));
        }
    }
    source.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `.takt/runs/<slug>/` に meta.json と (任意で) report を置く。
    fn write_run(repo_root: &Path, slug: &str, pr: u64, status: &str, report: Option<&str>) {
        let dir = repo_root.join(".takt").join("runs").join(slug);
        let reports = dir.join("reports");
        fs::create_dir_all(&reports).unwrap();
        fs::write(
            dir.join("meta.json"),
            format!(
                r#"{{"task":"post-merge-feedback for #{pr}","status":"{status}",
                    "reportDirectory":".takt/runs/{slug}/reports"}}"#
            ),
        )
        .unwrap();
        if let Some(body) = report {
            fs::write(reports.join("feedback-report.md"), body).unwrap();
        }
    }

    /// **順位 398 / 388 の核心**: 連続マージ中に別 PR の新しい run があっても、
    /// 対象 PR の run の report をコピーする。
    #[test]
    fn copies_the_report_of_the_requested_pr_not_the_newest_run() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_run(
            root,
            "20260810-100000-post-merge-feedback-for-382",
            382,
            "completed",
            Some("382 の内容"),
        );
        write_run(
            root,
            "20260810-200000-post-merge-feedback-for-383",
            383,
            "completed",
            Some("383 の内容"),
        );

        let target = copy_feedback_report(root, 382).expect("382 の report をコピーできること");
        assert_eq!(target, root.join(FEEDBACK_DIR).join("382.md"));
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "382 の内容",
            "より新しい 383 の report を 382.md へコピーしてはいけない"
        );
    }

    #[test]
    fn uses_the_latest_run_when_the_same_pr_was_retried() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_run(
            root,
            "20260810-100000-post-merge-feedback-for-7",
            7,
            "failed",
            Some("1 回目"),
        );
        write_run(
            root,
            "20260810-200000-post-merge-feedback-for-7",
            7,
            "completed",
            Some("2 回目"),
        );

        let target = copy_feedback_report(root, 7).expect("copy");
        assert_eq!(fs::read_to_string(&target).unwrap(), "2 回目");
    }

    #[test]
    fn reports_which_run_was_inspected_when_the_report_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_run(
            root,
            "20260810-100000-post-merge-feedback-for-5",
            5,
            "completed",
            None,
        );

        let message = copy_feedback_report(root, 5).expect_err("report 不在は Err");
        assert!(message.contains("feedback-report.md"), "{message}");
        assert!(
            message.contains("#5"),
            "どの run を見たかを示すこと: {message}"
        );
    }

    /// 対象 PR の run が無い場合は「別 PR の run を代用」せず Err にする。
    #[test]
    fn a_run_of_another_pr_is_not_used_as_a_substitute() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_run(
            root,
            "20260810-100000-post-merge-feedback-for-1",
            1,
            "completed",
            Some("別 PR"),
        );

        let message = copy_feedback_report(root, 2).expect_err("2 の run は無いので Err");
        assert!(message.contains("#2"), "{message}");
        assert!(
            !root.join(FEEDBACK_DIR).join("2.md").exists(),
            "誤ったレポートを生成してはいけない"
        );
    }

    #[test]
    fn missing_runs_dir_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(copy_feedback_report(tmp.path(), 1).is_err());
    }

    /// report が遅れて現れるケース (順位 388 の write race) で再試行が効くこと。
    #[test]
    fn waits_briefly_for_a_report_that_appears_late() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        write_run(
            &root,
            "20260810-100000-post-merge-feedback-for-3",
            3,
            "completed",
            None,
        );

        let late = root
            .join(".takt/runs/20260810-100000-post-merge-feedback-for-3/reports")
            .join("feedback-report.md");
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(REPORT_WAIT_INTERVAL_MS * 2));
            fs::write(&late, "遅れて出た report").unwrap();
        });

        let target = copy_feedback_report(&root, 3).expect("再試行で拾えること");
        writer.join().unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "遅れて出た report");
    }
}
