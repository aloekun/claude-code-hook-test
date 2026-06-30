//! PR メタデータ取得 (時刻 range / diff summary)。
//!
//! `gh pr view --json ...` の応答を `PrTimeRange` / `PrDiffSummary` に変換する。

use std::process::{Command, Stdio};

/// PR の時刻 range (gh api の出力から取得)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrTimeRange {
    pub first_commit_time: String,
    pub merged_at: String,
}

/// PR の diff summary (#A-2 の trivial PR skip 判定で使用)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDiffSummary {
    pub commit_count: usize,
    pub total_lines_changed: u64,
    pub all_files_are_markdown: bool,
}

/// 「trivial」と判定する +/- 合計の上限 (この値未満なら trivial)。
///
/// 動機 (#A-2): doc-only PR や 1-commit fix PR では post-merge-feedback の ROI が
/// 低いため skip する。50 行は doc 系 PR の典型サイズ + α として設定。
pub const TRIVIAL_PR_LINE_LIMIT: u64 = 50;

impl PrDiffSummary {
    /// docs/pipeline-token-efficiency.md PR 1 #A-2 の判定条件:
    /// 1) changed files が全て .md
    /// 2) かつ commit 数 = 1
    /// 3) かつ +/- 合計 < TRIVIAL_PR_LINE_LIMIT
    pub fn is_trivial(&self) -> bool {
        self.commit_count == 1
            && self.total_lines_changed < TRIVIAL_PR_LINE_LIMIT
            && self.all_files_are_markdown
    }
}

/// gh api から PR の `first_commit_time` (oldest commit authoredDate) と `mergedAt` を取得する。
///
/// 失敗時は `Err` を返す (caller 側で fallback)。
pub fn fetch_pr_time_range(pr_number: u64, owner_repo: &str) -> Result<PrTimeRange, String> {
    let pr_str = pr_number.to_string();
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_str,
            "--repo",
            owner_repo,
            "--json",
            "commits,mergedAt",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("gh コマンド起動失敗: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "gh pr view 失敗: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("gh 出力 JSON パース失敗: {}", e))?;

    parse_pr_time_range(&json)
}

/// `gh pr view --json commits,mergedAt` の応答を `PrTimeRange` に変換する。
fn parse_pr_time_range(json: &serde_json::Value) -> Result<PrTimeRange, String> {
    let merged_at = json
        .get("mergedAt")
        .and_then(|v| v.as_str())
        .ok_or("mergedAt が応答に含まれていません")?
        .to_string();

    let commits = json
        .get("commits")
        .and_then(|v| v.as_array())
        .ok_or("commits が応答に含まれていません")?;

    let first_commit_time = commits
        .iter()
        .filter_map(|c| {
            c.get("authoredDate")
                .or_else(|| c.get("committedDate"))
                .and_then(|v| v.as_str())
        })
        .min()
        .ok_or("commits 配列が空です")?
        .to_string();

    Ok(PrTimeRange {
        first_commit_time,
        merged_at,
    })
}

/// gh api から PR の diff summary (commit 数 / 行数 / 拡張子) を取得する (#A-2)。
///
/// 失敗時は `Err` を返す (caller は通常 flow に fallback)。
pub fn fetch_pr_diff_summary(pr_number: u64, owner_repo: &str) -> Result<PrDiffSummary, String> {
    let pr_str = pr_number.to_string();
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_str,
            "--repo",
            owner_repo,
            "--json",
            "files,commits,additions,deletions",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("gh コマンド起動失敗: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "gh pr view (diff summary) 失敗: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("gh 出力 JSON パース失敗: {}", e))?;

    parse_pr_diff_summary(&json)
}

/// `gh pr view --json files,commits,additions,deletions` の応答を `PrDiffSummary` に変換する。
fn parse_pr_diff_summary(json: &serde_json::Value) -> Result<PrDiffSummary, String> {
    let commits = json
        .get("commits")
        .and_then(|v| v.as_array())
        .ok_or("commits が応答に含まれていません")?;

    let additions = json
        .get("additions")
        .and_then(|v| v.as_u64())
        .ok_or("additions が応答に含まれていません")?;
    let deletions = json
        .get("deletions")
        .and_then(|v| v.as_u64())
        .ok_or("deletions が応答に含まれていません")?;

    let files = json
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or("files が応答に含まれていません")?;

    let all_md = !files.is_empty()
        && files.iter().all(|f| {
            f.get("path")
                .and_then(|v| v.as_str())
                .map(|p| p.to_lowercase().ends_with(".md"))
                .unwrap_or(false)
        });

    Ok(PrDiffSummary {
        commit_count: commits.len(),
        total_lines_changed: additions + deletions,
        all_files_are_markdown: all_md,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_summary_trivial_when_single_md_commit_under_limit() {
        let summary = PrDiffSummary {
            commit_count: 1,
            total_lines_changed: TRIVIAL_PR_LINE_LIMIT - 1,
            all_files_are_markdown: true,
        };
        assert!(summary.is_trivial());
    }

    #[test]
    fn diff_summary_not_trivial_when_at_line_limit_boundary() {
        let summary = PrDiffSummary {
            commit_count: 1,
            total_lines_changed: TRIVIAL_PR_LINE_LIMIT,
            all_files_are_markdown: true,
        };
        assert!(!summary.is_trivial());
    }

    #[test]
    fn diff_summary_not_trivial_when_multi_commit() {
        let summary = PrDiffSummary {
            commit_count: 2,
            total_lines_changed: 10,
            all_files_are_markdown: true,
        };
        assert!(!summary.is_trivial());
    }

    #[test]
    fn diff_summary_not_trivial_when_non_md_file_present() {
        let summary = PrDiffSummary {
            commit_count: 1,
            total_lines_changed: 5,
            all_files_are_markdown: false,
        };
        assert!(!summary.is_trivial());
    }

    #[test]
    fn parse_diff_summary_recognizes_trivial_doc_pr() {
        let json = serde_json::json!({
            "commits": [{ "oid": "abc" }],
            "additions": 10,
            "deletions": 5,
            "files": [
                { "path": "docs/sample.md", "additions": 10, "deletions": 5 }
            ],
        });
        let summary = parse_pr_diff_summary(&json).unwrap();
        assert_eq!(summary.commit_count, 1);
        assert_eq!(summary.total_lines_changed, 15);
        assert!(summary.all_files_are_markdown);
        assert!(summary.is_trivial());
    }

    #[test]
    fn parse_diff_summary_uppercase_md_extension_recognized() {
        let json = serde_json::json!({
            "commits": [{ "oid": "abc" }],
            "additions": 1,
            "deletions": 0,
            "files": [
                { "path": "README.MD", "additions": 1, "deletions": 0 }
            ],
        });
        let summary = parse_pr_diff_summary(&json).unwrap();
        assert!(summary.all_files_are_markdown);
    }

    #[test]
    fn parse_diff_summary_mixed_files_not_all_md() {
        let json = serde_json::json!({
            "commits": [{ "oid": "abc" }],
            "additions": 20,
            "deletions": 10,
            "files": [
                { "path": "docs/sample.md", "additions": 10, "deletions": 5 },
                { "path": "src/main.rs",  "additions": 10, "deletions": 5 }
            ],
        });
        let summary = parse_pr_diff_summary(&json).unwrap();
        assert!(!summary.all_files_are_markdown);
        assert!(!summary.is_trivial());
    }

    #[test]
    fn parse_diff_summary_empty_files_not_all_md() {
        let json = serde_json::json!({
            "commits": [{ "oid": "abc" }],
            "additions": 0,
            "deletions": 0,
            "files": [],
        });
        let summary = parse_pr_diff_summary(&json).unwrap();
        assert!(!summary.all_files_are_markdown);
        assert!(!summary.is_trivial());
    }

    #[test]
    fn parse_diff_summary_errors_on_missing_field() {
        let json = serde_json::json!({
            "commits": [{ "oid": "abc" }],
            "additions": 10,
            "files": [],
        });
        assert!(parse_pr_diff_summary(&json).is_err());
    }
}
