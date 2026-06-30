//! takt workflow の同期実行と report コピー。
//!
//! `pnpm exec takt -w post-merge-feedback ...` を spawn し、完了後に最新 run dir の
//! `feedback-report.md` を `.claude/feedback-reports/<pr>.md` にコピーする。

use crate::feedback::context::{find_latest_run_dir, TAKT_TASK_PREFIX, TAKT_WORKFLOW};
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

/// takt 完了後、最新 run dir の `feedback-report.md` を `.claude/feedback-reports/<pr>.md` にコピーする。
pub fn copy_feedback_report(repo_root: &Path, pr_number: u64) -> Result<PathBuf, String> {
    let runs_dir = repo_root.join(".takt").join("runs");
    let latest = find_latest_run_dir(&runs_dir, TAKT_WORKFLOW)
        .ok_or("post-merge-feedback の run dir が見つかりません")?;

    let source = latest.join("reports").join("feedback-report.md");
    if !source.is_file() {
        return Err(format!(
            "feedback-report.md が見つかりません: {}",
            source.display()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orphan_threshold_exceeds_takt_timeout() {
        assert!(
            ORPHAN_THRESHOLD_SECS > TAKT_TIMEOUT_SECS,
            "orphan threshold ({}s) must exceed TAKT_TIMEOUT_SECS ({}s) to avoid \
             false-positive reaping of legitimately-running takt workflows",
            ORPHAN_THRESHOLD_SECS,
            TAKT_TIMEOUT_SECS,
        );
        assert_eq!(
            ORPHAN_THRESHOLD_SECS,
            TAKT_TIMEOUT_SECS + 300,
            "ORPHAN_THRESHOLD_SECS must track TAKT_TIMEOUT_SECS + 300s margin \
             (ADR-030 §L2 reaper threshold)"
        );
    }
}
