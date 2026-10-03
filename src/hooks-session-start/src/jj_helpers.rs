//! jj subprocess helpers — timeout 付き `jj` 呼び出し + revset commit count + fetch の鮮度。
//!
//! staleness / その他 jj 依存処理が共有する低レイヤ。failure mode は fail-open
//! (network 異常 / fetch timeout / parse 失敗等で session 起動を阻害しない)。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_basic};
use std::path::Path;
use std::process::Command;

const STALENESS_JJ_LOG_TIMEOUT_SECS: u64 = 5;

/// 最後に fetch が成功した時刻 (UNIX 秒) を**内容として**持つ記録 (repo root からの相対パス)。
///
/// 以前は `.git/FETCH_HEAD` の mtime で判定していたが、**`jj git fetch` は FETCH_HEAD を
/// 書かない** (`jj git clone` も作らない。2026-10-02 に実測、順位 493)。そのためキャッシュは
/// 一度も効かず、セッション開始のたびに fetch していた。mtime は他の操作でも動くので、
/// 時刻は内容で持つ。
const LAST_FETCH_RECORD: &str = ".claude/session-start-last-fetch";

/// 最後の fetch 成功から `cache_secs` 秒以内か。記録が無い / 読めない / 壊れている場合は
/// `false` (= fetch する。staleness の判定が古いデータに基づかない側へ倒す)。
pub(crate) fn fetch_is_recent(repo_root: &Path, cache_secs: u64) -> bool {
    let Ok(content) = std::fs::read_to_string(repo_root.join(LAST_FETCH_RECORD)) else {
        return false;
    };
    recorded_fetch_is_recent(&content, now_unix_secs(), cache_secs)
}

/// 記録の内容 (UNIX 秒) が `now` から見て `cache_secs` 秒以内か。未来の時刻は `false`
/// (時計の巻き戻りや壊れた記録で、fetch が恒久的に止まらないようにする)。
pub(crate) fn recorded_fetch_is_recent(content: &str, now: u64, cache_secs: u64) -> bool {
    let Ok(fetched_at) = content.trim().parse::<u64>() else {
        return false;
    };
    fetched_at <= now && now - fetched_at < cache_secs
}

/// fetch の成功を記録する。書き込みに失敗しても、次のセッションで fetch し直すだけなので
/// 結果は捨てる (fail-open。セッション起動を阻害しない)。
pub(crate) fn record_successful_fetch(repo_root: &Path) {
    let path = repo_root.join(LAST_FETCH_RECORD);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&path, now_unix_secs().to_string()).ok();
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

    const CACHE_SECS: u64 = 300;
    const NOW: u64 = 1_790_000_000;

    #[test]
    fn fetch_is_not_recent_without_a_record() {
        let root = tempfile::tempdir().unwrap();
        assert!(!fetch_is_recent(root.path(), CACHE_SECS));
    }

    #[test]
    fn a_recorded_fetch_is_recent_until_the_cache_expires() {
        assert!(recorded_fetch_is_recent(&NOW.to_string(), NOW, CACHE_SECS));
        assert!(recorded_fetch_is_recent(&(NOW - (CACHE_SECS - 1)).to_string(), NOW, CACHE_SECS));
        assert!(!recorded_fetch_is_recent(&(NOW - CACHE_SECS).to_string(), NOW, CACHE_SECS));
    }

    /// 未来の時刻・壊れた内容は fetch する側 (`false`) に倒す。
    #[test]
    fn future_or_malformed_records_are_not_recent() {
        assert!(!recorded_fetch_is_recent(&(NOW + 1).to_string(), NOW, CACHE_SECS));
        assert!(!recorded_fetch_is_recent("", NOW, CACHE_SECS));
        assert!(!recorded_fetch_is_recent("yesterday", NOW, CACHE_SECS));
    }

    #[test]
    fn recording_a_fetch_makes_it_recent() {
        let root = tempfile::tempdir().unwrap();
        record_successful_fetch(root.path());
        assert!(fetch_is_recent(root.path(), CACHE_SECS));
    }

    /// **mtime ではなく内容で判定する** (順位 493)。jj は FETCH_HEAD を書かず、mtime は
    /// 他の操作でも動く。古い記録を新しい mtime で、新しい記録を古い mtime で書いても、
    /// 判定は内容どおりになる。
    #[test]
    fn the_judgement_ignores_the_file_mtime() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(LAST_FETCH_RECORD);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        std::fs::write(&path, (now_unix_secs() - 10 * CACHE_SECS).to_string()).unwrap();
        assert!(!fetch_is_recent(root.path(), CACHE_SECS), "古い記録は mtime が新しくても期限切れ");

        record_successful_fetch(root.path());
        let long_ago = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        assert!(fetch_is_recent(root.path(), CACHE_SECS), "新しい記録は mtime が古くても有効");
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
}
