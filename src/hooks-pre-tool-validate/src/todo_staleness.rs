//! `docs/todo*.md` Edit/Write 時の staleness 検知 + 既実装 grep 提示。
//!
//! 順位 136 案 B: ADR-039 experimental pattern 準拠 (default-OFF in source、
//! repo config で明示 enable)。fail-closed (lineage 判定不能 = stale 扱いで安全側) per
//! entry 設計決定。

use crate::config::{
    TodoStalenessConfig, TODO_STALENESS_DEFAULT_BRANCH, TODO_STALENESS_DEFAULT_GREP_LIMIT,
    TODO_STALENESS_JJ_TIMEOUT_SECS,
};
use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_basic};
use regex::Regex;

pub(crate) struct TodoStalenessResult {
    pub(crate) message: String,
    pub(crate) stale: bool,
}

pub(crate) fn is_docs_todo_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    let re = match Regex::new(r"(^|/)docs/todo[\w-]*\.md$") {
        Ok(r) => r,
        Err(_) => return false,
    };
    re.is_match(&normalized)
}

pub(crate) fn extract_heading_keywords(text: &str) -> Vec<String> {
    let prefix_re = Regex::new(r"^順位\s*\d+\s*[:：]?\s*").ok();
    text.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .map(|heading| {
            let stripped = match &prefix_re {
                Some(re) => re.replace(heading.trim(), "").to_string(),
                None => heading.trim().to_string(),
            };
            stripped
                .split(['(', '（', '['])
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| s.len() >= 3)
        .collect()
}

fn run_jj_with_timeout(args: &[&str], timeout_secs: u64) -> Option<String> {
    use std::process::{Command, Stdio};

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

fn count_commits_branch_ahead(branch: &str) -> Option<usize> {
    let revset = format!("@-..{}", branch);
    let output = run_jj_with_timeout(
        &[
            "log",
            "-r",
            &revset,
            "--no-graph",
            "-T",
            "commit_id ++ \"\\n\"",
        ],
        TODO_STALENESS_JJ_TIMEOUT_SECS,
    )?;
    Some(output.lines().filter(|l| !l.trim().is_empty()).count())
}

pub(crate) fn parse_jj_log_records(raw: &str) -> Vec<(String, String)> {
    raw.split('\x1e')
        .filter_map(|record| {
            let mut parts = record.splitn(2, '\x1f');
            let commit_id = parts.next()?.trim().to_string();
            let description = parts.next()?.trim().to_string();
            if commit_id.is_empty() || description.is_empty() {
                None
            } else {
                Some((commit_id, description))
            }
        })
        .collect()
}

fn jj_log_recent_descriptions(limit: u64) -> Vec<(String, String)> {
    let limit_str = limit.to_string();
    let template = "commit_id.shortest(8) ++ \"\\x1f\" ++ description ++ \"\\x1e\"";
    match run_jj_with_timeout(
        &["log", "--limit", &limit_str, "--no-graph", "-T", template],
        TODO_STALENESS_JJ_TIMEOUT_SECS,
    ) {
        Some(raw) => parse_jj_log_records(&raw),
        None => Vec::new(),
    }
}

pub(crate) fn first_line(s: &str) -> &str {
    s.split('\n').next().unwrap_or("").trim()
}

pub(crate) fn find_matching_commits<'a>(
    keyword: &str,
    commits: &'a [(String, String)],
) -> Vec<&'a (String, String)> {
    let needle = keyword.to_lowercase();
    commits
        .iter()
        .filter(|(_, desc)| desc.to_lowercase().contains(&needle))
        .take(3)
        .collect()
}

pub(crate) fn build_todo_staleness_message(
    file_path: &str,
    behind: Option<usize>,
    keyword_matches: &[(String, Vec<(String, String)>)],
    branch: &str,
) -> Option<String> {
    let stale = behind.is_none_or(|n| n > 0);
    let any_matches = keyword_matches.iter().any(|(_, m)| !m.is_empty());
    if !stale && !any_matches {
        return None;
    }
    let mut lines = vec![format!("[docs/todo edit context] {}", file_path)];
    if let Some(b) = behind {
        if b > 0 {
            lines.push(format!(
                "stale parent detected: {} は @- より {} commits ahead",
                branch, b
            ));
            lines.push(format!(
                "修正手順: `jj git fetch && jj new {} -m \"WIP: <description>\"`",
                branch
            ));
        }
    } else {
        lines
            .push("stale parent detected: lineage 判定不能のため fail-closed で block".to_string());
    }
    for (keyword, matches) in keyword_matches {
        if matches.is_empty() {
            continue;
        }
        lines.push(format!("関連既実装の可能性 (keyword: \"{}\"):", keyword));
        for (commit_id, desc) in matches {
            lines.push(format!("  {} {}", commit_id, first_line(desc)));
        }
    }
    Some(lines.join("\n"))
}

pub(crate) fn check_todo_staleness(
    file_path: &str,
    text_for_keywords: &str,
    config: &TodoStalenessConfig,
) -> Option<TodoStalenessResult> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    if !is_docs_todo_path(file_path) {
        return None;
    }
    let branch = config
        .default_branch
        .as_deref()
        .unwrap_or(TODO_STALENESS_DEFAULT_BRANCH);
    let limit = config
        .grep_recent_limit
        .unwrap_or(TODO_STALENESS_DEFAULT_GREP_LIMIT);

    let behind = count_commits_branch_ahead(branch);
    let stale = behind.is_none_or(|n| n > 0);

    let keywords = extract_heading_keywords(text_for_keywords);
    let keyword_matches: Vec<(String, Vec<(String, String)>)> = if keywords.is_empty() {
        Vec::new()
    } else {
        let commits = jj_log_recent_descriptions(limit);
        keywords
            .iter()
            .take(3)
            .map(|kw| {
                let matches: Vec<(String, String)> = find_matching_commits(kw, &commits)
                    .into_iter()
                    .cloned()
                    .collect();
                (kw.clone(), matches)
            })
            .collect()
    };

    let message = build_todo_staleness_message(file_path, behind, &keyword_matches, branch)?;
    Some(TodoStalenessResult { message, stale })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TodoStalenessConfig;

    fn build_todo_path(suffix: &str) -> String {
        format!("docs/todo{}.md", suffix)
    }

    fn build_todo_path_with_prefix(prefix: &str, suffix: &str) -> String {
        format!("{}/docs/todo{}.md", prefix, suffix)
    }

    fn build_windows_todo_path(suffix: &str) -> String {
        format!("docs\\todo{}.md", suffix)
    }

    #[test]
    fn is_docs_todo_path_detects_repo_layout() {
        assert!(is_docs_todo_path(&build_todo_path("")));
        assert!(is_docs_todo_path(&build_todo_path("2")));
        assert!(is_docs_todo_path(&build_todo_path("-summary")));
        assert!(is_docs_todo_path(&build_todo_path_with_prefix(
            "e:/work/repo",
            "9"
        )));
    }

    #[test]
    fn is_docs_todo_path_handles_windows_separators() {
        assert!(is_docs_todo_path(&build_windows_todo_path("")));
        assert!(is_docs_todo_path(&format!(
            r"e:\work\repo\docs\todo{}.md",
            "8"
        )));
    }

    #[test]
    fn is_docs_todo_path_rejects_unrelated_paths() {
        assert!(!is_docs_todo_path("README.md"));
        assert!(!is_docs_todo_path("docs/adr/adr-041.md"));
        assert!(!is_docs_todo_path(&format!("notes/todo{}.md", "")));
        assert!(!is_docs_todo_path("src/main.rs"));
    }

    #[test]
    fn extract_heading_keywords_strips_rank_prefix() {
        let text = "### 順位 136 working copy staleness 検出 hook\n\n本文";
        let keywords = extract_heading_keywords(text);
        assert_eq!(keywords.len(), 1);
        assert!(
            keywords[0].contains("working copy staleness"),
            "got: {:?}",
            keywords
        );
        assert!(!keywords[0].contains("順位 136"));
    }

    #[test]
    fn extract_heading_keywords_handles_multiple_headings() {
        let text = "### 順位 1 first heading\n\n### 順位 2 second heading\n";
        let keywords = extract_heading_keywords(text);
        assert_eq!(keywords.len(), 2);
        assert!(keywords[0].contains("first heading"));
        assert!(keywords[1].contains("second heading"));
    }

    #[test]
    fn extract_heading_keywords_returns_empty_when_no_headings() {
        let text = "## sub heading\nplain text without ### prefix";
        assert!(extract_heading_keywords(text).is_empty());
    }

    #[test]
    fn extract_heading_keywords_filters_too_short() {
        let text = "### \n### ab\n### 順位 1 longer title";
        let keywords = extract_heading_keywords(text);
        assert_eq!(keywords.len(), 1);
        assert!(keywords[0].contains("longer title"));
    }

    #[test]
    fn parse_jj_log_records_basic() {
        let raw = "abc1234\x1ffirst commit description\x1edef5678\x1fsecond commit\x1e";
        let records = parse_jj_log_records(raw);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "abc1234");
        assert_eq!(records[0].1, "first commit description");
        assert_eq!(records[1].0, "def5678");
        assert_eq!(records[1].1, "second commit");
    }

    #[test]
    fn parse_jj_log_records_skips_malformed() {
        let raw = "abc\x1fdesc1\x1eonlyid_no_separator\x1exyz\x1fdesc2\x1e";
        let records = parse_jj_log_records(raw);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "abc");
        assert_eq!(records[1].0, "xyz");
    }

    #[test]
    fn find_matching_commits_case_insensitive() {
        let commits = vec![
            ("abc1".to_string(), "feat: ADD STALENESS hook".to_string()),
            ("abc2".to_string(), "unrelated change".to_string()),
            ("abc3".to_string(), "fix(staleness): tweak".to_string()),
        ];
        let matches = find_matching_commits("staleness", &commits);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn find_matching_commits_limits_to_three() {
        let commits: Vec<_> = (0..5)
            .map(|i| (format!("c{}", i), format!("feat: keyword #{}", i)))
            .collect();
        let matches = find_matching_commits("keyword", &commits);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn first_line_extracts_first_line() {
        assert_eq!(first_line("first\nsecond\nthird"), "first");
        assert_eq!(first_line("single"), "single");
        assert_eq!(first_line(""), "");
        assert_eq!(first_line("  spaced  \nrest"), "spaced");
    }

    #[test]
    fn build_todo_staleness_message_stale_with_matches() {
        let path = build_todo_path("");
        let matches = vec![(
            "test keyword".to_string(),
            vec![("abc1234".to_string(), "feat: implement test".to_string())],
        )];
        let msg = build_todo_staleness_message(&path, Some(3), &matches, "master");
        let msg = msg.expect("message should be generated");
        assert!(msg.contains(&path));
        assert!(msg.contains("3 commits ahead"));
        assert!(msg.contains("関連既実装の可能性"));
        assert!(msg.contains("test keyword"));
        assert!(msg.contains("abc1234"));
    }

    #[test]
    fn build_todo_staleness_message_stale_only() {
        let path = build_todo_path("");
        let msg = build_todo_staleness_message(&path, Some(2), &[], "main");
        let msg = msg.expect("stale should produce message");
        assert!(msg.contains("main"));
        assert!(msg.contains("2 commits ahead"));
        assert!(!msg.contains("関連既実装の可能性"));
    }

    #[test]
    fn build_todo_staleness_message_grep_only() {
        let path = build_todo_path("");
        let matches = vec![(
            "kw".to_string(),
            vec![("abc1234".to_string(), "feat: kw impl".to_string())],
        )];
        let msg = build_todo_staleness_message(&path, Some(0), &matches, "master");
        let msg = msg.expect("grep match alone should produce message");
        assert!(msg.contains("関連既実装の可能性"));
        assert!(!msg.contains("stale parent detected"));
    }

    #[test]
    fn build_todo_staleness_message_neither_returns_none() {
        let path = build_todo_path("");
        let msg = build_todo_staleness_message(&path, Some(0), &[], "master");
        assert!(msg.is_none());
    }

    #[test]
    fn build_todo_staleness_message_returns_some_when_behind_is_none() {
        let path = build_todo_path("");
        let msg = build_todo_staleness_message(&path, None, &[], "master");
        let msg = msg.expect("None behind should fail-closed and produce message");
        assert!(msg.contains(&path));
        assert!(msg.contains("判定不能"));
        assert!(msg.contains("fail-closed"));
        assert!(!msg.contains("commits ahead"));
    }

    #[test]
    fn build_todo_staleness_message_behind_none_with_matches_includes_both_sections() {
        let path = build_todo_path("");
        let matches = vec![(
            "kw".to_string(),
            vec![("abc1234".to_string(), "feat: kw impl".to_string())],
        )];
        let msg = build_todo_staleness_message(&path, None, &matches, "master");
        let msg = msg.expect("None behind always produces message regardless of matches");
        assert!(msg.contains("判定不能"));
        assert!(msg.contains("fail-closed"));
        assert!(msg.contains("関連既実装の可能性"));
        assert!(msg.contains("abc1234"));
    }

    #[test]
    fn check_todo_staleness_skip_when_disabled() {
        let config = TodoStalenessConfig {
            enabled: Some(false),
            default_branch: None,
            grep_recent_limit: None,
        };
        let result = check_todo_staleness(&build_todo_path(""), "### something", &config);
        assert!(result.is_none());
    }

    #[test]
    fn check_todo_staleness_skip_when_enabled_field_missing() {
        let config = TodoStalenessConfig {
            enabled: None,
            default_branch: None,
            grep_recent_limit: None,
        };
        let result = check_todo_staleness(&build_todo_path(""), "### something", &config);
        assert!(result.is_none(), "ADR-039 § 1 準拠で default-OFF");
    }

    #[test]
    fn check_todo_staleness_skip_when_not_todo_path() {
        let config = TodoStalenessConfig {
            enabled: Some(true),
            default_branch: None,
            grep_recent_limit: None,
        };
        let result = check_todo_staleness("docs/adr/adr-041.md", "### test", &config);
        assert!(result.is_none());
    }
}
