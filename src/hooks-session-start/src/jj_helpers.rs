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
