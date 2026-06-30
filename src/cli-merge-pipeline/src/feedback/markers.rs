//! `.failed` marker / pending marker / Drop guard / 並行起動 guard。
//!
//! ADR-030 §L1 in-process recovery の中核。pre-emptive marker + RAII Drop guard で
//! abrupt 終了 (SIGPIPE / kill -9 / panic / 早期 return) でも marker 存在を保証する。

use crate::feedback::context::{TAKT_TASK_PREFIX, TAKT_WORKFLOW};
use crate::feedback::FEEDBACK_DIR;
use std::fs;
use std::path::{Path, PathBuf};

/// 並行起動 guard の TTL (秒)。`TAKT_TIMEOUT_SECS` (1200s) より少し長い値。
///
/// 直前の cli-merge-pipeline 起動で `context.json` が書かれてから本値の経過時間内に
/// 次の起動が来た場合、orphan takt が生きている可能性が高いとみなして refuse する
/// (Bug 3: cross-invocation context overwrite race の予防)。
pub const CONCURRENT_RUN_GUARD_SECS: u64 = 1500;

/// `.failed` marker を書き出す (L2 recovery が拾う前提)。
pub fn write_failed_marker(
    repo_root: &Path,
    pr_number: u64,
    reason: &str,
) -> Result<PathBuf, String> {
    let dir = repo_root.join(FEEDBACK_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("feedback dir 作成失敗 {}: {}", dir.display(), e))?;
    let path = dir.join(format!("{}.md.failed", pr_number));
    let body = format!(
        "# post-merge-feedback failed (PR #{})\n\n\
         takt workflow `{}` の同期実行が失敗しました。\n\n\
         ## 失敗理由\n\n{}\n\n\
         ## 復旧手順\n\n\
         1. このマーカー (`{}`) を残したまま、Claude Code セッションで何か入力する\n\
         2. UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) が検出し、Claude に再実行を促す\n\
         3. Claude セッションから手動で再実行する場合は、リポジトリルートで\n   \
            `pnpm exec takt -w {} -t \"{}{}\"` を直接起動してください\n   \
            注意: この再実行は `.takt/post-merge-feedback-context.json` を読み直すだけなので、\n   \
            失敗から再実行までの間に **別 PR が `pnpm merge-pr` を実行している** と context が\n   \
            上書きされ、誤った PR の transcript range が使われます。再実行前に\n   \
            `.takt/post-merge-feedback-context.json` の `pr_number` が #{} と一致することを必ず確認してください。\n",
        pr_number,
        TAKT_WORKFLOW,
        reason,
        path.display(),
        TAKT_WORKFLOW,
        TAKT_TASK_PREFIX,
        pr_number,
        pr_number,
    );
    fs::write(&path, body).map_err(|e| format!("failed marker 書込失敗: {}", e))?;
    Ok(path)
}

/// 成功時に `.failed` marker が残っていたら削除する。
pub(crate) fn cleanup_failed_marker(repo_root: &Path, pr_number: u64) {
    let path = failed_marker_path(repo_root, pr_number);
    let _ = fs::remove_file(path);
}

/// `.claude/feedback-reports/<pr>.md.failed` の絶対パスを返す (純粋関数)。
pub fn failed_marker_path(repo_root: &Path, pr_number: u64) -> PathBuf {
    repo_root
        .join(FEEDBACK_DIR)
        .join(format!("{}.md.failed", pr_number))
}

/// pre-emptive `.failed` marker (Drop guard 用)。
///
/// ADR-030 §L1 in-process recovery の中核。`feedback::run` の早期段階で marker を
/// 書き出し、正常完了時のみ `cleanup_failed_marker` で削除する。SIGPIPE / kill -9 等で
/// process が abrupt 終了しても marker がディスクに残るため L2 recovery (UserPromptSubmit
/// hook) が拾える。
///
/// 詳細 reason (例: takt timeout、report 不在) は caller (main.rs) が Err 経路で
/// `write_failed_marker` を再呼び出しして上書きするため、本関数は最小限の
/// "pending" 状態のみ書き出す。
fn write_pending_marker(repo_root: &Path, pr_number: u64) -> Result<PathBuf, String> {
    write_failed_marker(
        repo_root,
        pr_number,
        "pending: takt workflow が完了する前に process が終了した可能性あり \
         (pre-emptive marker, ADR-030 §L1)",
    )
}

/// `write_pending_marker` を呼び出し、失敗時はステージログに記録する。
///
/// silent drop ではなく log 残存 (Bundle l 順位 129、lint_screen.rs の
/// `write_skip_report_logged` と同パターン)。fail-fast せずに継続するのは、
/// pending marker は best-effort observability の補助で、本体 workflow の
/// 進行を block すべきではないため。
pub(crate) fn write_pending_marker_logged(repo_root: &Path, pr_number: u64) {
    if let Err(e) = write_pending_marker(repo_root, pr_number) {
        eprintln!(
            "[merge-pipeline] [feedback] pending marker 書き込み失敗 (PR #{}, 続行): {}",
            pr_number, e
        );
    }
}

/// RAII guard: scope 終了時に `.failed` marker が残っていることを保証する。
///
/// ADR-030 §L1 in-process Drop guard。`feedback::run` 内で armed 状態の guard を
/// 作成し、正常完了時のみ `disarm()` で抑止する。abnormal 経路 (panic / 早期 return)
/// では Drop が marker 存在を check し、欠落していれば backup として書き直す
/// (idempotent: 既存 marker (例: caller の detailed marker) は overwrite しない)。
///
/// **Rust default SIGPIPE の制約**: `SIG_DFL` で process が abrupt 終了するため
/// Drop は呼ばれない。SIGPIPE 経路は `write_pending_marker` の **pre-emptive 書込み**
/// で marker をディスクに先置きすることで救済する。本 guard は panic / 早期 return
/// のような Drop が走る経路の backup として機能する。
pub(crate) struct FailedMarkerGuard<'a> {
    repo_root: &'a Path,
    pr_number: u64,
    armed: bool,
}

impl<'a> FailedMarkerGuard<'a> {
    pub(crate) fn new(repo_root: &'a Path, pr_number: u64) -> Self {
        Self {
            repo_root,
            pr_number,
            armed: true,
        }
    }

    /// 正常完了時に guard を解除する。Drop は no-op になる。
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FailedMarkerGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if failed_marker_path(self.repo_root, self.pr_number).exists() {
            return;
        }
        if let Err(e) = write_failed_marker(
            self.repo_root,
            self.pr_number,
            "pending: workflow が unexpected に終了した \
             (FailedMarkerGuard Drop, ADR-030 §L1)",
        ) {
            eprintln!(
                "[merge-pipeline] [feedback] FailedMarkerGuard Drop で marker 書き込み失敗 \
                 (PR #{}): {}",
                self.pr_number, e
            );
        }
    }
}

/// 既存の context file の経過時刻を返す。存在しない/読めない場合は `None`。
fn context_age_secs(context_path: &Path) -> Option<u64> {
    let modified = fs::metadata(context_path).ok()?.modified().ok()?;
    modified.elapsed().ok().map(|d| d.as_secs())
}

/// 並行 cli-merge-pipeline 起動を検出した場合 `Err` を返す。
///
/// 「直前の cli-merge-pipeline 起動の context.json が依然として新しい」状態は
/// orphan takt が走り続けている可能性を示すため、context.json を上書きしない。
/// `cleanup_failed_marker` 等の場合は影響なし (これは marker 系ファイル)。
pub(crate) fn check_concurrent_run_guard(context_path: &Path) -> Result<(), String> {
    let Some(age) = context_age_secs(context_path) else {
        return Ok(());
    };
    if age >= CONCURRENT_RUN_GUARD_SECS {
        return Ok(());
    }
    Err(format!(
        "前回の post-merge-feedback workflow がまだ進行中の可能性 \
         (context.json が {}s 前に書かれた)。{}s 待つか、進行中の takt が無いことを\
         確認してから手動で {} を削除してください。",
        age,
        CONCURRENT_RUN_GUARD_SECS,
        context_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    fn unique_tmp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    #[test]
    fn write_failed_marker_creates_file() {
        let root = std::env::temp_dir().join(format!(
            "feedback-marker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&root).unwrap();
        let path = write_failed_marker(&root, 7, "takt timeout (10 minutes)").unwrap();
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("PR #7"));
        assert!(body.contains("takt timeout (10 minutes)"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_failed_marker_removes_existing() {
        let root = std::env::temp_dir().join(format!(
            "feedback-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(root.join(FEEDBACK_DIR)).unwrap();
        let marker = root.join(FEEDBACK_DIR).join("5.md.failed");
        fs::write(&marker, "old failure").unwrap();
        assert!(marker.exists());
        cleanup_failed_marker(&root, 5);
        assert!(!marker.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_marker_path_uses_feedback_dir_layout() {
        let path = failed_marker_path(Path::new("/repo"), 42);
        let suffix = format!("{}{}42.md.failed", FEEDBACK_DIR, std::path::MAIN_SEPARATOR);
        assert!(path.to_string_lossy().ends_with(&suffix));
    }

    #[test]
    fn write_pending_marker_creates_marker_with_pending_reason() {
        let root = unique_temp_root("feedback-pending");
        fs::create_dir_all(&root).unwrap();
        let path = write_pending_marker(&root, 9).unwrap();
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("PR #9"));
        assert!(body.contains("pending"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_disarmed_does_not_create_marker() {
        let root = unique_temp_root("feedback-guard-disarmed");
        fs::create_dir_all(&root).unwrap();
        {
            let mut guard = FailedMarkerGuard::new(&root, 11);
            guard.disarm();
        }
        assert!(!failed_marker_path(&root, 11).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_armed_writes_backup_when_missing() {
        let root = unique_temp_root("feedback-guard-armed");
        fs::create_dir_all(&root).unwrap();
        {
            let _guard = FailedMarkerGuard::new(&root, 12);
        }
        let path = failed_marker_path(&root, 12);
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("FailedMarkerGuard Drop"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_armed_preserves_existing_detailed_marker() {
        let root = unique_temp_root("feedback-guard-preserve");
        fs::create_dir_all(&root).unwrap();
        let existing = write_failed_marker(&root, 13, "takt timeout 1200s").unwrap();
        let original_body = fs::read_to_string(&existing).unwrap();
        {
            let _guard = FailedMarkerGuard::new(&root, 13);
        }
        let after_body = fs::read_to_string(&existing).unwrap();
        assert_eq!(
            original_body, after_body,
            "Drop guard must not overwrite an existing detailed marker (idempotent backup)"
        );
        assert!(after_body.contains("takt timeout 1200s"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_guard_disarmed_preserves_existing_marker() {
        let root = unique_temp_root("feedback-guard-disarm-preserve");
        fs::create_dir_all(&root).unwrap();
        write_failed_marker(&root, 14, "leftover").unwrap();
        let path = failed_marker_path(&root, 14);
        {
            let mut guard = FailedMarkerGuard::new(&root, 14);
            guard.disarm();
        }
        assert!(
            path.exists(),
            "disarm must not delete an existing marker (only the Ok-path cleanup_failed_marker does)"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_run_guard_passes_when_context_absent() {
        let path = unique_tmp_path("feedback-guard-absent");
        assert!(check_concurrent_run_guard(&path).is_ok());
    }

    #[test]
    fn concurrent_run_guard_blocks_when_context_recent() {
        let path = unique_tmp_path("feedback-guard-recent");
        fs::write(&path, "{}").unwrap();
        let result = check_concurrent_run_guard(&path);
        assert!(result.is_err(), "newly-written context should block");
        let msg = result.unwrap_err();
        assert!(msg.contains("進行中"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn concurrent_run_guard_passes_when_context_stale() {
        let path = unique_tmp_path("feedback-guard-stale-substitute");
        fs::write(&path, "{}").unwrap();
        let age = context_age_secs(&path);
        assert!(age.is_some());
        assert!(age.unwrap() < CONCURRENT_RUN_GUARD_SECS);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn context_age_secs_returns_none_for_missing() {
        let path = unique_tmp_path("feedback-age-missing");
        assert!(context_age_secs(&path).is_none());
    }

    #[test]
    fn context_age_secs_returns_some_for_existing() {
        let path = unique_tmp_path("feedback-age-exists");
        fs::write(&path, "{}").unwrap();
        let age = context_age_secs(&path);
        assert!(age.is_some());
        assert!(age.unwrap() < 5);
        let _ = fs::remove_file(&path);
    }
}
