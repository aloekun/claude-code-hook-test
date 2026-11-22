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
/// 節の中のコードブロックが閉じないまま節が終わったときの理由。
const UNCLOSED_FENCE: &str = "節の中のコードブロックが閉じていない";
/// 見出しと読んだ行の節より後ろに、節の語がもう一度出たときの理由。
const PHRASE_OUTSIDE_SECTION: &str = "「Outside diff range comment」が、見出しと読んだ行の節より後ろにもある";

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
    let unreadable = || format!("「{SECTION_PHRASE}」はあるが、見出しの件数が読めない");
    let heading_at = lines.iter().position(|l| l.contains(SECTION_PHRASE)).ok_or_else(unreadable)?;
    let expected = HEADING
        .captures(lines[heading_at])
        .and_then(|caps| caps[1].parse::<usize>().ok())
        .ok_or_else(unreadable)?;
    let section: Vec<&str> = lines[heading_at + 1..]
        .iter()
        .copied()
        .take_while(|l| unquote(l).is_some())
        .collect();
    if phrase_reappears(&lines[heading_at + 1 + section.len()..]) {
        return Err(PHRASE_OUTSIDE_SECTION.to_string());
    }
    let items = split_items(&section)?;
    if items.len() != expected {
        return Err(format!("見出しは {expected} 件だが、読めた指摘は {} 件", items.len()));
    }
    Ok(items)
}

/// 見出しと読んだ行の節より後ろ (`after_section`) に、節の語がもう一度出るか。
///
/// 見出しと読んだ行が本文中の語句で、本物の見出しが空行の後ろにあると、節が空に見えて件数 0 と
/// 照合が合ってしまう。語がもう一度出たら、どちらが見出しか決められないので読めない扱いにする。
fn phrase_reappears(after_section: &[&str]) -> bool {
    after_section.iter().any(|l| l.contains(SECTION_PHRASE))
}

/// 引用ブロックの 1 行から `>` を外す。引用ブロックの外なら None (節の終わり)。
fn unquote(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// 引用ブロックの 1 行の中身の字下げ (桁数) と、字下げの後ろの文字列。引用ブロックの外なら None。
///
/// CommonMark はタブを 4 桁単位のタブ位置まで進めて数え、`>` の後ろの省略できる空白 1 個は、
/// タブの一部でもよい。そのため `> \t` の後ろの字下げは 2 桁で、fence になりうる
/// (PR #657 の CodeRabbit の指摘)。`>` を外した後の文字列だけでは桁位置が分からないので、
/// 元の行の桁で数える。
fn quoted_content_indent(line: &str) -> Option<(usize, &str)> {
    const TAB_STOP: usize = 4;
    let rest = line.strip_prefix('>')?;
    let content_start = if rest.starts_with([' ', '\t']) { 2 } else { 1 };
    let mut column = 1;
    for (at, c) in rest.char_indices() {
        match c {
            ' ' => column += 1,
            '\t' => column += TAB_STOP - column % TAB_STOP,
            _ => return Some((column.saturating_sub(content_start), &rest[at..])),
        }
    }
    Some((column.saturating_sub(content_start), ""))
}

/// 節の行を、`パス:行` の行ごとに指摘へ分ける。
///
/// コードブロック (```` ``` ```` か `~~~` で囲んだ範囲) の中の `パス:行` は、本文に書かれた例であって
/// 指摘の始まりではない。数えると見出しの件数と合わなくなり、節全体が読めない扱いになる。
///
/// ブロックが閉じないまま節が終われば、件数が合っていても [`UNCLOSED_FENCE`] を返す。閉じないブロックの
/// 後ろに `パス:行` があったかどうかを区別できないため ([ADR-043] の fail-closed)。
///
/// `section` は引用ブロックの行を `>` を付けたまま受け取る (字下げを元の行の桁で数えるため)。
///
/// [ADR-043]: ../../../docs/adr/adr-043-security-gates-fail-closed.md
fn split_items(quoted_section: &[&str]) -> Result<Vec<SectionItem>, String> {
    let section: Vec<&str> = quoted_section.iter().map(|l| unquote(l).unwrap_or(l)).collect();
    let mut open: Option<Fence> = None;
    let mut starts: Vec<(usize, String, u64)> = Vec::new();
    for (i, (quoted, unquoted)) in quoted_section.iter().zip(&section).enumerate() {
        let l = unquoted.trim();
        match (open, Fence::of(quoted)) {
            (None, Some(f)) => {
                open = Some(f);
                continue;
            }
            (Some(o), Some(f)) if f.closes(&o, l) => {
                open = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        let Some(caps) = LOCATION.captures(l) else { continue };
        if let Ok(line) = caps[2].parse() {
            starts.push((i, caps[1].to_string(), line));
        }
    }
    if open.is_some() {
        return Err(UNCLOSED_FENCE.to_string());
    }
    Ok(starts
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
        .collect())
}

/// コードブロックの開き・閉じの行 (CommonMark の fence)。記号を 3 個以上並べる。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fence {
    mark: char,
    len: usize,
}

impl Fence {
    /// 引用ブロックの 1 行 (`>` 付き) が fence で始まっていれば、その記号と個数。
    ///
    /// 中身の字下げ ([`quoted_content_indent`]、タブは桁位置で数える) は 3 桁まで許す。4 桁以上なら、
    /// CommonMark では字下げのコードブロックの行であって fence ではない。
    ///
    /// バッククォートの fence は、記号の後ろ (info string) にバッククォートを含められない。
    /// ```` ``` ```` の例です」のように後ろにもバッククォートがある行はインラインコードで、fence ではない
    /// (PR #657 の CodeRabbit の指摘)。`~~~` の後ろは何でもよい。
    fn of(quoted_line: &str) -> Option<Fence> {
        const MAX_FENCE_INDENT: usize = 3;
        let (indent, line) = quoted_content_indent(quoted_line)?;
        if indent > MAX_FENCE_INDENT {
            return None;
        }
        let mark = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
        let len = line.chars().take_while(|c| *c == mark).count();
        let info = &line[len * mark.len_utf8()..];
        if len < 3 || (mark == '`' && info.contains('`')) {
            return None;
        }
        Some(Fence { mark, len })
    }

    /// `opened` で開いたブロックを閉じる行か。同じ記号を開きと同じ数以上並べ、後ろに何も書かない。
    fn closes(self, opened: &Fence, line: &str) -> bool {
        self.mark == opened.mark && self.len >= opened.len && line.chars().all(|c| c == self.mark)
    }
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

    /// コードブロックの中に行全体が `パス:行` の例があっても、指摘の始まりにしない。
    #[test]
    fn a_path_and_line_inside_a_code_block_does_not_start_another_item() {
        let body = PR649.replace(
            "> `wsl|cat` は Bash で",
            "> ```\n> `src/example.rs:10`\n> ```\n> `wsl|cat` は Bash で",
        );
        assert_ne!(body, PR649, "fixture の文が変わっていたら置換先を直す");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// PR649 の本物の位置行の前に `block` を差し込む。ブロックが閉じなければ本物の指摘も読めなくなる
    /// ので、閉じの判定も確かめられる。
    fn with_block_before_the_location(block: &str) -> String {
        const LOCATION_LINE: &str = "> `src/hooks-pre-tool-validate/src/presets/safety/wsl_direct.rs:52`";
        let body = PR649.replace(LOCATION_LINE, &format!("{block}\n{LOCATION_LINE}"));
        assert_ne!(body, PR649, "fixture の位置行が変わっていたら置換先を直す");
        body
    }

    /// `~~~` のブロックも同じ。ブロック内の ```` ``` ```` は閉じではなく中身として読む。
    #[test]
    fn a_path_and_line_inside_a_tilde_code_block_does_not_start_another_item() {
        let body = with_block_before_the_location("> ~~~text\n> ```\n> `src/example.rs:10`\n> ~~~");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// 開きより短い fence は閉じではない。
    #[test]
    fn a_shorter_fence_does_not_close_a_longer_one() {
        let body = with_block_before_the_location("> ````\n> ```\n> `src/example.rs:10`\n> ````");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// (a) 閉じないコードブロックは、指摘がすべてその前にあって件数が合っても、節を読めない扱いにする。
    /// 閉じないブロックより後ろの `パス:行` を読み落としたのか、本当に無いのかを区別できないため。
    #[test]
    fn an_unclosed_fence_after_every_item_is_unreadable() {
        let body = PR649.replace("> `wsl|cat` は Bash で", "> ```\n> `wsl|cat` は Bash で");
        assert_ne!(body, PR649, "fixture の文が変わっていたら置換先を直す");
        assert_eq!(read_section(&body), Err(UNCLOSED_FENCE.to_string()));
    }

    /// (a) 閉じないブロックが指摘より前にあれば、件数の照合より先に、閉じていないことを理由にする。
    #[test]
    fn an_unclosed_fence_before_the_items_is_reported_as_unclosed() {
        let body = with_block_before_the_location("> ```");
        assert_eq!(read_section(&body), Err(UNCLOSED_FENCE.to_string()));
    }

    /// (b) 記号の後ろに文字がある行 (```` ```text ````) は閉じではなく、ブロックの中身として読む。
    #[test]
    fn a_fence_with_an_info_string_does_not_close() {
        let body = with_block_before_the_location("> ```\n> ```text\n> `src/example.rs:10`\n> ```");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// (b) ```` ```text ```` しか後ろに無ければ、ブロックは閉じていない。
    #[test]
    fn a_block_followed_only_by_an_info_string_fence_is_unclosed() {
        let body = with_block_before_the_location("> ```\n> ```text");
        assert_eq!(read_section(&body), Err(UNCLOSED_FENCE.to_string()));
    }

    /// (c) 開きより長い fence は閉じになる。閉じた後の本物の指摘は読む。
    #[test]
    fn a_longer_fence_closes_a_shorter_one() {
        let body = with_block_before_the_location("> ```\n> `src/example.rs:10`\n> ````");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// (d) `>` を外した後の行頭に空白が 1〜3 個ある ```` ``` ```` は、fence として扱う (CommonMark)。
    #[test]
    fn a_fence_indented_by_up_to_three_spaces_is_a_fence() {
        for indent in 1..=3 {
            let pad = " ".repeat(indent);
            let body = with_block_before_the_location(&format!("> {pad}```\n> `src/example.rs:10`\n> {pad}```"));
            let items = read_section(&body).unwrap();
            assert_eq!(items.len(), 1, "字下げ {indent} 個");
            assert_eq!(items[0].line, 52, "字下げ {indent} 個");
        }
    }

    /// (d) 中身の字下げが 4 桁以上ある ```` ``` ```` は fence ではない (CommonMark では字下げの
    /// コードブロック)。fence と読むと、その後ろの本物の指摘が隠れる。タブは桁位置で数える
    /// (`> ` の後ろの `  \t` は 6 桁、`\t  ` は 4 桁)。
    #[test]
    fn a_fence_indented_by_four_columns_or_more_is_not_a_fence() {
        for pad in ["    ", "     ", "  \t", "\t  "] {
            let body = with_block_before_the_location(&format!("> {pad}```"));
            let items = read_section(&body).unwrap_or_else(|e| panic!("字下げ {pad:?}: {e}"));
            assert_eq!(items.len(), 1, "字下げ {pad:?}");
            assert_eq!(items[0].line, 52, "字下げ {pad:?}");
        }
    }

    /// (d) `>` の後ろのタブは、省略できる空白 1 個を兼ねて 4 桁目まで進むので、字下げは 2 桁で fence に
    /// なる (PR #657 の CodeRabbit の指摘)。開き・閉じのどちらでも fence として読む。
    #[test]
    fn a_tab_after_the_quote_marker_indents_a_fence_by_two_columns() {
        for (open, close) in [("> \t", "> \t"), (">\t", ">\t"), (">  \t", ">  \t"), ("> ", "> \t")] {
            let body = with_block_before_the_location(&format!("{open}```\n> `src/example.rs:10`\n{close}```"));
            let items = read_section(&body).unwrap_or_else(|e| panic!("開き {open:?} / 閉じ {close:?}: {e}"));
            assert_eq!(items.len(), 1, "開き {open:?} / 閉じ {close:?}");
            assert_eq!(items[0].line, 52, "開き {open:?} / 閉じ {close:?}");
        }
    }

    /// バッククォートの後ろにもバッククォートがある行 (インラインコード) は fence ではない。
    /// fence と読むと、閉じないブロックとして節全体が読めない扱いになる (PR #657 の CodeRabbit の指摘)。
    #[test]
    fn a_backtick_run_followed_by_another_backtick_is_not_a_fence() {
        let body = PR649.replace("> `wsl|cat` は Bash で", "> ```` ``` ```` の例です\n> `wsl|cat` は Bash で");
        assert_ne!(body, PR649, "fixture の文が変わっていたら置換先を直す");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// バッククォートを含まない info string (言語名) の開きと、`~~~` の後ろのバッククォートは、
    /// これまでどおり fence として読む。
    #[test]
    fn info_strings_without_backticks_and_tilde_fences_with_backticks_still_open() {
        for block in [
            "> ```rust\n> `src/example.rs:10`\n> ```",
            "> ~~~ `quoted`\n> `src/example.rs:10`\n> ~~~",
        ] {
            let items = read_section(&with_block_before_the_location(block)).unwrap_or_else(|e| panic!("{block:?}: {e}"));
            assert_eq!(items.len(), 1, "{block:?}");
            assert_eq!(items[0].line, 52, "{block:?}");
        }
    }

    /// (e) 見出しより前の本文に件数の語句があり、空行で本物の節と離れていても、0 件にせず読めない扱いにする。
    /// 先頭の語句を見出しと読むと、その後ろの節が空に見え、本物の指摘が「指摘なし」に化ける。
    #[test]
    fn a_count_phrase_before_the_heading_is_unreadable_not_zero() {
        let body = PR649.replace(
            "**Actionable comments posted: 1**",
            "**Actionable comments posted: 1**\n\n前回の Outside diff range comments (0) は対応済み",
        );
        assert_ne!(body, PR649, "fixture の文が変わっていたら置換先を直す");
        assert_eq!(read_section(&body), Err(PHRASE_OUTSIDE_SECTION.to_string()));
    }

    /// (e) 見出しの後ろ (節の中) の本文にある件数の語句は使わず、見出しの件数で照合する。
    #[test]
    fn a_count_phrase_inside_the_section_does_not_replace_the_heading_count() {
        let body =
            PR649.replace("> `wsl|cat` は Bash で", "> 例: `Outside diff range comments (0)`\n> `wsl|cat` は Bash で");
        assert_ne!(body, PR649, "fixture の文が変わっていたら置換先を直す");
        let items = read_section(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 52);
    }

    /// 件数は節の見出し行からだけ読む。見出しに件数が無ければ、本文中の例の件数を使わず読めない扱いにする。
    #[test]
    fn a_count_in_the_body_is_not_used_for_a_heading_without_a_count() {
        let body = PR649
            .replace("Outside diff range comments (1)", "Outside diff range comments")
            .replace("> `wsl|cat` は Bash で", "> `Outside diff range comments (0)`\n> `wsl|cat` は Bash で");
        assert!(body.contains("comments (0)"), "fixture の文が変わっていたら置換先を直す");
        assert!(read_section(&body).unwrap_err().contains("見出しの件数が読めない"));
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
