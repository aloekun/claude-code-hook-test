//! convention-declaration check — `docs/dev-conventions.md` のルール台帳ゲート
//! (順位 515 / defect-convergence-plan.md § Phase 5 の撤1-③)。
//!
//! # なぜ検査するか — 「ルールを書いて溜飲を下げる」経路を塞ぐ
//!
//! 本リポジトリは「強制力のないルール追加は即却下」を方針に持ちながら、`dev-conventions.md`
//! は 16 節 220 行まで育った。**「ルールを作らないルール」自身が強制されていなかった**ためで
//! ある (defect-convergence-plan.md § 根因)。
//!
//! そこで各節に **[`MECHANIZED`] / [`NOT_MECHANIZABLE`] / [`PLANNED`] のいずれかの宣言**を
//! 要求する。新しい節を書くたびに「これは機械化できるのか、できないならなぜか」を明示させる
//! ことで、判断を経ずにルールだけが増える経路が閉じる ([ADR-042](adr-042) の 3 step 判定を
//! 節単位で強制する層にあたる)。
//!
//! # 3 値である理由
//!
//! 2 値 (`機械化:` / `機械化不能:`) だと、**機械化できるが未実装**の節がどちらとも名乗れない。
//! 嘘の宣言を書くか検査を外すかの二択になり、どちらも台帳を腐らせる。`機械化予定:` に
//! 追跡先 (順位番号) を書かせることで、未実装のまま忘れられる状態を台帳側に可視化する。
//!
//! # 位置まで固定する理由
//!
//! 宣言は**節の最初の非空行**でなければならない。「節内のどこかにある」だと、長い節の末尾へ
//! 埋めても通ってしまい、読み手が節を読み始める時点では判定が見えない。宣言は節の性質を
//! 決めるものなので、本文より先に来る必要がある。

use std::path::Path;

use crate::Violation;

/// 検査対象。派生プロジェクトには存在しないことがあるため、無ければ違反なしで通す。
const CONVENTIONS_FILE: &str = "dev-conventions.md";

/// 機械化済み。値は強制している機構の名前 (custom lint rule / 検査スクリプト / hook)。
const MECHANIZED: &str = "機械化:";
/// 機械化できない。値は [ADR-042](adr-042) Step 1/2 に照らした判定理由。
const NOT_MECHANIZABLE: &str = "機械化不能:";
/// 機械化できるが未実装。値は追跡先 (台帳の順位番号など)。
const PLANNED: &str = "機械化予定:";

const DECLARATIONS: &[&str] = &[MECHANIZED, NOT_MECHANIZABLE, PLANNED];

/// 節見出しの開始。
///
/// `### ` 以下は節の内部構造なので対象外だが、明示的な除外は要らない — `"### x"` の 3 文字目は
/// `#` であり `"## "` に一致しないため、この判定で既に落ちる。
fn is_section_heading(line: &str) -> bool {
    line.starts_with("## ")
}

/// 宣言行なら、その接頭辞を返す。
fn declaration_prefix(line: &str) -> Option<&'static str> {
    let body = strip_list_marker(line.trim_start()).trim_start();
    let unbolded = body.strip_prefix("**").unwrap_or(body);
    DECLARATIONS
        .iter()
        .copied()
        .find(|prefix| unbolded.starts_with(prefix))
}

/// 箇条書きマーカーを 1 つだけ剥がす。
///
/// マーカーの後ろに空白を要求するのは、bold の `**` を箇条書きの `*` として食わないため。
/// 文字集合での一括 trim にすると `*` を全部剥がしてしまい、後段の bold 除去が到達不能になる。
fn strip_list_marker(line: &str) -> &str {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .unwrap_or(line)
}

/// 宣言に中身があるか。
///
/// **接頭辞だけの宣言を通さない。** `機械化:` とだけ書けば検査は満たされるが、どの機構が
/// 強制しているかは誰にも分からない。値の無い宣言を許すと、この検査は「その 4 文字を書く
/// 儀式」に退化する (origin-markers が値の無い `発火:` を弾くのと同じ理由)。
fn declaration_has_value(line: &str, prefix: &str) -> bool {
    let Some(index) = line.find(prefix) else {
        return false;
    };
    !strip_markup_and_punctuation(&line[index + prefix.len()..]).is_empty()
}

/// 宣言の値から markdown 装飾 (`*`) と句読点だけを剥がす。
///
/// `**機械化不能:**` のように装飾の閉じだけが続く行を「値あり」と誤判定しないため。
fn strip_markup_and_punctuation(rest: &str) -> &str {
    rest.trim_matches(|c: char| c.is_whitespace() || c == '*' || c == '。' || c == '、')
}

/// 1 節分の判定結果。
enum SectionVerdict {
    Ok,
    Missing,
    EmptyValue(&'static str),
}

/// 節本文 (見出しの次の行から次の見出しの手前まで) を判定する。
fn verdict_for(body: &[&str]) -> SectionVerdict {
    let Some(first) = body.iter().find(|line| !line.trim().is_empty()) else {
        return SectionVerdict::Missing;
    };
    let Some(prefix) = declaration_prefix(first) else {
        return SectionVerdict::Missing;
    };
    if declaration_has_value(first, prefix) {
        SectionVerdict::Ok
    } else {
        SectionVerdict::EmptyValue(prefix)
    }
}

/// 見出し行から節名を取り出す。
///
/// `trim_start_matches` は繰り返しを剥がすため、`## ## x` のような見出しで節名が変わる。
/// 接頭辞は 1 度だけ剥がす。
fn heading_title(heading: &str) -> &str {
    heading.strip_prefix("## ").unwrap_or(heading).trim()
}

fn missing_message(heading: &str) -> String {
    format!(
        "節「{}」の最初の非空行が機械化の宣言ではありません。\
         `{} <強制している機構>` / `{} <ADR-042 Step1/2 の判定理由>` / `{} <追跡先の順位>` の\
         いずれかを節の冒頭に書いてください (順位 515 のルール台帳ゲート)",
        heading_title(heading),
        MECHANIZED,
        NOT_MECHANIZABLE,
        PLANNED
    )
}

fn empty_value_message(heading: &str, prefix: &str) -> String {
    format!(
        "節「{}」の宣言 `{}` に中身がありません。接頭辞だけでは何が強制しているか (または\
         なぜ強制できないか) が読めないため、値を書いてください",
        heading_title(heading),
        prefix
    )
}

/// 開いているコードフェンスの種類と長さ。
#[derive(Clone, Copy)]
struct Fence {
    marker: char,
    len: usize,
}

/// フェンス行を解析した結果。`bare` は fence 文字の後ろが空 (= 閉じフェンスたりうる) か。
struct FenceRun {
    marker: char,
    len: usize,
    bare: bool,
}

/// 行がコードフェンスなら解析結果を返す。
///
/// **`~~~` も fence である。** ` ``` ` だけを見る実装は、`~~~` ブロック内の `## ` を節見出しとして
/// 拾い、正当な `dev-conventions.md` を違反にする (PR #492 CodeRabbit 指摘)。
/// 字下げ 4 以上はフェンスではなく indented code block なので対象外。
fn fence_run(line: &str) -> Option<FenceRun> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == marker).count();
    if len < 3 {
        return None;
    }
    Some(FenceRun {
        marker,
        len,
        bare: trimmed[len..].trim().is_empty(),
    })
}

/// フェンス行を読んだ後の状態。
///
/// 閉じられるのは**同じ文字種で、開きフェンス以上の長さで、情報文字列を持たない**行だけ。
/// 単純なトグルにすると、` ``` ` ブロック内の `~~~` や、開きより短い ` `` ` 行で閉じたことに
/// なってしまう。
fn next_fence_state(current: Option<Fence>, run: FenceRun) -> Option<Fence> {
    match current {
        None => Some(Fence {
            marker: run.marker,
            len: run.len,
        }),
        Some(open) if run.marker == open.marker && run.len >= open.len && run.bare => None,
        open => open,
    }
}

/// markdown 本文を検査する。ファイル読み込みから分離してあるのは fixture テストのため。
pub fn check_markdown(file: &str, markdown: &str) -> Vec<Violation> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut violations = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut heading: Option<(usize, &str)> = None;
    let mut body: Vec<&str> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(run) = fence_run(line) {
            fence = next_fence_state(fence, run);
            if heading.is_some() {
                body.push(line);
            }
            continue;
        }
        if fence.is_none() && is_section_heading(line) {
            if let Some((line_no, text)) = heading.take() {
                violations.extend(finish_section(file, line_no, text, &body));
            }
            heading = Some((index + 1, line));
            body.clear();
        } else if heading.is_some() {
            body.push(line);
        }
    }
    if let Some((line_no, text)) = heading {
        violations.extend(finish_section(file, line_no, text, &body));
    }
    violations
}

fn finish_section(file: &str, line: usize, heading: &str, body: &[&str]) -> Option<Violation> {
    let message = match verdict_for(body) {
        SectionVerdict::Ok => return None,
        SectionVerdict::Missing => missing_message(heading),
        SectionVerdict::EmptyValue(prefix) => empty_value_message(heading, prefix),
    };
    Some(Violation {
        file: file.to_string(),
        line,
        message,
    })
}

pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let path = docs_dir.join(CONVENTIONS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let markdown = std::fs::read_to_string(&path)
        .map_err(|e| format!("{} を読めません: {e}", path.display()))?;
    Ok(check_markdown(&path.display().to_string(), &markdown))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "docs/dev-conventions.md";

    fn messages(markdown: &str) -> Vec<String> {
        check_markdown(FILE, markdown)
            .into_iter()
            .map(|v| v.message)
            .collect()
    }

    #[test]
    fn accepts_each_declaration_kind_at_the_head_of_its_section() {
        let markdown = "# 開発 convention\n\n\
                        > 前書きは節ではない。\n\n\
                        ## A\n\n\
                        機械化: custom lint rule `no-unbounded-child-wait`\n\n本文\n\n\
                        ## B\n\n\
                        機械化不能: 検知方法を regex / AST で表現できない (ADR-042 Step 1)\n\n\
                        ## C\n\n\
                        機械化予定: 順位 445 (preamble ⇄ facet routing の集合比較 lint)\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// 宣言は見出し記法や箇条書きの中に置かれることがある。表記ゆれで落とさない。
    #[test]
    fn accepts_a_declaration_wrapped_in_bold_or_a_list_marker() {
        let markdown = "## A\n\n- **機械化: `pnpm lint:workflows` 契約検査 3**\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// `*` 箇条書きと bold の `**` は先頭 2 文字が紛らわしい。マーカー除去が `**` を
    /// 食うと bold 除去が効かなくなるため、両方を通すことを固定する。
    #[test]
    fn accepts_a_declaration_under_an_asterisk_list_marker_and_bold() {
        for line in ["* **機械化: x**", "* 機械化: x", "**機械化: x**", "機械化: x"] {
            let markdown = format!("## A\n\n{line}\n");
            assert!(
                check_markdown(FILE, &markdown).is_empty(),
                "宣言として認識されること: {line}"
            );
        }
    }

    #[test]
    fn flags_a_section_without_any_declaration() {
        let markdown = "## A\n\n本文だけの節。\n";
        let found = messages(markdown);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("機械化の宣言ではありません"), "{found:?}");
    }

    /// 節の**途中**にある宣言は通さない。読み始めた時点で判定が見えないため。
    #[test]
    fn flags_a_declaration_that_is_not_the_first_non_empty_line() {
        let markdown = "## A\n\n本文が先に来る。\n\n機械化不能: 後から書いた宣言\n";
        assert_eq!(messages(markdown).len(), 1);
    }

    /// 接頭辞だけの宣言は fail-closed で弾く。通すと検査が儀式に退化する。
    #[test]
    fn flags_a_declaration_with_no_value() {
        let markdown = "## A\n\n機械化:\n\n本文\n";
        let found = messages(markdown);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("中身がありません"), "{found:?}");
    }

    #[test]
    fn flags_a_bold_declaration_with_no_value() {
        let markdown = "## A\n\n**機械化不能:**\n";
        let found = messages(markdown);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("中身がありません"), "{found:?}");
    }

    #[test]
    fn flags_an_empty_section() {
        assert_eq!(messages("## A\n\n## B\n\n機械化: x\n").len(), 1);
    }

    /// `###` は節の内部構造。独立した宣言を要求しない。
    #[test]
    fn ignores_subsection_headings() {
        let markdown = "## A\n\n機械化不能: 理由\n\n### 小見出し\n\n本文\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// コードフェンス内の `## ` は見出しではない。
    #[test]
    fn ignores_headings_inside_a_fenced_block() {
        let markdown = "## A\n\n機械化不能: 理由\n\n```sh\n## これは見出しではない\n```\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// `~~~` も markdown のコードフェンスである。バックティックだけを見ると、`~~~` 内の
    /// `## ` を節見出しとして拾い、正当なファイルを違反にする。
    #[test]
    fn ignores_headings_inside_a_tilde_fenced_block() {
        let markdown = "## A\n\n機械化不能: 理由\n\n~~~sh\n## これは見出しではない\n~~~\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// 異なる文字種はフェンスを閉じない。単純なトグルだと ` ``` ` 内の `~~~` で閉じてしまう。
    #[test]
    fn a_different_fence_marker_does_not_close_the_block() {
        let markdown = "## A\n\n機械化不能: 理由\n\n```sh\n~~~\n## これは見出しではない\n```\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// 開きより短い run はフェンスを閉じない (CommonMark)。
    #[test]
    fn a_shorter_run_does_not_close_the_block() {
        let markdown = "## A\n\n機械化不能: 理由\n\n````sh\n```\n## これは見出しではない\n````\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// 情報文字列を持つ行は閉じフェンスにならない。
    #[test]
    fn a_run_with_an_info_string_does_not_close_the_block() {
        let markdown = "## A\n\n機械化不能: 理由\n\n```sh\n```rust\n## これは見出しではない\n```\n";
        assert!(check_markdown(FILE, markdown).is_empty());
    }

    /// フェンスが閉じた後の `## ` は通常どおり節見出しとして扱う。
    #[test]
    fn a_heading_after_a_closed_fence_is_still_a_section() {
        let markdown = "## A\n\n機械化不能: 理由\n\n~~~\nx\n~~~\n\n## B\n\n本文\n";
        let found = messages(markdown);
        assert_eq!(found.len(), 1, "節 B だけが違反: {found:?}");
        assert!(found[0].contains('B'), "{found:?}");
    }

    /// 字下げ 4 以上は fence ではなく indented code block。フェンス扱いすると、
    /// 以降の見出しが丸ごと本文へ吸い込まれて検査が空洞化する。
    #[test]
    fn an_indented_run_is_not_a_fence() {
        let markdown = "## A\n\n機械化不能: 理由\n\n    ```\n\n## B\n\n本文\n";
        assert_eq!(messages(markdown).len(), 1);
    }

    /// 節名は見出し接頭辞を 1 度だけ剥がす。繰り返し剥がすと節名そのものが変わる。
    #[test]
    fn heading_title_strips_the_prefix_once() {
        assert_eq!(heading_title("## ## A"), "## A");
        assert_eq!(heading_title("## A"), "A");
    }

    #[test]
    fn reports_the_heading_line_number() {
        let violations = check_markdown(FILE, "# T\n\n## A\n\n本文\n");
        assert_eq!(violations[0].line, 3);
    }

    #[test]
    fn reports_every_offending_section() {
        let markdown = "## A\n\n本文\n\n## B\n\n機械化: x\n\n## C\n\n本文\n";
        assert_eq!(messages(markdown).len(), 2);
    }

    /// 派生プロジェクトには本ファイルが無い。存在しないことは違反ではない。
    #[test]
    fn passes_when_the_conventions_file_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(check(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn reads_the_conventions_file_from_the_docs_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONVENTIONS_FILE), "## A\n\n本文\n").unwrap();
        assert_eq!(check(dir.path()).unwrap().len(), 1);
    }
}
