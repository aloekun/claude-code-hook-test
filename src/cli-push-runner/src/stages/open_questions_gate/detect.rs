//! `docs/open-questions.md` から未解決の問いを読む純粋層 (I/O なし)。
//!
//! # 何を読むか
//!
//! `## Q-<連番>: <問い>` の見出しだけを問いとして数える。見出し配下の `関連:` / `仮定:` は
//! 表示のために拾うが、**欠けていてもエントリとしては数える** — 書式の不備を理由に push を
//! 通すと、gate が守ろうとしている「書かれた問いは必ず push 前に届く」が崩れるためである
//! ([ADR-077](../../../../docs/adr/adr-077-open-questions-gate.md))。
//!
//! # コードブロックは読まない
//!
//! 本ファイル自身が「書き方」の節で `## Q-1: ...` の例を示すため、fence (```) の内側は
//! 読み飛ばす。**dogfood で実測して分かった** — 例を問いとして数えると、書き方を説明した
//! 時点で gate が常に発火し、機構ごと無効化される動機になる。
//!
//! # 見出しレベルを `##` に固定する理由
//!
//! 説明用の `##` 節 (「書き方」等) と問いを区別する必要がある。`Q-` 前置を鍵にすることで、
//! 説明文をいくら足しても誤検出しない。`### 順位 N:` を鍵にした
//! [`lib-ledger`](../../../../src/lib-ledger/src/removal.rs) の `heading_rank` と同じ流儀。

/// 未解決の問い 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenQuestion {
    /// `Q-` に続く識別子 (例 `1`)。
    pub(crate) id: String,
    /// 見出しのコロン以降。
    pub(crate) question: String,
    /// `関連:` の値。無ければ `None`。
    pub(crate) related: Option<String>,
    /// `仮定:` の値。無ければ `None`。
    pub(crate) assumption: Option<String>,
}

const HEADING_PREFIX: &str = "## Q-";
const RELATED_PREFIX: &str = "関連:";
const ASSUMPTION_PREFIX: &str = "仮定:";

/// `docs/open-questions.md` の内容から未解決の問いを取り出す (I/O なし)。
pub(crate) fn open_questions(content: &str) -> Vec<OpenQuestion> {
    let mut out: Vec<OpenQuestion> = Vec::new();
    let mut current: Option<usize> = None;
    let mut in_fence = false;
    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(heading) = parse_heading(trimmed) {
            out.push(heading);
            current = Some(out.len() - 1);
            continue;
        }
        if is_heading(trimmed) {
            current = None;
            continue;
        }
        let Some(question) = current.and_then(|i| out.get_mut(i)) else {
            continue;
        };
        if let Some(value) = field_value(trimmed, RELATED_PREFIX) {
            question.related.get_or_insert(value);
        } else if let Some(value) = field_value(trimmed, ASSUMPTION_PREFIX) {
            question.assumption.get_or_insert(value);
        }
    }
    out
}

/// 通常の markdown 見出しか (`# ` 以降のレベルすべて)。
///
/// **問いでない見出しはフィールド収集を終わらせる** — `## 解消済み` のような節の下に
/// 書かれた `関連:` / `仮定:` が、直前の問いへ吸着して誤表示になる (CodeRabbit #463)。
fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#') && trimmed.trim_start_matches('#').starts_with(' ')
}

/// `## Q-<id>: <問い>` を読む。コロンが無い行は見出しとして数えない。
fn parse_heading(line: &str) -> Option<OpenQuestion> {
    let rest = line.strip_prefix(HEADING_PREFIX)?;
    let (id, question) = rest.split_once(':')?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    Some(OpenQuestion {
        id: id.to_string(),
        question: question.trim().to_string(),
        related: None,
        assumption: None,
    })
}

/// `関連:` / `仮定:` の値を読む。行頭の `-` や引用符は許容する。
fn field_value(line: &str, prefix: &str) -> Option<String> {
    let body = line
        .trim_start()
        .trim_start_matches(['-', '*', '>'])
        .trim_start();
    let value = body.strip_prefix(prefix)?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_QUESTION: &str = "# 未解決の問い\n\n\
        ## 書き方\n\n\
        説明の節。ここは問いではない。\n\n\
        ## Q-1: この判定の比較対象は何か\n\n\
        関連: src/cli-pr-monitor/src/stages/monitor.rs\n\
        仮定: 助言層なので fail-open (判定不能なら警告を出さない)\n";

    #[test]
    fn an_empty_document_has_no_questions() {
        assert!(open_questions("").is_empty());
        assert!(open_questions("# 未解決の問い\n\n(現在なし)\n").is_empty());
    }

    /// 説明用の `##` 節を問いと誤認しない。
    #[test]
    fn explanatory_sections_are_not_questions() {
        let questions = open_questions(ONE_QUESTION);
        assert_eq!(questions.len(), 1, "{questions:?}");
        assert_eq!(questions[0].id, "1");
        assert_eq!(questions[0].question, "この判定の比較対象は何か");
    }

    #[test]
    fn related_and_assumption_are_collected() {
        let questions = open_questions(ONE_QUESTION);
        assert_eq!(
            questions[0].related.as_deref(),
            Some("src/cli-pr-monitor/src/stages/monitor.rs")
        );
        assert_eq!(
            questions[0].assumption.as_deref(),
            Some("助言層なので fail-open (判定不能なら警告を出さない)")
        );
    }

    /// **書式が欠けていてもエントリとして数える。** 不備を理由に push を通すと、
    /// gate が守ろうとしている性質が崩れる。
    #[test]
    fn a_question_without_fields_still_counts() {
        let questions = open_questions("## Q-2: 書式が欠けた問い\n");
        assert_eq!(questions.len(), 1);
        assert!(questions[0].related.is_none());
        assert!(questions[0].assumption.is_none());
    }

    /// 複数の問いは、それぞれの見出し配下の値を取る。
    #[test]
    fn fields_belong_to_the_preceding_heading() {
        let content = "## Q-1: 一つ目\n\n関連: a.rs\n仮定: A\n\n## Q-2: 二つ目\n\n関連: b.rs\n仮定: B\n";
        let questions = open_questions(content);
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].related.as_deref(), Some("a.rs"));
        assert_eq!(questions[1].related.as_deref(), Some("b.rs"));
        assert_eq!(questions[1].assumption.as_deref(), Some("B"));
    }

    /// 見出しより前に現れた `関連:` は、どの問いにも属さない。
    #[test]
    fn fields_before_any_heading_are_ignored() {
        let questions = open_questions("関連: orphan.rs\n\n## Q-1: 問い\n");
        assert_eq!(questions.len(), 1);
        assert!(questions[0].related.is_none());
    }

    /// 同じ項目が 2 回書かれたら先勝ち (後続の説明文で上書きしない)。
    #[test]
    fn the_first_value_wins_for_duplicated_fields() {
        let questions = open_questions("## Q-1: 問い\n\n仮定: 最初\n仮定: あとから書いた説明\n");
        assert_eq!(questions[0].assumption.as_deref(), Some("最初"));
    }

    /// リストや引用の中に書かれていても読む (書式の揺れで見落とさない)。
    #[test]
    fn list_and_quote_markers_are_tolerated() {
        let questions = open_questions("## Q-1: 問い\n\n- 関連: a.rs\n> 仮定: B\n");
        assert_eq!(questions[0].related.as_deref(), Some("a.rs"));
        assert_eq!(questions[0].assumption.as_deref(), Some("B"));
    }

    /// コロンの無い見出しは問いとして数えない (`## Q-` で始まる説明節への保険)。
    #[test]
    fn a_heading_without_a_colon_is_not_a_question() {
        assert!(open_questions("## Q- についての説明\n").is_empty());
    }

    /// 連番は数字に限らない (`Q-490a` のような枝番も通す)。
    #[test]
    fn identifiers_are_not_restricted_to_digits() {
        let questions = open_questions("## Q-490a: 枝番の問い\n");
        assert_eq!(questions[0].id, "490a");
    }

    /// **コードブロック内の例を問いと数えない** (dogfood で実測した誤検出)。
    ///
    /// `docs/open-questions.md` 自身が「書き方」の節でエントリの例を示すため、
    /// fence を読み飛ばさないと、書き方を説明した時点で gate が常に発火する。
    #[test]
    fn examples_inside_a_fence_are_not_questions() {
        let content = "## 書き方\n\n\
            ```markdown\n\
            ## Q-1: この判定の比較対象は何か\n\n\
            関連: src/cli-pr-monitor/src/stages/monitor.rs\n\
            仮定: 助言層なので fail-open\n\
            ```\n\n\
            ## 未解決の問い\n\n(現在なし)\n";
        assert!(open_questions(content).is_empty(), "{:?}", open_questions(content));
    }

    /// fence の外にある本物の問いは、例の後でも読む。
    #[test]
    fn a_real_question_after_a_fence_is_still_read() {
        let content = "```markdown\n## Q-1: 例\n```\n\n## Q-2: 本物\n\n仮定: A\n";
        let questions = open_questions(content);
        assert_eq!(questions.len(), 1, "{questions:?}");
        assert_eq!(questions[0].id, "2");
    }

    /// fence 内の `関連:` / `仮定:` を直前の問いへ混ぜない。
    #[test]
    fn fields_inside_a_fence_do_not_attach_to_the_previous_question() {
        let content = "## Q-1: 本物\n\n仮定: 本物の仮定\n\n```markdown\n関連: example.rs\n```\n";
        let questions = open_questions(content);
        assert_eq!(questions[0].assumption.as_deref(), Some("本物の仮定"));
        assert!(questions[0].related.is_none(), "{questions:?}");
    }

    /// **通常見出しはフィールド収集を終わらせる** (CodeRabbit #463)。
    ///
    /// `## 解消済み` のような節の下に書かれた `関連:` / `仮定:` が直前の問いへ吸着すると、
    /// deny 時の表示が実際の問いと食い違う。
    #[test]
    fn an_ordinary_heading_ends_field_collection() {
        let content = "## Q-1: 本物の問い\n\n仮定: 本物の仮定\n\n\
            ## 解消済み\n\n関連: resolved.rs\n仮定: 別の節の仮定\n";
        let questions = open_questions(content);
        assert_eq!(questions.len(), 1, "{questions:?}");
        assert_eq!(questions[0].assumption.as_deref(), Some("本物の仮定"));
        assert!(questions[0].related.is_none(), "{questions:?}");
    }

    /// 見出しレベルが違っても同じ (`### 補足` の下も吸着させない)。
    #[test]
    fn any_heading_level_ends_field_collection() {
        let content = "## Q-1: 問い\n\n### 補足\n\n関連: note.rs\n";
        let questions = open_questions(content);
        assert!(questions[0].related.is_none(), "{questions:?}");
    }

    /// 通常見出しの後に来た `## Q-` は、新しい問いとして拾い直す。
    #[test]
    fn a_question_after_an_ordinary_heading_is_collected_again() {
        let content = "## Q-1: 一つ目\n\n## 区切り\n\n## Q-2: 二つ目\n\n仮定: B\n";
        let questions = open_questions(content);
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[1].assumption.as_deref(), Some("B"));
        assert!(questions[0].assumption.is_none());
    }
}
