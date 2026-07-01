//! workflow context file の生成と takt run dir 解決。
//!
//! takt workflow が Read で読む PR メタデータ JSON を書き出し、pre-push-review の
//! 最新 reports ディレクトリを探索する。

use crate::feedback::pr_metadata::PrTimeRange;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// takt workflow 名 / task ラベル
///
/// 命名規約 (ADR-030 §task labeling convention): task label は workflow 名を必ず prefix
/// として含む `"<workflow-name> [<context>]"` 形式とする。これにより takt の sanitization 後の
/// dir 名 (`<timestamp>-<sanitized-task-label>`) が必ず workflow 名を含み、`find_latest_run_dir`
/// が `name.contains("-<workflow>")` でマッチできる。
pub const TAKT_WORKFLOW: &str = "post-merge-feedback";

/// post-merge-feedback の task label prefix。`hooks-session-start` の orphan reaper
/// (ADR-030 §L2 out-of-process) も meta.json `task` field を本値で discriminate する。
/// 値を変更する場合は両 crate を同 PR で更新する (Drift 検出用 test は
/// `hooks-session-start` 側で literal を assertion している)。
pub const TAKT_TASK_PREFIX: &str = "post-merge-feedback for #";

/// takt workflow に渡す JSON コンテキスト。
#[derive(Serialize)]
struct WorkflowContext<'a> {
    pr_number: u64,
    owner_repo: &'a str,
    merged_at: &'a str,
    first_commit_time: &'a str,
    transcript_path: &'a str,
    prepush_reports_dir: &'a str,
}

/// `.takt/runs/` 配下で `workflow` 名を含む最新ディレクトリ (lex-sort 末尾) を返す。
///
/// takt の run dir は `<timestamp>-<sanitized-task-label>` 形式。task label が
/// ADR-030 §task labeling convention に従い workflow 名を prefix として含む場合、
/// dir 名にも `-<workflow>` という連続部分文字列が必ず現れる:
///   - task = `"<workflow>"`             → dir = `<ts>-<workflow>`
///   - task = `"<workflow> for #<pr>"`   → dir = `<ts>-<workflow>-for-<pr>`
///
/// どちらの形にも `name.contains(&format!("-{}", workflow))` で一律にマッチする。
/// `ends_with` を避けることで、context suffix 付きの形にも対応する。
///
/// 制約: workflow 名同士が部分文字列関係になってはいけない (例: `merge` と
/// `post-merge-feedback` は OK、`post-merge` と `post-merge-feedback` は NG)。
pub(crate) fn find_latest_run_dir(runs_dir: &Path, workflow: &str) -> Option<PathBuf> {
    let needle = format!("-{}", workflow);
    let mut candidates: Vec<PathBuf> = fs::read_dir(runs_dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if name.contains(&needle) {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next_back()
}

/// pre-push-review workflow の最新 reports ディレクトリを返す。
pub fn find_latest_prepush_reports_dir(repo_root: &Path) -> Option<PathBuf> {
    let runs_dir = repo_root.join(".takt").join("runs");
    let latest = find_latest_run_dir(&runs_dir, "pre-push-review")?;
    let reports = latest.join("reports");
    if reports.is_dir() {
        Some(reports)
    } else {
        None
    }
}

/// context file (workflow が Read で読む) を書き出す。
pub fn write_context_file(
    out_path: &Path,
    pr_number: u64,
    owner_repo: &str,
    range: &PrTimeRange,
    transcript_relpath: &str,
    prepush_reports_dir: &str,
) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("context dir 作成失敗 {}: {}", parent.display(), e))?;
    }
    let ctx = WorkflowContext {
        pr_number,
        owner_repo,
        merged_at: &range.merged_at,
        first_commit_time: &range.first_commit_time,
        transcript_path: transcript_relpath,
        prepush_reports_dir,
    };
    let json = serde_json::to_string_pretty(&ctx)
        .map_err(|e| format!("context JSON serialize 失敗: {}", e))?;
    fs::write(out_path, json).map_err(|e| format!("context 書込失敗: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_context_file_serializes_fields() {
        let dir = std::env::temp_dir().join(format!(
            "feedback-ctx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("context.json");
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        write_context_file(
            &out,
            42,
            "owner/repo",
            &range,
            ".takt/transcript.jsonl",
            ".takt/runs/foo/reports",
        )
        .unwrap();

        let raw = fs::read_to_string(&out).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.get("pr_number").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(
            parsed.get("owner_repo").and_then(|v| v.as_str()),
            Some("owner/repo")
        );
        assert_eq!(
            parsed.get("merged_at").and_then(|v| v.as_str()),
            Some("2026-04-25T10:00:00.000Z")
        );
        assert_eq!(
            parsed.get("transcript_path").and_then(|v| v.as_str()),
            Some(".takt/transcript.jsonl")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_latest_prepush_picks_lexicographic_max() {
        let root = std::env::temp_dir().join(format!(
            "feedback-prepush-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        let runs = root.join(".takt").join("runs");
        fs::create_dir_all(runs.join("20260425-000000-pre-push-review").join("reports")).unwrap();
        fs::create_dir_all(runs.join("20260425-094925-pre-push-review").join("reports")).unwrap();
        fs::create_dir_all(runs.join("20260425-100000-other-workflow").join("reports")).unwrap();

        let latest = find_latest_prepush_reports_dir(&root).unwrap();
        assert!(latest
            .to_string_lossy()
            .contains("20260425-094925-pre-push-review"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_latest_run_dir_matches_workflow_name_only() {
        let root = std::env::temp_dir().join(format!(
            "feedback-find-name-only-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        let runs = root.join(".takt").join("runs");
        fs::create_dir_all(runs.join("20260425-100000-post-merge-feedback")).unwrap();
        fs::create_dir_all(runs.join("20260425-110000-other-workflow")).unwrap();

        let latest = find_latest_run_dir(&runs, "post-merge-feedback").unwrap();
        assert!(latest
            .to_string_lossy()
            .contains("20260425-100000-post-merge-feedback"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_latest_run_dir_matches_workflow_with_context_suffix() {
        let root = std::env::temp_dir().join(format!(
            "feedback-find-with-ctx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        let runs = root.join(".takt").join("runs");
        fs::create_dir_all(runs.join("20260425-100000-post-merge-feedback-for-77")).unwrap();
        fs::create_dir_all(runs.join("20260425-090000-pre-push-review")).unwrap();

        let latest = find_latest_run_dir(&runs, "post-merge-feedback").unwrap();
        assert!(latest
            .to_string_lossy()
            .contains("20260425-100000-post-merge-feedback-for-77"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_latest_run_dir_picks_lex_max_across_mixed_forms() {
        let root = std::env::temp_dir().join(format!(
            "feedback-find-mixed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        let runs = root.join(".takt").join("runs");
        fs::create_dir_all(runs.join("20260425-090000-post-merge-feedback")).unwrap();
        fs::create_dir_all(runs.join("20260425-100000-post-merge-feedback-for-77")).unwrap();
        fs::create_dir_all(runs.join("20260425-080000-post-merge-feedback-for-50")).unwrap();

        let latest = find_latest_run_dir(&runs, "post-merge-feedback").unwrap();
        assert!(latest
            .to_string_lossy()
            .contains("20260425-100000-post-merge-feedback-for-77"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_latest_run_dir_returns_none_when_no_match() {
        let root = std::env::temp_dir().join(format!(
            "feedback-find-none-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        let runs = root.join(".takt").join("runs");
        fs::create_dir_all(runs.join("20260425-090000-pre-push-review")).unwrap();
        fs::create_dir_all(runs.join("20260425-100000-analyze-pr-review-comments")).unwrap();

        assert!(find_latest_run_dir(&runs, "post-merge-feedback").is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_latest_run_dir_returns_none_when_dir_missing() {
        let nonexistent = std::env::temp_dir().join(format!(
            "feedback-nonexistent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        assert!(find_latest_run_dir(&nonexistent, "post-merge-feedback").is_none());
    }
}
