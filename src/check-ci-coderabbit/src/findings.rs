//! Inline review comments → Finding / ListedFinding 変換 (順位 209 / 順位 208 PR A refactor)。

use lib_report_formatter::Finding;

use crate::models::{GhPullComment, ListedFinding};

/// PR インラインレビューコメント (pulls/{pr}/comments) を [`Finding`] に変換する。
///
/// CodeRabbit のインラインコメントから severity, issue, suggestion を抽出する。
/// severity は本文先頭の `_⚠️ Potential issue_ | _🔴 Critical_` パターンから判定。
/// suggestion は `<details><summary>💡 修正イメージ</summary>` ブロックから抽出。
pub(crate) fn parse_findings(json: &str, push_time: &str) -> Vec<Finding> {
    let comments: Vec<GhPullComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!(
            "[check-ci-coderabbit] pull comments JSON パースエラー: {}",
            e
        );
        vec![]
    });

    comments
        .iter()
        .filter(|c| is_finding_candidate(c, push_time))
        .map(comment_to_finding)
        .collect()
}

fn is_finding_candidate(c: &GhPullComment, push_time: &str) -> bool {
    let is_coderabbit = c
        .user
        .as_ref()
        .and_then(|u| u.login.as_deref())
        .map(|l| l == "coderabbitai[bot]")
        .unwrap_or(false);
    let after_push_time = c
        .created_at
        .as_deref()
        .map(|t| t >= push_time)
        .unwrap_or(false);
    is_coderabbit && after_push_time
}

fn comment_to_finding(c: &GhPullComment) -> Finding {
    let body = c.body.as_deref().unwrap_or("");
    let severity = extract_severity(body);
    let issue = extract_issue(body);
    let suggestion = extract_suggestion(body);
    let file = c.path.clone().unwrap_or_default();
    let line = c
        .line
        .or(c.original_line)
        .map(|l| l.to_string())
        .unwrap_or_default();

    Finding {
        severity,
        file,
        line,
        issue,
        suggestion,
        source: "CodeRabbit".to_string(),
    }
}

/// PR インラインコメント JSON から [`ListedFinding`] を抽出する。
///
/// フィルタ条件:
///   - 投稿者が `coderabbitai[bot]` (review コメントのみ採用、reply は除外)
///   - `created_at >= push_time` (epoch 0 で実質全件)
///   - thread が `resolved:` reply で resolve されていない (outdated filter)
///   - `in_reply_to_id` が None (= thread root)
///
/// outdated filter は MVP として `resolved:` prefix の reply が同 thread に
/// 存在するかで判定する (memory `project_coderabbit_auto_resolve.md` 参照)。
pub(crate) fn parse_listed_findings(json: &str, push_time: &str) -> Vec<ListedFinding> {
    let comments: Vec<GhPullComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!(
            "[check-ci-coderabbit] pull comments JSON パースエラー: {}",
            e
        );
        vec![]
    });

    let resolved_root_ids = collect_resolved_root_ids(&comments);

    comments
        .iter()
        .filter(|c| is_listed_finding_candidate(c, push_time, &resolved_root_ids))
        .map(comment_to_listed_finding)
        .collect()
}

fn collect_resolved_root_ids(
    comments: &[GhPullComment],
) -> std::collections::HashSet<u64> {
    comments
        .iter()
        .filter_map(|c| {
            let parent = c.in_reply_to_id?;
            let body = c.body.as_deref()?;
            if is_resolve_reply(body) {
                Some(parent)
            } else {
                None
            }
        })
        .collect()
}

fn is_listed_finding_candidate(
    c: &GhPullComment,
    push_time: &str,
    resolved_root_ids: &std::collections::HashSet<u64>,
) -> bool {
    let is_coderabbit = c
        .user
        .as_ref()
        .and_then(|u| u.login.as_deref())
        .map(|l| l == "coderabbitai[bot]")
        .unwrap_or(false);
    let after_push_time = c
        .created_at
        .as_deref()
        .map(|t| t >= push_time)
        .unwrap_or(false);
    let is_thread_root = c.in_reply_to_id.is_none();
    let is_resolved =
        c.id.map(|id| resolved_root_ids.contains(&id))
            .unwrap_or(false);

    is_coderabbit && after_push_time && is_thread_root && !is_resolved
}

fn comment_to_listed_finding(c: &GhPullComment) -> ListedFinding {
    let body = c.body.as_deref().unwrap_or("");
    let severity = extract_severity(body);
    let summary = extract_summary(body);
    let file = c.path.clone().unwrap_or_default();
    let line = c.line.or(c.original_line).unwrap_or(0);
    let url = c.html_url.clone().unwrap_or_default();
    ListedFinding {
        severity,
        file,
        line,
        summary,
        url,
    }
}

/// reply 本文が `resolved:` (大文字小文字無視) で始まれば true。
pub(crate) fn is_resolve_reply(body: &str) -> bool {
    body.trim_start()
        .to_ascii_lowercase()
        .starts_with("resolved:")
}

/// `extract_issue` をベースに 1 行サマリ (最大 120 chars) に整形する。
pub(crate) fn extract_summary(body: &str) -> String {
    let issue = extract_issue(body);
    let single_line: String = issue.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_str(&single_line, 120)
}

/// CodeRabbit コメント本文から severity を抽出。
pub(crate) fn extract_severity(body: &str) -> String {
    let first_line = body.lines().next().unwrap_or("");
    if first_line.contains("Critical") || first_line.contains("🔴") {
        "Critical".to_string()
    } else if first_line.contains("Major") || first_line.contains("🟠") {
        "Major".to_string()
    } else if first_line.contains("Minor") || first_line.contains("🟡") {
        "Minor".to_string()
    } else if first_line.contains("High") {
        "High".to_string()
    } else if first_line.contains("Low") {
        "Low".to_string()
    } else {
        "Info".to_string()
    }
}

/// CodeRabbit コメント本文から指摘内容を抽出 (太字行優先、無ければ最初の意味のある行)。
pub(crate) fn extract_issue(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
            return trimmed[2..trimmed.len() - 2].to_string();
        }
    }
    for line in body.lines().skip(1) {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('_') && !trimmed.starts_with('<') {
            return truncate_str(trimmed, 100);
        }
    }
    "(詳細はコメント参照)".to_string()
}

/// CodeRabbit コメント本文から修正案を抽出。
pub(crate) fn extract_suggestion(body: &str) -> String {
    if let Some(start) = body.find("```suggestion") {
        let after = &body[start + 14..];
        if let Some(end) = after.find("```") {
            let suggestion = after[..end].trim();
            if !suggestion.is_empty() {
                return truncate_str(suggestion, 150);
            }
        }
    }
    if let Some(start) = body.find("```diff") {
        let after = &body[start + 7..];
        if let Some(end) = after.find("```") {
            let diff = after[..end].trim();
            if !diff.is_empty() {
                return truncate_str(diff, 150);
            }
        }
    }
    if body.contains("Prompt for AI Agents") {
        return "(修正指示あり — コメント参照)".to_string();
    }
    String::new()
}

/// UTF-8 安全な文字列切り詰め。
pub(crate) fn truncate_str(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ListFindingsOutput;

    #[test]
    fn is_resolve_reply_matches_lowercase_prefix() {
        assert!(is_resolve_reply("resolved: 修正完了"));
    }

    #[test]
    fn is_resolve_reply_matches_uppercase_prefix() {
        assert!(is_resolve_reply("Resolved: applied the fix"));
    }

    #[test]
    fn is_resolve_reply_matches_with_leading_whitespace() {
        assert!(is_resolve_reply("  resolved: 反映済み"));
    }

    #[test]
    fn is_resolve_reply_rejects_non_prefix_match() {
        assert!(!is_resolve_reply("Not resolved: still pending"));
    }

    #[test]
    fn is_resolve_reply_rejects_empty() {
        assert!(!is_resolve_reply(""));
    }

    #[test]
    fn extract_summary_collapses_whitespace_and_truncates() {
        let body = "**short summary**\n\nbody body body";
        assert_eq!(extract_summary(body), "short summary");
    }

    #[test]
    fn extract_summary_normalizes_extra_whitespace() {
        let body = "**summary   with    extra spaces**\n\nrest";
        assert_eq!(extract_summary(body), "summary with extra spaces");
    }

    #[test]
    fn parse_listed_findings_includes_thread_root_only() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "_⚠️ Potential issue_ | _🔴 Critical_\n\n**root finding A**", "path": "src/a.rs", "line": 10, "created_at": "2026-04-01T13:00:00Z", "html_url": "https://github.com/o/r/pull/1#discussion_r1"},
            {"id": 2, "user": {"login": "someuser"}, "body": "ack", "path": "src/a.rs", "line": 10, "created_at": "2026-04-01T13:05:00Z", "in_reply_to_id": 1, "html_url": "https://github.com/o/r/pull/1#discussion_r2"}
        ]"#;
        let findings = parse_listed_findings(json, "2026-04-01T12:00:00Z");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "src/a.rs");
        assert_eq!(findings[0].line, 10);
        assert_eq!(findings[0].severity, "Critical");
        assert_eq!(findings[0].summary, "root finding A");
        assert_eq!(
            findings[0].url,
            "https://github.com/o/r/pull/1#discussion_r1"
        );
    }

    #[test]
    fn parse_listed_findings_excludes_resolved_thread() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "_⚠️ Potential issue_ | _🟠 Major_\n\n**finding A (resolved)**", "path": "src/a.rs", "line": 10, "created_at": "2026-04-01T13:00:00Z", "html_url": "u1"},
            {"id": 2, "user": {"login": "coderabbitai[bot]"}, "body": "_⚠️ Potential issue_ | _🟡 Minor_\n\n**finding B (open)**", "path": "src/b.rs", "line": 20, "created_at": "2026-04-01T13:01:00Z", "html_url": "u2"},
            {"id": 3, "user": {"login": "human"}, "body": "resolved: 修正済み", "in_reply_to_id": 1, "created_at": "2026-04-01T13:10:00Z"}
        ]"#;
        let findings = parse_listed_findings(json, "2026-04-01T12:00:00Z");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "src/b.rs");
        assert_eq!(findings[0].severity, "Minor");
        assert_eq!(findings[0].summary, "finding B (open)");
    }

    #[test]
    fn parse_listed_findings_rejects_non_resolve_replies() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "_⚠️ Potential issue_ | _🔴 Critical_\n\n**still open**", "path": "src/a.rs", "line": 10, "created_at": "2026-04-01T13:00:00Z", "html_url": "u1"},
            {"id": 2, "user": {"login": "human"}, "body": "discussing this", "in_reply_to_id": 1, "created_at": "2026-04-01T13:10:00Z"}
        ]"#;
        let findings = parse_listed_findings(json, "2026-04-01T12:00:00Z");
        assert_eq!(findings.len(), 1, "non-resolve reply should not hide root");
        assert_eq!(findings[0].summary, "still open");
    }

    #[test]
    fn parse_listed_findings_filters_by_push_time() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "**old finding**", "path": "src/a.rs", "line": 10, "created_at": "2026-04-01T10:00:00Z", "html_url": "u1"},
            {"id": 2, "user": {"login": "coderabbitai[bot]"}, "body": "**new finding**", "path": "src/b.rs", "line": 20, "created_at": "2026-04-01T13:00:00Z", "html_url": "u2"}
        ]"#;
        let findings = parse_listed_findings(json, "2026-04-01T12:00:00Z");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "src/b.rs");
    }

    #[test]
    fn parse_listed_findings_epoch_push_time_includes_all() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "**ancient**", "path": "a.rs", "line": 1, "created_at": "2020-01-01T00:00:00Z", "html_url": "u"}
        ]"#;
        let findings = parse_listed_findings(json, "1970-01-01T00:00:00Z");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn parse_listed_findings_falls_back_to_original_line() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "**outdated finding**", "path": "src/a.rs", "original_line": 42, "created_at": "2026-04-01T13:00:00Z", "html_url": "u"}
        ]"#;
        let findings = parse_listed_findings(json, "2026-04-01T12:00:00Z");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 42);
    }

    #[test]
    fn parse_listed_findings_excludes_non_coderabbit_authors() {
        let json = r#"[
            {"id": 1, "user": {"login": "human"}, "body": "**not from CR**", "path": "a.rs", "line": 1, "created_at": "2026-04-01T13:00:00Z", "html_url": "u"}
        ]"#;
        assert!(parse_listed_findings(json, "2026-04-01T12:00:00Z").is_empty());
    }

    #[test]
    fn parse_listed_findings_empty_json_returns_empty() {
        assert!(parse_listed_findings("[]", "2026-04-01T12:00:00Z").is_empty());
    }

    #[test]
    fn parse_listed_findings_invalid_json_returns_empty() {
        assert!(parse_listed_findings("not json", "2026-04-01T12:00:00Z").is_empty());
    }

    #[test]
    fn parse_listed_findings_serializes_as_expected_schema() {
        let json = r#"[
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": "_⚠️ Potential issue_ | _🟠 Major_\n\n**signature mismatch**", "path": "src/lib.rs", "line": 100, "created_at": "2026-04-01T13:00:00Z", "html_url": "https://github.com/o/r/pull/1#r1"}
        ]"#;
        let findings = parse_listed_findings(json, "2026-04-01T12:00:00Z");
        let output = ListFindingsOutput { findings };
        let serialized = serde_json::to_value(&output).unwrap();
        let item = &serialized["findings"][0];
        assert_eq!(item["severity"], "Major");
        assert_eq!(item["file"], "src/lib.rs");
        assert_eq!(item["line"], 100);
        assert_eq!(item["summary"], "signature mismatch");
        assert_eq!(item["url"], "https://github.com/o/r/pull/1#r1");
    }
}
