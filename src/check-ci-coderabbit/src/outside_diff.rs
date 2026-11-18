//! レビュー本文の「Outside diff range comments」節 → [`ListedFinding`] (順位 610)。
//!
//! CodeRabbit は、差分の外の行への指摘をインラインのスレッドにせず、レビュー本文の引用ブロックに
//! 書く。インラインコメント (`pulls/{pr}/comments`) だけを見ていると取りこぼす (#108 / #636 /
//! #649 で 3 回、未対応のままマージや「未解決 0 件」の報告に至った)。
//!
//! 節の書式 (2026-11、#636 / #649 で実測):
//!
//! ```text
//! > **⚠️ Outside diff range comments (1)**
//! > <details>
//! > <summary>… Minor</strong> · 見出し · <code>guide.md:192</code></summary><blockquote>
//! > `docs/guide.md:192`
//! > **<picture>…</picture> Minor** · **Functional Correctness** · **Quick win**
//! > **見出し**
//! > 本文
//! > Also applies to: 279-279
//! ```
//!
//! 指摘 1 件は、バッククォートだけで囲んだ `パス:行` の行から始まる。重大度と見出しは、
//! インラインコメントと同じ読み方 ([`extract_severity`] / [`extract_summary`]) で読む。
//!
//! 見出しの件数と読めた件数が合わないとき、見出しに件数が無いときは、0 件にせず
//! [`UnreadableReviewBody`] を返す ([ADR-043] の fail-closed)。書式が変わって 0 件に見えると、
//! 取りこぼしが「指摘なし」に化けるため。
//!
//! [ADR-043]: ../../../docs/adr/adr-043-security-gates-fail-closed.md

use std::sync::LazyLock;

use regex::Regex;

use crate::findings::{extract_severity, extract_summary};
use crate::models::{FindingSource, GhReview, ListedFinding, UnreadableReviewBody};

/// 節があることを示す語。見出しの件数が読めなくても、この語があれば節はある。
const SECTION_PHRASE: &str = "Outside diff range comment";

static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Outside diff range comments? \((\d+)\)").unwrap());
static LOCATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^`([^`\s]+):(\d+)(?:-\d+)?`$").unwrap());
static ALSO_APPLIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Also applies to:\s*(.+?)\s*$").unwrap());

/// レビュー本文の指摘と、節を読めなかったレビュー。
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ReviewBodyFindings {
    pub(crate) findings: Vec<ListedFinding>,
    pub(crate) unreadable: Vec<UnreadableReviewBody>,
}

/// `push_time` 以降の CodeRabbit のレビュー (`pulls/{pr}/reviews`) から、本文の指摘を集める。
///
/// 解決済みかどうかは機械では分からないので、古いレビューの指摘も含める。各指摘の `commit` で、
/// どの時点のコードへの指摘かを示す。
pub(crate) fn parse_review_body_findings(reviews_json: &str, push_time: &str) -> Result<ReviewBodyFindings, String> {
    let reviews: Vec<GhReview> =
        serde_json::from_str(reviews_json).map_err(|e| format!("reviews JSON パースエラー: {e}"))?;
    let mut out = ReviewBodyFindings::default();
    for review in reviews.iter().filter(|r| is_coderabbit_after(r, push_time)) {
        let url = review.html_url.clone().unwrap_or_default();
        match read_section(review.body.as_deref().unwrap_or("")) {
            Ok(items) => out.findings.extend(items.into_iter().map(|item| ListedFinding {
                severity: item.severity,
                file: item.file,
                line: item.line,
                summary: item.summary,
                url: url.clone(),
                source: FindingSource::ReviewBody,
                commit: review.commit_id.clone(),
                also_applies_to: item.also_applies_to,
            })),
            Err(reason) => out.unreadable.push(UnreadableReviewBody { url, reason }),
        }
    }
    Ok(out)
}

fn is_coderabbit_after(review: &GhReview, push_time: &str) -> bool {
    let login = review.user.as_ref().and_then(|u| u.login.as_deref());
    let submitted = review.submitted_at.as_deref();
    login == Some("coderabbitai[bot]") && submitted.is_some_and(|t| t >= push_time)
}

/// 節の指摘 1 件。
#[derive(Debug, PartialEq)]
struct SectionItem {
    file: String,
    line: u64,
    severity: String,
    summary: String,
    also_applies_to: Option<String>,
}

/// 本文から節の指摘を読む。節が無ければ空。読めなければ理由を返す。
fn read_section(body: &str) -> Result<Vec<SectionItem>, String> {
    if !body.contains(SECTION_PHRASE) {
        return Ok(Vec::new());
    }
    let lines: Vec<&str> = body.lines().collect();
    let (heading_at, expected) = lines
        .iter()
        .enumerate()
        .find_map(|(i, l)| Some((i, HEADING.captures(l)?[1].parse::<usize>().ok()?)))
        .ok_or_else(|| format!("「{SECTION_PHRASE}」はあるが、見出しの件数が読めない"))?;
    let section: Vec<&str> = lines[heading_at + 1..]
        .iter()
        .map_while(|l| unquote(l))
        .collect();
    let items = split_items(&section);
    if items.len() != expected {
        return Err(format!("見出しは {expected} 件だが、読めた指摘は {} 件", items.len()));
    }
    Ok(items)
}

/// 引用ブロックの 1 行から `>` を外す。引用ブロックの外なら None (節の終わり)。
fn unquote(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// 節の行を、`パス:行` の行ごとに指摘へ分ける。
fn split_items(section: &[&str]) -> Vec<SectionItem> {
    let starts: Vec<(usize, String, u64)> = section
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let caps = LOCATION.captures(l.trim())?;
            Some((i, caps[1].to_string(), caps[2].parse().ok()?))
        })
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(n, (at, file, line))| {
            let end = starts.get(n + 1).map_or(section.len(), |(next, _, _)| *next);
            let text = section[at + 1..end].join("\n");
            SectionItem {
                file: file.clone(),
                line: *line,
                severity: extract_severity(&text),
                summary: extract_summary(&text),
                also_applies_to: also_applies_to(&text),
            }
        })
        .collect()
}

fn also_applies_to(text: &str) -> Option<String> {
    text.lines().find_map(|l| Some(ALSO_APPLIES.captures(l.trim())?[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PR636: &str = include_str!("../fixtures/review-body/pr636-outside-diff-also-applies.txt");
    const PR649: &str = include_str!("../fixtures/review-body/pr649-outside-diff.txt");
    const PR650: &str = include_str!("../fixtures/review-body/pr650-no-section.txt");

    /// #636 の指摘が指していたファイル。fixture から読める値であって、台帳への参照ではない
    /// (`no-ephemeral-todo-reference` の対象にしないため、パスを分けて書く)。
    const PR636_FILE: &str = concat!("docs/", "todo19.md");

    fn reviews_json(entries: &[(&str, &str, &str)]) -> String {
        let reviews: Vec<serde_json::Value> = entries
            .iter()
            .map(|(login, submitted_at, body)| {
                serde_json::json!({
                    "user": {"login": login},
                    "submitted_at": submitted_at,
                    "body": body,
                    "commit_id": "59ffd7c5ed59bcebc2cf019d285c6d1426337f6b",
                    "html_url": "https://github.com/o/r/pull/1#pullrequestreview-9",
                })
            })
            .collect();
        serde_json::to_string(&reviews).unwrap()
    }

    #[test]
    fn reads_the_pr636_item_with_also_applies_to() {
        let items = read_section(PR636).unwrap();
        assert_eq!(
            items,
            vec![SectionItem {
                file: PR636_FILE.to_string(),
                line: 192,
                severity: "Minor".to_string(),
                summary: "順位 223 の旧 Tier 表記を更新してください。".to_string(),
                also_applies_to: Some("279-279".to_string()),
            }]
        );
    }

    #[test]
    fn reads_the_pr649_item_without_also_applies_to() {
        let items = read_section(PR649).unwrap();
        assert_eq!(
            items,
            vec![SectionItem {
                file: "src/hooks-pre-tool-validate/src/presets/safety/wsl_direct.rs".to_string(),
                line: 52,
                severity: "Minor".to_string(),
                summary: "シェルの区切り文字を末尾条件に含めてください。".to_string(),
                also_applies_to: None,
            }]
        );
    }

    /// 本文の中のインラインコードの `パス:行` は、指摘の始まりではない。行全体がそれのときだけ区切る。
    #[test]
    fn a_path_and_line_inside_a_sentence_does_not_start_another_item() {
        let body = PR649.replace(
            "> `wsl|cat` は Bash で",
            "> 同じ書き方が `src/hooks-pre-tool-validate/src/presets/safety/script_runtime.rs:118` にもあります。\n> `wsl|cat` は Bash で",
        );
        assert_ne!(body, PR649, "fixture の文が変わっていたら置換先を直す");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    #[test]
    fn a_body_without_the_section_has_no_items() {
        assert_eq!(read_section(PR650), Ok(Vec::new()));
    }

    #[test]
    fn a_count_mismatch_is_unreadable_not_zero() {
        let body = PR649.replace("Outside diff range comments (1)", "Outside diff range comments (2)");
        assert_eq!(read_section(&body), Err("見出しは 2 件だが、読めた指摘は 1 件".to_string()));
    }

    #[test]
    fn a_changed_location_line_is_unreadable_not_zero() {
        let body = PR649.replace(
            "`src/hooks-pre-tool-validate/src/presets/safety/wsl_direct.rs:52`",
            "src/hooks-pre-tool-validate/src/presets/safety/wsl_direct.rs line 52",
        );
        assert_eq!(read_section(&body), Err("見出しは 1 件だが、読めた指摘は 0 件".to_string()));
    }

    #[test]
    fn a_heading_without_a_count_is_unreadable() {
        let body = PR649.replace("Outside diff range comments (1)", "Outside diff range comments");
        assert!(read_section(&body).unwrap_err().contains("見出しの件数が読めない"));
    }

    #[test]
    fn collects_coderabbit_reviews_after_push_time_with_commit_and_url() {
        let json = reviews_json(&[
            ("coderabbitai[bot]", "2026-11-12T16:43:21Z", PR636),
            ("coderabbitai[bot]", "2026-11-01T00:00:00Z", PR649),
            ("aloekun", "2026-11-13T00:00:00Z", PR649),
            ("coderabbitai[bot]", "2026-11-13T00:00:00Z", PR650),
        ]);
        let got = parse_review_body_findings(&json, "2026-11-10T00:00:00Z").unwrap();
        assert!(got.unreadable.is_empty());
        assert_eq!(got.findings.len(), 1);
        let f = &got.findings[0];
        assert_eq!((f.file.as_str(), f.line, f.source), (PR636_FILE, 192, FindingSource::ReviewBody));
        assert_eq!(f.commit.as_deref(), Some("59ffd7c5ed59bcebc2cf019d285c6d1426337f6b"));
        assert_eq!(f.url, "https://github.com/o/r/pull/1#pullrequestreview-9");
    }

    #[test]
    fn reports_an_unreadable_review_by_url() {
        let broken = PR649.replace("Outside diff range comments (1)", "Outside diff range comments (3)");
        let json = reviews_json(&[("coderabbitai[bot]", "2026-11-17T00:00:00Z", &broken)]);
        let got = parse_review_body_findings(&json, "1970-01-01T00:00:00Z").unwrap();
        assert!(got.findings.is_empty());
        assert_eq!(
            got.unreadable,
            vec![UnreadableReviewBody {
                url: "https://github.com/o/r/pull/1#pullrequestreview-9".to_string(),
                reason: "見出しは 3 件だが、読めた指摘は 1 件".to_string(),
            }]
        );
    }

    #[test]
    fn broken_json_is_an_error() {
        assert!(parse_review_body_findings("{", "1970-01-01T00:00:00Z").is_err());
    }
}
