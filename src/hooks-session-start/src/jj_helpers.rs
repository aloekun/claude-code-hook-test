//! jj subprocess helpers — timeout 付き `jj` 呼び出し + revset commit count + FETCH_HEAD freshness。
//!
//! staleness / その他 jj 依存処理が共有する低レイヤ。failure mode は fail-open
//! (network 異常 / fetch timeout / parse 失敗等で session 起動を阻害しない)。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_basic};
use std::path::Path;
use std::process::Command;

const STALENESS_JJ_LOG_TIMEOUT_SECS: u64 = 5;

pub(crate) fn fetch_head_is_recent(repo_root: &Path, cache_secs: u64) -> bool {
    let fetch_head = repo_root.join(".git").join("FETCH_HEAD");
    let metadata = match std::fs::metadata(&fetch_head) {
        Ok(m) => m,
        Err(_) => return false,
    };
    match metadata.modified().and_then(|t| {
        t.elapsed()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }) {
        Ok(elapsed) => elapsed.as_secs() < cache_secs,
        Err(_) => false,
    }
}

pub(crate) fn run_jj_with_timeout(args: &[&str], timeout_secs: u64) -> Option<String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let Some(out) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let stdout_handle = drain_pipe_unlimited(out);
    let status = wait_with_timeout_basic("jj", &mut child, timeout_secs)
        .ok()
        .flatten();
    let output = stdout_handle.join().ok()?;
    status.filter(|s| s.success()).map(|_| output)
}

/// working copy が stale (別 workspace の操作で repo view から取り残された状態) かを検知する。
///
/// jj は stale 状態のとき通常コマンドで stderr に "The working copy is stale" を出して
/// 停止する (公式の設計。回復は `jj workspace update-stale`、ADR-045 § Known operational
/// risks)。軽量な `jj log -r @` を実行し stderr で判定する。
///
/// fail-open: spawn 失敗 / timeout / stderr 取得失敗は false (= nudge を出さない)。
/// 正常時の実行は既存の staleness 検査と同等の auto-snapshot 副作用のみ。
pub(crate) fn working_copy_is_stale(timeout_secs: u64) -> bool {
    use std::process::Stdio;

    let Ok(mut child) = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id.short()"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return false;
    };

    let Some(err_pipe) = child.stderr.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    };
    let stderr_handle = drain_pipe_unlimited(err_pipe);
    let _ = wait_with_timeout_basic("jj", &mut child, timeout_secs);
    let stderr = stderr_handle.join().unwrap_or_default();
    stderr_indicates_stale(&stderr)
}

/// stderr 出力が stale working copy を示すか (jj のエラー文言に基づく判定)。
pub(crate) fn stderr_indicates_stale(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("working copy is stale") || lower.contains("update-stale")
}

pub(crate) fn count_commits_in_revset(revset: &str) -> Option<usize> {
    let output = run_jj_with_timeout(
        &[
            "log",
            "-r",
            revset,
            "--no-graph",
            "-T",
            "commit_id ++ \"\\n\"",
        ],
        STALENESS_JJ_LOG_TIMEOUT_SECS,
    )?;
    Some(output.lines().filter(|l| !l.trim().is_empty()).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "jj-helpers-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn fetch_head_is_recent_returns_false_when_file_missing() {
        let root = unique_temp_root("fetch-head-missing");
        assert!(!fetch_head_is_recent(&root, 300));
    }

    #[test]
    fn stderr_indicates_stale_detects_jj_stale_error() {
        assert!(stderr_indicates_stale(
            "Error: The working copy is stale (not updated since operation 8b2bdf3bfd7b)"
        ));
        assert!(stderr_indicates_stale(
            "Hint: Run `jj workspace update-stale` to update it"
        ));
    }

    #[test]
    fn stderr_indicates_stale_ignores_normal_output() {
        assert!(!stderr_indicates_stale(""));
        assert!(!stderr_indicates_stale("Concurrent modification detected, resolving automatically."));
        assert!(!stderr_indicates_stale("Error: Revision `@` doesn't exist"));
    }

    #[test]
    fn fetch_head_is_recent_returns_true_for_fresh_file() {
        use std::io::Write;
        let root = unique_temp_root("fetch-head-fresh");
        let git_dir = root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let fetch_head = git_dir.join("FETCH_HEAD");
        let mut f = std::fs::File::create(&fetch_head).unwrap();
        writeln!(f, "fake content").unwrap();
        drop(f);
        assert!(fetch_head_is_recent(&root, 3600));
        let _ = std::fs::remove_dir_all(&root);
    }
}
