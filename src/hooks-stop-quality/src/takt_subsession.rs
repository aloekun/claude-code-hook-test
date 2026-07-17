//! active takt subsession の検知 (ADR-004 § takt subsession skip)。
//!
//! takt subsession は `edit: false` で起動される read-only 分析セッションが多く
//! (例: weekly-review whole-tree reviewer / post-merge-feedback analyzer)、Stop hook が
//! 品質ゲート失敗の「直せ」指示を返すと制約に反して stray edit を試みる事故が発生する
//! (PR #221 で実観測)。本 module で active subsession を検知し、呼び出し側が品質ゲートを
//! skip することで、ADR-004 の趣旨 (= 本対話セッションの品質担保) と takt の
//! `edit: false` 制約の整合を取る。
//!
//! 判定の起点は repo root であり cwd ではない。cwd 基準にすると cwd drift で
//! active run を黙って見落とす (T7 の incident。`main` の `normalize_cwd_to_project_root`
//! 参照)。

use serde::Deserialize;
use std::path::Path;

/// `.takt/runs/` の相対パス (repo root から)。hooks-session-start の reaper module と同値。
const TAKT_RUNS_DIR: &str = ".takt/runs";

/// freshness threshold (秒)。meta.json の mtime がこの値以内なら active 扱い。
///
/// hooks-session-start の reaper module の `ORPHAN_THRESHOLD_SECS` (= 1500s
/// = takt timeout 1200s + 余裕 5 分) と同値。本 threshold を超えた `status: "running"` は
/// abrupt termination で残った orphan run とみなし、active subsession 判定から除外する。
/// この上限により、orphan run が永久に残って品質ゲートを skip し続ける問題を防ぐ
/// (ADR-004 § takt subsession skip 参照)。
const ACTIVE_RUN_FRESH_THRESHOLD_SECS: u64 = 1500;

/// takt meta.json の必要 field のみ部分デシリアライズ (status 判定のみ)。
#[derive(Deserialize)]
struct TaktMetaPartial {
    status: Option<String>,
}

/// `<repo_root>/.takt/runs/<slug>/meta.json` を scan して active takt run があるか判定する。
///
/// 条件: いずれかの meta.json が `status: "running"` **かつ** mtime が
/// `ACTIVE_RUN_FRESH_THRESHOLD_SECS` 以内であれば true (= subsession active)。
/// 1 件以上見つかった時点で短絡 return する。malformed JSON / non-dir / read error は skip。
/// freshness check で「abrupt termination で残った orphan run が永続的に品質ゲートを
/// skip させる」問題を防ぐ (CR PR #222 Major 指摘の根本対策)。
pub fn takt_subsession_active(repo_root: &Path) -> bool {
    let runs_dir = repo_root.join(TAKT_RUNS_DIR);
    let entries = match std::fs::read_dir(&runs_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if meta_is_active_run(&path.join("meta.json")) {
            return true;
        }
    }
    false
}

/// 単一の `meta.json` が active run (= status: "running" AND fresh) か判定する。
///
/// freshness は meta.json の filesystem mtime で判定。
/// `ACTIVE_RUN_FRESH_THRESHOLD_SECS` 以内なら fresh、超えていれば orphan とみなして
/// active 扱いしない (CR PR #222 Major 指摘対応)。
fn meta_is_active_run(meta_path: &Path) -> bool {
    if !meta_status_is_running(meta_path) {
        return false;
    }
    meta_is_fresh(meta_path)
}

/// 単一の `meta.json` の status が `"running"` か判定する (test 用に切り出し)。
fn meta_status_is_running(meta_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(meta_path) else {
        return false;
    };
    let Ok(meta) = serde_json::from_str::<TaktMetaPartial>(&content) else {
        return false;
    };
    meta.status.as_deref() == Some("running")
}

/// 単一の `meta.json` の mtime が `ACTIVE_RUN_FRESH_THRESHOLD_SECS` 以内か判定する。
///
/// fail-closed: mtime 取得失敗 / 未来時刻 (= clock skew) は false (= active 扱いしない)。
/// これにより orphan run / 異常な timestamp で品質ゲートが skip され続ける事故を防ぐ。
fn meta_is_fresh(meta_path: &Path) -> bool {
    let metadata = match std::fs::metadata(meta_path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let mtime = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return false,
    };
    match mtime.elapsed() {
        Ok(elapsed) => elapsed.as_secs() < ACTIVE_RUN_FRESH_THRESHOLD_SECS,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static UNIQUE_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let n = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("stop_quality_{}_{}_{}", prefix, pid, n));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_run_meta(root: &Path, slug: &str, status: &str) {
        let run_dir = root.join(".takt/runs").join(slug);
        std::fs::create_dir_all(&run_dir).unwrap();
        let json = serde_json::json!({ "status": status });
        std::fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn takt_subsession_active_returns_false_when_runs_dir_missing() {
        let root = unique_temp_root("no-runs-dir");
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_false_when_no_meta_json_files() {
        let root = unique_temp_root("empty-runs-dir");
        std::fs::create_dir_all(root.join(".takt/runs/orphan-slug")).unwrap();
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_false_when_all_status_completed() {
        let root = unique_temp_root("all-completed");
        write_run_meta(&root, "run-a", "completed");
        write_run_meta(&root, "run-b", "failed");
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_true_when_any_status_running() {
        let root = unique_temp_root("one-running");
        write_run_meta(&root, "completed-run", "completed");
        write_run_meta(&root, "active-run", "running");
        write_run_meta(&root, "failed-run", "failed");
        assert!(takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_true_for_single_running_run() {
        let root = unique_temp_root("single-running");
        write_run_meta(&root, "active", "running");
        assert!(takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_skips_malformed_meta_json() {
        let root = unique_temp_root("malformed");
        let run_dir = root.join(".takt/runs/malformed-run");
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::write(run_dir.join("meta.json"), "not-valid-json{").unwrap();
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn meta_status_is_running_returns_true_for_running_status() {
        let root = unique_temp_root("status-running");
        write_run_meta(&root, "test", "running");
        let meta_path = root.join(".takt/runs/test/meta.json");
        assert!(meta_status_is_running(&meta_path));
    }

    #[test]
    fn meta_status_is_running_returns_false_for_other_statuses() {
        let root = unique_temp_root("status-other");
        for status in &["completed", "failed", "cancelled", "pending"] {
            write_run_meta(&root, status, status);
            let meta_path = root.join(format!(".takt/runs/{}/meta.json", status));
            assert!(
                !meta_status_is_running(&meta_path),
                "status {:?} must not be detected as running",
                status
            );
        }
    }

    #[test]
    fn meta_status_is_running_returns_false_when_file_missing() {
        let root = unique_temp_root("missing");
        let meta_path = root.join(".takt/runs/never-existed/meta.json");
        assert!(!meta_status_is_running(&meta_path));
    }

    fn set_meta_mtime_to_past(meta_path: &Path, secs_ago: u64) {
        use std::time::{Duration, SystemTime};
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(meta_path)
            .expect("open meta.json for mtime set");
        let past = SystemTime::now() - Duration::from_secs(secs_ago);
        f.set_modified(past).expect("set_modified");
    }

    #[test]
    fn meta_is_fresh_returns_true_for_just_written_file() {
        let root = unique_temp_root("fresh-just-written");
        write_run_meta(&root, "now", "running");
        let meta_path = root.join(".takt/runs/now/meta.json");
        assert!(meta_is_fresh(&meta_path));
    }

    #[test]
    fn meta_is_fresh_returns_false_for_stale_mtime_above_threshold() {
        let root = unique_temp_root("fresh-stale");
        write_run_meta(&root, "old", "running");
        let meta_path = root.join(".takt/runs/old/meta.json");
        set_meta_mtime_to_past(&meta_path, ACTIVE_RUN_FRESH_THRESHOLD_SECS + 60);
        assert!(!meta_is_fresh(&meta_path));
    }

    #[test]
    fn meta_is_fresh_returns_true_just_below_threshold_boundary() {
        let root = unique_temp_root("fresh-just-below");
        write_run_meta(&root, "boundary", "running");
        let meta_path = root.join(".takt/runs/boundary/meta.json");
        set_meta_mtime_to_past(&meta_path, ACTIVE_RUN_FRESH_THRESHOLD_SECS - 10);
        assert!(meta_is_fresh(&meta_path));
    }

    #[test]
    fn meta_is_fresh_returns_false_when_file_missing() {
        let root = unique_temp_root("fresh-missing");
        let meta_path = root.join(".takt/runs/never-existed/meta.json");
        assert!(!meta_is_fresh(&meta_path));
    }

    #[test]
    fn takt_subsession_active_returns_false_for_stale_orphan_running_run() {
        let root = unique_temp_root("orphan-stale");
        write_run_meta(&root, "orphan", "running");
        let meta_path = root.join(".takt/runs/orphan/meta.json");
        set_meta_mtime_to_past(&meta_path, ACTIVE_RUN_FRESH_THRESHOLD_SECS + 3600);
        assert!(
            !takt_subsession_active(&root),
            "stale orphan run (status: running but mtime > threshold) must not block quality gate (CR PR #222 Major 指摘対応)"
        );
    }

    #[test]
    fn takt_subsession_active_distinguishes_fresh_running_from_stale_running() {
        let root = unique_temp_root("orphan-mixed");
        write_run_meta(&root, "stale-orphan", "running");
        set_meta_mtime_to_past(
            &root.join(".takt/runs/stale-orphan/meta.json"),
            ACTIVE_RUN_FRESH_THRESHOLD_SECS + 60,
        );
        write_run_meta(&root, "fresh-active", "running");
        assert!(
            takt_subsession_active(&root),
            "fresh running run must override stale orphan (= 過剰 skip ではなく適切な active 判定)"
        );
    }

    #[test]
    fn meta_is_active_run_requires_both_running_and_fresh() {
        let root = unique_temp_root("active-conditions");
        write_run_meta(&root, "completed-fresh", "completed");
        let completed = root.join(".takt/runs/completed-fresh/meta.json");
        assert!(!meta_is_active_run(&completed), "fresh but not running");

        write_run_meta(&root, "running-stale", "running");
        let stale = root.join(".takt/runs/running-stale/meta.json");
        set_meta_mtime_to_past(&stale, ACTIVE_RUN_FRESH_THRESHOLD_SECS + 30);
        assert!(!meta_is_active_run(&stale), "running but stale");

        write_run_meta(&root, "running-fresh", "running");
        let active = root.join(".takt/runs/running-fresh/meta.json");
        assert!(meta_is_active_run(&active), "running AND fresh = active");
    }
}
