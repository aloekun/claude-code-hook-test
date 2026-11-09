//! Inline review comments → Finding / ListedFinding 変換 (順位 209 / 順位 208 PR A refactor)。

use lib_report_formatter::Finding;

use crate::models::{GhPullComment, ListedFinding};

/// PR インラインレビューコメント (pulls/{pr}/comments) を [`Finding`] に変換する。
///
/// CodeRabbit のインラインコメントから severity, issue, suggestion を抽出する。
/// severity は重大度の行 (タグを除いて最初に空でない行) から判定する ([`extract_severity`])。
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

/// 本文の各行から HTML のタグ (`<picture>` / `<!-- … -->` など) を除き、前後の空白を落とす。
///
/// CodeRabbit は 2026-11 から、本文の 1 行目に `<!-- coderabbit-ai-generated:v1 -->` を置き、
/// 重大度の行にアイコンの `<picture>` を埋め込む (#625 / #626 で実測)。タグを残したまま
/// 読むと、重大度の行が見つからず、見出しの代わりにタグを拾う。
fn plain_lines(body: &str) -> Vec<String> {
    static TAG: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]*>").expect("固定の正規表現"));
    body.lines()
        .map(|l| TAG.replace_all(l, "").trim().to_string())
        .collect()
}

/// 重大度の行 = タグを除いて最初に空でない行の位置。
fn severity_line_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|l| !l.is_empty())
}

/// `**…**` だけの太字の行なら、その中身を返す (`**A** · **B**` のように太字が並ぶ行は除く)。
fn bold_only(line: &str) -> Option<&str> {
    let inner = line.strip_prefix("**")?.strip_suffix("**")?;
    (!inner.trim().is_empty() && !inner.contains("**")).then(|| inner.trim())
}

/// CodeRabbit コメント本文から severity を抽出。
///
/// 書式は 2 つある。
/// - 新 (2026-11〜): `**<picture>…</picture> Major** · **Functional Correctness** · **Quick win**`。
///   重大度は先頭の太字の中だけで読む。同じ行の `**Low value**` などの労力の表示を、重大度の
///   `Low` と取り違えないため
/// - 旧: `_⚠️ Potential issue_ | _🟠 Major_`。行全体の語と絵文字で読む
///
/// 読めない場合は `Info` (旧来の既定値)。
pub(crate) fn extract_severity(body: &str) -> String {
    let lines = plain_lines(body);
    let Some(line) = severity_line_index(&lines).map(|i| lines[i].as_str()) else {
        return "Info".to_string();
    };
    let label = match line.strip_prefix("**").and_then(|rest| rest.split_once("**")) {
        Some((first_bold, _)) => first_bold.trim(),
        None => line,
    };
    severity_of_label(label).to_string()
}

fn severity_of_label(label: &str) -> &'static str {
    if label.contains("Critical") || label.contains("🔴") {
        "Critical"
    } else if label.contains("Major") || label.contains("🟠") {
        "Major"
    } else if label.contains("Minor") || label.contains("🟡") {
        "Minor"
    } else if label.contains("High") {
        "High"
    } else if label.contains("Low") {
        "Low"
    } else {
        "Info"
    }
}

/// CodeRabbit コメント本文から指摘内容を抽出 (太字だけの行を優先、無ければ重大度の行の次の
/// 意味のある行)。重大度を読めた行 (`**Minor**` だけの行など) は見出しに選ばない。
pub(crate) fn extract_issue(body: &str) -> String {
    let lines = plain_lines(body);
    let severity_at = severity_line_index(&lines);
    let is_severity_label = |i: usize| Some(i) == severity_at && extract_severity(body) != "Info";
    let heading = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !is_severity_label(*i))
        .find_map(|(_, l)| bold_only(l));
    if let Some(heading) = heading {
        return heading.to_string();
    }
    let after_severity = severity_at.map_or(0, |i| i + 1);
    lines
        .iter()
        .skip(after_severity)
        .find(|l| !l.is_empty() && !l.starts_with('_'))
        .map(|l| truncate_str(l, 100))
        .unwrap_or_else(|| "(詳細はコメント参照)".to_string())
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

    /// 2026-11 からの書式 (#625 の Major の指摘の本文から、行の構造を保って抜粋)。
    /// 1 行目が HTML コメント、重大度の行にアイコンの `<picture>`、見出しは折りたたみの後。
    const NEW_FORMAT_MAJOR: &str = concat!(
        "<!-- coderabbit-ai-generated:v1 -->\n",
        "\n",
        "**<picture><img src=\"https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/major-16.svg\" width=\"16\" height=\"16\" align=\"absmiddle\" alt=\"\"></picture> Major** · **Functional Correctness** · **Heavy lift**\n",
        "\n",
        "<details>\n",
        "<summary><picture><img src=\"https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/review-details.svg\" width=\"16\" height=\"16\" align=\"absmiddle\" alt=\"\"></picture> Supported by static analysis</summary>\n",
        "<dl>\n",
        "<dd>\n",
        "<br>\n",
        "</dl>\n",
        "</details>\n",
        "\n",
        "**通常監視でも全ページのコメントを取得してください。**\n",
        "\n",
        "CodeRabbit の要約が最初のページにない場合、失敗した pre-merge check を見落とします。\n",
    );

    /// 同じ書式の Minor (#626)。同じ行に労力の表示 `**Low value**` がある。
    const NEW_FORMAT_MINOR_LOW_VALUE: &str = concat!(
        "<!-- coderabbit-ai-generated:v1 -->\n",
        "\n",
        "**<picture><img src=\"https://storage.googleapis.com/coderabbit_public_assets/review-comment-icons-v1/amber-16.svg\" width=\"16\" height=\"16\" align=\"absmiddle\" alt=\"\"></picture> Minor** · **Functional Correctness** · **Low value**\n",
        "\n",
        "**`Thread::title` の選択方法を訂正してください。**\n",
    );

    #[test]
    fn new_format_severity_is_read_after_the_html_comment_line() {
        assert_eq!(extract_severity(NEW_FORMAT_MAJOR), "Major");
        assert_eq!(extract_severity(NEW_FORMAT_MINOR_LOW_VALUE), "Minor");
    }

    /// 労力の表示 (`Low value`) を重大度の `Low` と読まない。先頭の太字だけを見る。
    #[test]
    fn new_format_effort_label_is_not_read_as_severity() {
        let trivial = NEW_FORMAT_MINOR_LOW_VALUE.replace("Minor**", "Trivial**");
        assert_eq!(extract_severity(&trivial), "Info");
    }

    #[test]
    fn new_format_issue_is_the_heading_not_the_severity_line() {
        assert_eq!(extract_issue(NEW_FORMAT_MAJOR), "通常監視でも全ページのコメントを取得してください。");
        assert_eq!(extract_summary(NEW_FORMAT_MINOR_LOW_VALUE), "`Thread::title` の選択方法を訂正してください。");
    }

    /// 重大度だけの太字の行 (`**Minor**`) も、見出しに選ばない。
    #[test]
    fn severity_only_bold_line_is_not_the_heading() {
        let body = "<!-- coderabbit-ai-generated:v1 -->\n**<picture></picture> Minor**\n\n**本当の見出し**\n";
        assert_eq!(extract_severity(body), "Minor");
        assert_eq!(extract_issue(body), "本当の見出し");
    }

    #[test]
    fn old_format_still_reads_severity_and_heading() {
        let body = "_⚠️ Potential issue_ | _🟠 Major_\n\n**旧書式の見出し**\n";
        assert_eq!(extract_severity(body), "Major");
        assert_eq!(extract_issue(body), "旧書式の見出し");
    }

    #[test]
    fn parse_findings_reads_new_format_severity_and_issue() {
        let json = serde_json::json!([{
            "id": 1, "user": {"login": "coderabbitai[bot]"}, "body": NEW_FORMAT_MAJOR,
            "path": "src/main.rs", "line": 7, "created_at": "2026-11-08T05:25:25Z"
        }])
        .to_string();
        let findings = parse_findings(&json, "2026-11-08T00:00:00Z");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "Major");
        assert_eq!(findings[0].issue, "通常監視でも全ページのコメントを取得してください。");
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
        let output = ListFindingsOutput { findings, pre_merge_checks: None };
        let serialized = serde_json::to_value(&output).unwrap();
        let item = &serialized["findings"][0];
        assert_eq!(item["severity"], "Major");
        assert_eq!(item["file"], "src/lib.rs");
        assert_eq!(item["line"], 100);
        assert_eq!(item["summary"], "signature mismatch");
        assert_eq!(item["url"], "https://github.com/o/r/pull/1#r1");
    }
}
