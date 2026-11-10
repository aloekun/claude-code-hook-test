//! Inline review comments → Finding / ListedFinding 変換 (順位 209 / 順位 208 PR A refactor)。

pub(crate) use lib_report_formatter::UNKNOWN_SEVERITY;
use lib_report_formatter::Finding;

use crate::models::{GhPullComment, ListedFinding};

/// PR インラインレビューコメント (pulls/{pr}/comments) を [`Finding`] に変換する。
///
/// 対象はスレッドの根 (= レビュー指摘) だけで、CodeRabbit の reply は含めない。reply は
/// 重大度の行を持たないので `Unknown` になり、指摘の重大度を読めなかったと誤って数えるため
/// (順位 603。CodeRabbit は対応の返信ごとに reply を付けるので、fix push 後の監視ではほぼ毎回
/// 混ざる)。reply は以前も `Info` に落ちて fix の対象にならなかったので、外しても fix は変わらない。
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
    let is_thread_root = c.in_reply_to_id.is_none();
    is_coderabbit && after_push_time && is_thread_root
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
///
/// インラインコード (`` `Option<u64>` ``) の中はタグとみなさず残す。山括弧は型の表記であって、
/// 消すと見出しの意味が変わるため。
fn plain_lines(body: &str) -> Vec<String> {
    body.lines()
        .map(|l| strip_tags_outside_code(l).trim().to_string())
        .collect()
}

/// 1 行から、インラインコードの外にある HTML のタグだけを除く。
///
/// インラインコードは CommonMark と同じく、n 個連続した `` ` `` から、次に同じ n 個が
/// 連続する箇所までとする。閉じる側が無ければ、残りはコードではない (タグを除く)。
fn strip_tags_outside_code(line: &str) -> String {
    static TAG: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]*>").expect("固定の正規表現"));
    let mut out = String::new();
    let mut rest = line;
    while let Some(open_at) = rest.find('`') {
        let run = rest[open_at..].chars().take_while(|&c| c == '`').count();
        let fence = &rest[open_at..open_at + run];
        let after_open = &rest[open_at + run..];
        let Some(close_at) = find_backtick_run(after_open, run) else {
            break;
        };
        out.push_str(&TAG.replace_all(&rest[..open_at], ""));
        out.push_str(fence);
        out.push_str(&after_open[..close_at + run]);
        rest = &after_open[close_at + run..];
    }
    out.push_str(&TAG.replace_all(rest, ""));
    out
}

/// `text` の中で、ちょうど `run` 個連続した `` ` `` が始まる位置 (前後に `` ` `` が続かないもの)。
fn find_backtick_run(text: &str, run: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let len = bytes[i..].iter().take_while(|&&b| b == b'`').count();
        if len == run {
            return Some(i);
        }
        i += len;
    }
    None
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
/// - 旧: `_⚠️ Potential issue_ | _🟠 Major_`。`_` / `|` / 空白で区切った語と絵文字で読む
///
/// どちらも部分一致では読まない。重大度の無い本文の先頭が `**Highlight …**` のような見出しでも、
/// `High` と取り違えないため。読めない場合は [`UNKNOWN_SEVERITY`]。
///
/// 既定値を `Info` にしていた頃は、CodeRabbit の書式変更で全件が `Info` と読まれても正常な値に
/// 見え、Major の指摘が承認側へ流れうる状態が約 1 日続いた (#625 / #627)。
pub(crate) fn extract_severity(body: &str) -> String {
    let lines = plain_lines(body);
    let Some(line) = severity_line_index(&lines).map(|i| lines[i].as_str()) else {
        return UNKNOWN_SEVERITY.to_string();
    };
    let severity = match line.strip_prefix("**").and_then(|rest| rest.split_once("**")) {
        Some((first_bold, _)) => severity_of_label(first_bold.trim()),
        None => severity_of_tokens(line),
    };
    severity.unwrap_or(UNKNOWN_SEVERITY).to_string()
}

/// 新書式の先頭の太字。中身全体が重大度の名前と一致するときだけ読む。
fn severity_of_label(label: &str) -> Option<&'static str> {
    SEVERITIES.iter().find(|(name, _)| *name == label).map(|(name, _)| *name)
}

/// 旧書式の行。語に分けて、重大度の名前か絵文字と一致する語を、重い順に探す。
fn severity_of_tokens(line: &str) -> Option<&'static str> {
    let tokens: Vec<&str> = line
        .split(|c: char| c == '_' || c == '|' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    SEVERITIES
        .iter()
        .find(|(name, emoji)| tokens.iter().any(|t| t == name || (!emoji.is_empty() && t == emoji)))
        .map(|(name, _)| *name)
}

/// 読み取る重大度と、旧書式での絵文字 (重い順)。
///
/// `Trivial` の旧書式の絵文字は載せない。#560〜#628 のインライン指摘 (約 170 件) に、`Trivial` の
/// 実物が 1 件も無く、絵文字を確かめられなかったため (順位 603)。名前の語なら旧書式でも読む。
const SEVERITIES: [(&str, &str); 6] = [
    ("Critical", "🔴"),
    ("Major", "🟠"),
    ("Minor", "🟡"),
    ("High", ""),
    ("Low", ""),
    ("Trivial", ""),
];

/// CodeRabbit コメント本文から指摘内容を抽出 (太字だけの行を優先、無ければ重大度の行の次の
/// 意味のある行)。重大度を読めた行 (`**Minor**` だけの行など) は見出しに選ばない。
pub(crate) fn extract_issue(body: &str) -> String {
    let lines = plain_lines(body);
    let severity_at = severity_line_index(&lines);
    let is_severity_label =
        |i: usize| Some(i) == severity_at && extract_severity(body) != UNKNOWN_SEVERITY;
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

    /// 1 行目が HTML コメントでも、次の空でない行から重大度を読む (#625 の Major が Info になった不具合)。
    #[test]
    fn new_format_severity_is_read_after_the_html_comment_line() {
        assert_eq!(extract_severity(NEW_FORMAT_MAJOR), "Major");
        assert_eq!(extract_severity(NEW_FORMAT_MINOR_LOW_VALUE), "Minor");
    }

    /// 労力の表示 (`Low value`) を重大度の `Low` と読まない。先頭の太字だけを見る。
    #[test]
    fn new_format_effort_label_is_not_read_as_severity() {
        let unlisted = NEW_FORMAT_MINOR_LOW_VALUE.replace("Minor**", "Cosmetic**");
        assert_eq!(extract_severity(&unlisted), UNKNOWN_SEVERITY);
    }

    /// `Trivial` は `Trivial` のまま読み、重大度の行は見出しに選ばない (順位 603)。
    /// 以前は一覧に無く `Info` に落ち、読めなかった指摘と同じ表示になっていた。
    #[test]
    fn trivial_is_read_as_its_own_severity() {
        let trivial = NEW_FORMAT_MINOR_LOW_VALUE.replace("Minor**", "Trivial**");
        assert_eq!(extract_severity(&trivial), "Trivial");
        assert_eq!(extract_issue(&trivial), "`Thread::title` の選択方法を訂正してください。");
        assert_eq!(extract_severity("_🧹 Nitpick_ | _Trivial_"), "Trivial");
    }

    /// 重大度を読めない本文は `Unknown` にする。`Info` に落とすと、正常な値と見分けられない
    /// (順位 603。#625 の Major が全件 `Info` と読まれた)。
    #[test]
    fn unreadable_severity_is_unknown_not_info() {
        let no_severity_line = "<!-- coderabbit-ai-generated:v1 -->\n\nこの関数は境界を見ていません。\n";
        assert_eq!(extract_severity(no_severity_line), UNKNOWN_SEVERITY);
        assert_eq!(extract_severity(""), UNKNOWN_SEVERITY);
        assert_eq!(UNKNOWN_SEVERITY, "Unknown");
    }

    /// 重大度の無い本文の先頭の見出しを、部分一致で重大度と読まない (#627 の指摘)。
    #[test]
    fn heading_containing_a_severity_word_is_not_a_severity() {
        let body = "<!-- coderabbit-ai-generated:v1 -->\n**Highlight the failing case**\n\nbody\n";
        assert_eq!(extract_severity(body), UNKNOWN_SEVERITY);
        assert_eq!(extract_issue(body), "Highlight the failing case");
        assert_eq!(extract_severity("_⚠️ Potential issue_ | _Lowercase only_"), UNKNOWN_SEVERITY);
    }

    /// 旧書式は絵文字だけでも読む。
    #[test]
    fn old_format_reads_severity_from_emoji_token() {
        assert_eq!(extract_severity("_⚠️ Potential issue_ | _🔴_"), "Critical");
    }

    /// インラインコードの中の山括弧はタグとして消さない (#627 の指摘)。外のタグは消す。
    #[test]
    fn angle_brackets_inside_inline_code_are_kept() {
        let body = "**<picture></picture> Minor** · **x**\n\n**`Option<u64>` と ``Vec<`T`>`` を返す<br>**\n";
        assert_eq!(extract_issue(body), "`Option<u64>` と ``Vec<`T`>`` を返す");
    }

    /// 閉じていない `` ` `` の後ろはコードではないので、タグを消す。
    #[test]
    fn unclosed_backtick_does_not_protect_tags() {
        assert_eq!(strip_tags_outside_code("a ` b <br> c"), "a ` b  c");
    }

    /// 見出しには、重大度の行ではなく、太字だけの行を選ぶ。
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

    /// 旧書式 (`_⚠️ Potential issue_ | _🟠 Major_`) の互換。
    #[test]
    fn old_format_still_reads_severity_and_heading() {
        let body = "_⚠️ Potential issue_ | _🟠 Major_\n\n**旧書式の見出し**\n";
        assert_eq!(extract_severity(body), "Major");
        assert_eq!(extract_issue(body), "旧書式の見出し");
    }

    /// `--push-time` 経路 (`parse_findings`) でも、新書式の重大度と見出しが後段へ渡る。
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

    /// `parse_findings` は CodeRabbit の reply を指摘に含めない (順位 603)。reply は重大度の行を
    /// 持たず `Unknown` になるので、含めると「重大度を読めない指摘」と誤って数える。
    #[test]
    fn parse_findings_excludes_coderabbit_replies() {
        let json = serde_json::json!([
            {"id": 1, "user": {"login": "coderabbitai[bot]"}, "body": NEW_FORMAT_MAJOR,
             "path": "src/main.rs", "line": 7, "created_at": "2026-11-08T05:25:25Z"},
            {"id": 2, "user": {"login": "coderabbitai[bot]"}, "in_reply_to_id": 1,
             "body": "<!-- coderabbit-ai-generated:v1 -->\n\n@aloekun 対応ありがとうございます。",
             "path": "src/main.rs", "line": 7, "created_at": "2026-11-08T06:00:00Z"}
        ])
        .to_string();
        let findings = parse_findings(&json, "2026-11-08T00:00:00Z");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "Major");
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
        let output = ListFindingsOutput {
            severity_readout: crate::severity_readout::read_severities(&findings, |f| &f.severity),
            findings,
            pre_merge_checks: None,
        };
        let serialized = serde_json::to_value(&output).unwrap();
        let item = &serialized["findings"][0];
        assert_eq!(item["severity"], "Major");
        assert_eq!(item["file"], "src/lib.rs");
        assert_eq!(item["line"], 100);
        assert_eq!(item["summary"], "signature mismatch");
        assert_eq!(item["url"], "https://github.com/o/r/pull/1#r1");
        assert_eq!(serialized["severity_readout"]["review_findings"], 1);
        assert_eq!(serialized["severity_readout"]["unknown"], 0);
        assert_eq!(serialized["severity_readout"]["unreadable"], false);
    }
}
