//! todo-routing check — `docs/todo.md` の preamble と whole-tree review facet が語る
//! 「番号付き詳細ファイルの集合」が、実在する `docs/todo<数字>.md` と一致するかを
//! 検証する (順位 445)。
//!
//! # 由来
//!
//! 2026-08-13 の PR #395。preamble の新規追加先を更新する一方、facet
//! ([`FACET_REL_PATH`]) には `todo6.md` / `todo2-7.md` という旧世代の固定値が残り、
//! whole-tree review が古い送付先を案内した。**同じ事実が 2 箇所で独立に手入力され、
//! 片方だけが古くなった**。既存の `preamble` 検査は数詞 (「24 つ」) しか見ておらず、
//! 列挙そのものは誰も照合していなかった。
//!
//! # 集合の作り方
//!
//! ここを曖昧にすると誤検出か検査漏れのどちらかが必ず出る。3 点を固定する:
//!
//! - **対象は番号付きの詳細ファイルのみ** — `docs/todo*.md` の素の glob は順位 table
//!   (`todo-summary*.md`) まで拾う。順位 table は詳細エントリの追加先ではないので
//!   [`is_numbered_detail_name`] で番号付きだけに限る。`todo.md` 本体も番号を持たない
//!   ため集合に入らない (列挙する側であって列挙される側ではない)。
//! - **範囲表記は展開してから比較する** — `todo3.md 〜 todo27.md` も `todo3-27.md` も
//!   文字列のまま比べれば常に不一致になる。展開して要素の集合へ落とす。
//! - **退役宣言は参照ごとに効く** — 「todo2.md は … 退役」と書かれた参照は列挙から外し、
//!   代わりに**実在しないこと**を要求する。行単位で退役を判定すると、
//!   `(todo.md / todo3-27.md / … 。todo2.md は 退役)` のように 1 行へ同居した記述で
//!   範囲側まで退役扱いになり、実ファイル全部が「退役なのに実在する」になる。
//!   判定範囲は**その参照から次の参照の直前まで**。

use crate::docs_files::list_docs_files;
use crate::preamble::is_preamble_boundary;
use crate::Violation;
use regex::{Captures, Regex};
use std::collections::BTreeSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// routing 契約を持つ preamble のファイル名。
const PREAMBLE_FILE: &str = "todo.md";

/// facet instruction のリポジトリ root からの相対パス。
const FACET_REL_PATH: &str = ".takt/facets/instructions/review-todo-whole.md";

/// 退役宣言の語。参照の直後にあれば、その参照は「実在しないこと」の宣言になる。
const RETIRED_MARKER: &str = "退役";

/// 範囲表記の区切り。`-` を入れないのは散文のハイフンを範囲と誤認しないため
/// (圧縮形 `todo3-27.md` は [`Patterns::compact`] が別に拾う)。
const RANGE_SEPARATORS: &[&str] = &["〜", "～", "…", "..."];

/// 展開を許す範囲の幅。誤記 (`todo3.md 〜 todo9999.md`) で巨大な集合を作らない。
const MAX_RANGE_SPAN: u32 = 200;

/// 列挙の出どころ。違反の直し方が出どころで変わる。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// `docs/todo.md` の preamble。区切り (`---` / `###`) までを走査する。
    Preamble,
    /// whole-tree review facet の instruction。全文を走査する。
    Facet,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Preamble => "preamble の列挙",
            Source::Facet => "facet の routing 記述",
        }
    }

    fn fix_hint(self) -> &'static str {
        match self {
            Source::Preamble => "preamble の列挙を実ファイルに合わせる",
            Source::Facet => {
                "facet は corpus 全体を列挙する。個別の例示に番号を書かず `docs/todo*.md` と書く"
            }
        }
    }
}

/// `todo<数字>.md` か。順位 table (`todo-summary*.md`) と `todo.md` 本体は含まない。
pub fn is_numbered_detail_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("todo") else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(".md") else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// 実在する番号付き詳細ファイルの番号集合。
pub fn actual_numbers(docs_dir: &Path) -> Result<BTreeSet<u32>, String> {
    let mut numbers = BTreeSet::new();
    for path in list_docs_files(docs_dir, is_numbered_detail_name)? {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let digits = name
            .strip_prefix("todo")
            .and_then(|rest| rest.strip_suffix(".md"))
            .unwrap_or_default();
        let number = digits
            .parse::<u32>()
            .map_err(|e| format!("{name} の番号を解釈できません: {e}"))?;
        numbers.insert(number);
    }
    Ok(numbers)
}

/// preamble と facet instruction の routing 記述を検査する。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let actual = actual_numbers(docs_dir)?;
    let preamble_path = docs_dir.join(PREAMBLE_FILE);
    let preamble = std::fs::read_to_string(&preamble_path)
        .map_err(|e| format!("{} を読めません: {e}", preamble_path.display()))?;
    let mut violations = check_text(
        &preamble_path.display().to_string(),
        &preamble,
        &actual,
        Source::Preamble,
    );
    if let Some(facet_path) = facet_path(docs_dir) {
        let facet = std::fs::read_to_string(&facet_path)
            .map_err(|e| format!("{} を読めません: {e}", facet_path.display()))?;
        violations.extend(check_text(
            &facet_path.display().to_string(),
            &facet,
            &actual,
            Source::Facet,
        ));
    }
    Ok(violations)
}

/// facet instruction の実パス。無ければ検査しない。
///
/// facet は `docs/` の外に住み、派生リポジトリでは存在しないことがある
/// (`convention_declaration` がファイル不在を許容するのと同じ扱い)。
/// **preamble 側は不在を許容しない** — `docs/todo.md` は本 lint の守備範囲そのもので、
/// 読めなければ検査が黙って素通りする ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
fn facet_path(docs_dir: &Path) -> Option<PathBuf> {
    let root = docs_dir.parent()?;
    let path = root.join(FACET_REL_PATH);
    path.is_file().then_some(path)
}

/// 1 つの routing 記述を検査する (I/O なし)。
pub fn check_text(
    file: &str,
    text: &str,
    actual: &BTreeSet<u32>,
    source: Source,
) -> Vec<Violation> {
    let scan = scan(text, source);
    let declared: BTreeSet<u32> = scan.declared.iter().map(|m| m.number).collect();
    let mut violations: Vec<Violation> = scan
        .problems
        .iter()
        .map(|(line, message)| violation(file, *line, message.clone()))
        .collect();
    violations.extend(unknown_violations(file, &scan, actual, source));
    violations.extend(missing_violation(file, &scan, &declared, actual, source));
    violations.extend(retired_violations(file, &scan, actual));
    violations
}

/// 1 参照とその行番号。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mention {
    number: u32,
    line: usize,
}

/// 走査結果。`declared` は列挙、`retired` は退役宣言。
#[derive(Debug, Default)]
struct Scan {
    declared: Vec<Mention>,
    retired: Vec<Mention>,
    problems: Vec<(usize, String)>,
}

fn scan(text: &str, source: Source) -> Scan {
    let patterns = Patterns::new();
    let mut scan = Scan::default();
    for (line_no, line) in scanned_lines(text, source) {
        let (mentions, problems) = raw_mentions(line, &patterns);
        scan.problems
            .extend(problems.into_iter().map(|p| (line_no, p)));
        classify(line, line_no, &mentions, &mut scan);
    }
    scan
}

/// 走査対象の行 (1 始まり)。preamble は区切りで打ち切る — 本文のエントリは
/// 自分のファイル名を書くので、全文を走査すると列挙と区別がつかない。
fn scanned_lines(text: &str, source: Source) -> Vec<(usize, &str)> {
    let lines = text.lines().enumerate().map(|(i, l)| (i + 1, l));
    match source {
        Source::Preamble => lines
            .take_while(|(_, line)| !is_preamble_boundary(line))
            .collect(),
        Source::Facet => lines.collect(),
    }
}

/// 参照の抽出に使う regex 一式。行ごとに作り直さない。
struct Patterns {
    /// `todo12.md` 形式。`todo-summary2.md` は `todo` の直後が数字でないため当たらない。
    single: Regex,
    /// `todo3-27.md` 形式の圧縮範囲。
    compact: Regex,
}

impl Patterns {
    fn new() -> Self {
        Self {
            single: Regex::new(r"todo(\d+)\.md").expect("単独参照の regex"),
            compact: Regex::new(r"todo(\d+)-(\d+)\.md").expect("圧縮範囲の regex"),
        }
    }
}

/// 行内の 1 参照。圧縮範囲は 1 参照で複数番号を持つ。
#[derive(Debug, Clone)]
struct RawMention {
    span: Range<usize>,
    numbers: Vec<u32>,
    /// 範囲表記の端点に使える単独参照のみ Some。
    endpoint: Option<u32>,
}

fn raw_mentions(line: &str, patterns: &Patterns) -> (Vec<RawMention>, Vec<String>) {
    let mut problems = Vec::new();
    let mut mentions: Vec<RawMention> = Vec::new();
    for caps in patterns.compact.captures_iter(line) {
        let Some((span, start, end)) = two_numbers(&caps) else {
            continue;
        };
        if !has_word_boundaries(line, &span) {
            continue;
        }
        let mut numbers = BTreeSet::new();
        expand_into(start, end, &mut numbers, &mut problems);
        mentions.push(RawMention {
            span,
            numbers: numbers.into_iter().collect(),
            endpoint: None,
        });
    }
    for caps in patterns.single.captures_iter(line) {
        let Some((span, number)) = one_number(&caps) else {
            continue;
        };
        if !has_word_boundaries(line, &span) {
            continue;
        }
        mentions.push(RawMention {
            span,
            numbers: vec![number],
            endpoint: Some(number),
        });
    }
    mentions.sort_by_key(|m| m.span.start);
    expand_between(line, &mut mentions, &mut problems);
    (mentions, problems)
}

/// 参照の前後が語境界か。`todo3.md.bak` のような別ファイル名を `todo3.md` の言及として
/// 数えないため、regex の後ろで境界を見る (regex crate に lookahead が無い)。
///
/// 実ファイル側は [`is_numbered_detail_name`] が `todo3.md.bak` を弾くので、境界を見ないと
/// 「列挙にあるが実在しない」という嘘の違反が出る。
fn has_word_boundaries(line: &str, span: &Range<usize>) -> bool {
    let before_ok = match line[..span.start].chars().next_back() {
        None => true,
        Some(c) => !c.is_ascii_alphanumeric(),
    };
    let mut after = line[span.end..].chars();
    let after_ok = match after.next() {
        None => true,
        Some('.') => !after.next().is_some_and(|c| c.is_ascii_alphanumeric()),
        Some(c) => !c.is_ascii_alphanumeric(),
    };
    before_ok && after_ok
}

fn one_number(caps: &Captures<'_>) -> Option<(Range<usize>, u32)> {
    let span = caps.get(0)?.range();
    let number = caps.get(1)?.as_str().parse().ok()?;
    Some((span, number))
}

fn two_numbers(caps: &Captures<'_>) -> Option<(Range<usize>, u32, u32)> {
    let span = caps.get(0)?.range();
    let start = caps.get(1)?.as_str().parse().ok()?;
    let end = caps.get(2)?.as_str().parse().ok()?;
    Some((span, start, end))
}

/// 隣り合う単独参照の間が範囲表記なら展開する。
fn expand_between(line: &str, mentions: &mut [RawMention], problems: &mut Vec<String>) {
    for i in 0..mentions.len().saturating_sub(1) {
        let (Some(start), Some(end)) = (mentions[i].endpoint, mentions[i + 1].endpoint) else {
            continue;
        };
        let gap = &line[mentions[i].span.end..mentions[i + 1].span.start];
        if !is_range_gap(gap) {
            continue;
        }
        let mut numbers = BTreeSet::new();
        expand_into(start, end, &mut numbers, problems);
        mentions[i].numbers.extend(numbers);
    }
}

/// 2 参照の間が範囲表記か。**区切りとリンク記法だけで埋まっている**ことを要求する。
///
/// 区切りの有無だけで判定すると、散文を挟んだ 2 参照を範囲と誤認する — 実際に
/// preamble の「… todo13.md へ。2026-06-12 PR #204 で PR #185〜#196 era のエントリを
/// todo12.md へ分離」が「todo13.md 〜 todo12.md の逆順範囲」として誤検出された。
fn is_range_gap(gap: &str) -> bool {
    let Some(separator) = RANGE_SEPARATORS.iter().find(|sep| gap.contains(**sep)) else {
        return false;
    };
    let rest = gap
        .replacen(separator, "", 1)
        .replace(GAP_SCAFFOLD_WORD, "");
    rest.chars()
        .all(|c| c.is_whitespace() || GAP_SCAFFOLD_CHARS.contains(c))
}

/// 範囲表記の間に現れてよい記号 (markdown link とコードスパンの骨組み)。
const GAP_SCAFFOLD_CHARS: &str = "()[]`*/|:、,";

/// 骨組みに現れてよい唯一の語。
///
/// 範囲は `[docs/todo3.md](todo3.md) 〜 [docs/todo27.md](todo27.md)` の形で書かれ、
/// 2 参照の間にリンク先のディレクトリ名が挟まる。ここを許さないと実 preamble の
/// 範囲がすべて「散文が挟まっている」と判定され、展開されなくなる。
/// **語を増やすときは、散文の単語を通さないかを必ず確認する** — 通すと
/// [`expand_between`] が散文を挟んだ 2 参照を範囲と誤認する側へ戻る。
const GAP_SCAFFOLD_WORD: &str = "docs";

/// `start` から `end` までを展開する。逆順・広すぎる範囲は展開せず問題として返す。
fn expand_into(start: u32, end: u32, numbers: &mut BTreeSet<u32>, problems: &mut Vec<String>) {
    if end < start {
        problems.push(format!(
            "範囲表記 `todo{start}.md` 〜 `todo{end}.md` の順序が逆です"
        ));
        return;
    }
    if end - start > MAX_RANGE_SPAN {
        problems.push(format!(
            "範囲表記 `todo{start}.md` 〜 `todo{end}.md` が広すぎます (上限 {MAX_RANGE_SPAN} 件)"
        ));
        return;
    }
    numbers.extend(start..=end);
}

/// 各参照を列挙と退役宣言へ振り分ける。判定範囲は次の参照の直前まで。
fn classify(line: &str, line_no: usize, mentions: &[RawMention], scan: &mut Scan) {
    for (i, mention) in mentions.iter().enumerate() {
        let until = mentions
            .get(i + 1)
            .map_or(line.len(), |next| next.span.start);
        let after = &line[mention.span.end..until];
        let bucket = if after.contains(RETIRED_MARKER) {
            &mut scan.retired
        } else {
            &mut scan.declared
        };
        bucket.extend(mention.numbers.iter().map(|number| Mention {
            number: *number,
            line: line_no,
        }));
    }
}

/// 列挙にあるが実在しないファイル。
///
/// `declared` は退役番号を除外する — [`expand_between`] は範囲展開を開始側 mention の
/// `numbers` へまとめるため、範囲の終端より後ろで個別に退役宣言された番号
/// (`todo3.md 〜 todo27.md … todo15.md は退役`) が展開側の `declared` にも混入する。
/// ここで除外しないと、範囲内で正しく退役されたファイルを [`missing_violation`] とは
/// 非対称に「実在しない列挙」と誤検知する。
fn unknown_violations(
    file: &str,
    scan: &Scan,
    actual: &BTreeSet<u32>,
    source: Source,
) -> Vec<Violation> {
    let retired: BTreeSet<u32> = scan.retired.iter().map(|m| m.number).collect();
    let mut seen = BTreeSet::new();
    scan.declared
        .iter()
        .filter(|m| {
            !actual.contains(&m.number) && !retired.contains(&m.number) && seen.insert(m.number)
        })
        .map(|m| {
            violation(
                file,
                m.line,
                format!(
                    "{}に実在しない `todo{}.md` があります。{}",
                    source.label(),
                    m.number,
                    source.fix_hint()
                ),
            )
        })
        .collect()
}

/// 実在するが列挙されていないファイル。集合としての欠落なので 1 件にまとめる。
///
/// 退役と書かれた番号は欠落から除く。実在すれば [`retired_violations`] が
/// 「退役なのに実在する」として正確に指すので、同じ 1 つの事実で 2 件出さない。
fn missing_violation(
    file: &str,
    scan: &Scan,
    declared: &BTreeSet<u32>,
    actual: &BTreeSet<u32>,
    source: Source,
) -> Option<Violation> {
    let mut accounted = declared.clone();
    accounted.extend(scan.retired.iter().map(|m| m.number));
    let missing: Vec<String> = actual
        .difference(&accounted)
        .map(|n| format!("todo{n}.md"))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let line = scan.declared.first().map_or(1, |m| m.line);
    Some(violation(
        file,
        line,
        format!(
            "{}に挙がっていない詳細ファイルがあります: {}。{}",
            source.label(),
            missing.join(", "),
            source.fix_hint()
        ),
    ))
}

/// 退役と書かれたのに実在するファイル。
fn retired_violations(file: &str, scan: &Scan, actual: &BTreeSet<u32>) -> Vec<Violation> {
    let mut seen = BTreeSet::new();
    scan.retired
        .iter()
        .filter(|m| actual.contains(&m.number) && seen.insert(m.number))
        .map(|m| {
            violation(
                file,
                m.line,
                format!(
                    "`todo{}.md` を退役と書いていますが実在します。退役記述を消すか、ファイルを畳む",
                    m.number
                ),
            )
        })
        .collect()
}

fn violation(file: &str, line: usize, message: String) -> Violation {
    Violation {
        file: file.to_string(),
        line,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "docs/todo.md";

    fn numbers(values: &[u32]) -> BTreeSet<u32> {
        values.iter().copied().collect()
    }

    fn messages(text: &str, actual: &BTreeSet<u32>, source: Source) -> Vec<String> {
        check_text(FILE, text, actual, source)
            .into_iter()
            .map(|v| v.message)
            .collect()
    }

    /// 実運用の preamble と同じ形 (範囲 + 個別 + 退役) が通ること。
    #[test]
    fn accepts_an_enumeration_that_matches_the_actual_files() {
        let text = "> **本ファイル + [docs/todo3.md](todo3.md) 〜 [docs/todo5.md](todo5.md) \
                    の使い分け** (todo2.md は 2026-08-12 退役)\n\
                    > - **docs/todo4.md**: 編集専用\n";
        assert!(check_text(FILE, text, &numbers(&[3, 4, 5]), Source::Preamble).is_empty());
    }

    /// 範囲表記を展開しないと「3 と 5 だけ宣言」に見え、4 を欠落として誤検出する。
    /// 欠落として出るのは実ファイルのうち範囲外の 6 だけであることを固定する。
    #[test]
    fn expands_a_spelled_range_before_comparing() {
        let found = messages(
            "> todo3.md 〜 todo5.md の使い分け\n",
            &numbers(&[3, 4, 5, 6]),
            Source::Preamble,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("todo6.md"), "{}", found[0]);
        assert!(!found[0].contains("todo4.md"), "{}", found[0]);
    }

    /// 圧縮形 `todo3-27.md` も展開する。
    #[test]
    fn expands_the_compact_range_form() {
        let text = "> 全 todo ファイルを確認すること (todo.md / todo3-6.md)\n";
        assert!(check_text(FILE, text, &numbers(&[3, 4, 5, 6]), Source::Preamble).is_empty());
    }

    /// 順位 table は詳細エントリの追加先ではない。`todo-summary2.md` を番号 2 と
    /// 読んでしまうと、実ファイル集合に存在しない番号が紛れ込む。
    #[test]
    fn a_summary_file_is_not_a_numbered_detail_file() {
        assert!(is_numbered_detail_name("todo3.md"));
        assert!(!is_numbered_detail_name("todo-summary2.md"));
        assert!(!is_numbered_detail_name("todo.md"));
        assert!(!is_numbered_detail_name("todo3.md.bak"));
    }

    /// 実ディレクトリからの集合づくりでも順位 table を数えない。
    #[test]
    fn the_actual_set_excludes_summary_and_unnumbered_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for name in ["todo.md", "todo3.md", "todo-summary2.md", "notes.md"] {
            std::fs::write(tmp.path().join(name), "").expect("write");
        }
        assert_eq!(actual_numbers(tmp.path()).unwrap(), numbers(&[3]));
    }

    /// `todo3.md.bak` は別ファイル名であって `todo3.md` の言及ではない。
    /// 実ファイル側も同じ理由で弾いているので、ここで数えると嘘の違反が出る。
    #[test]
    fn a_longer_filename_is_not_a_mention() {
        let found = messages(
            "> `todo3.md.bak` は対象外\n",
            &numbers(&[3]),
            Source::Preamble,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("挙がっていない"), "{}", found[0]);
        assert!(!found[0].contains("実在しない"), "{}", found[0]);
    }

    /// 圧縮範囲にも語境界が要る。`mytodo3-6.md` / `todo3-6.md.bak` を範囲として
    /// 展開すると、実在しない番号を「列挙にある」と誤検出する (CodeRabbit #500)。
    #[test]
    fn a_compact_range_also_needs_word_boundaries() {
        for text in ["> `mytodo3-6.md` は別物\n", "> `todo3-6.md.bak` は別物\n"] {
            let found = messages(text, &numbers(&[3]), Source::Preamble);
            assert!(
                found.iter().all(|m| !m.contains("実在しない")),
                "範囲として展開しないこと: {text} -> {found:?}"
            );
        }
    }

    /// 文末のピリオドは拡張子の続きではない。facet instruction は英文なので、
    /// `.` を一律に境界違反とすると英文中の参照を丸ごと見落とす。
    #[test]
    fn a_trailing_period_still_counts_as_a_mention() {
        let text = "The corpus runs from todo3.md to todo4.md.\n";
        assert!(check_text(FILE, text, &numbers(&[3, 4]), Source::Facet).is_empty());
    }

    #[test]
    fn flags_an_enumerated_file_that_does_not_exist() {
        let found = messages("> todo3.md / todo9.md\n", &numbers(&[3]), Source::Preamble);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("実在しない `todo9.md`"), "{}", found[0]);
    }

    /// 退役の判定は次の参照の直前で切れる。範囲と退役宣言が 1 行に同居する
    /// 実運用の記述 (「todo3-27.md / … 。todo2.md は 退役」) で範囲側を巻き込まない。
    #[test]
    fn a_retirement_marker_stops_at_the_next_reference() {
        let text = "> (todo.md / todo3-6.md / todo-summary.md。todo2.md は 2026-08-12 退役)\n";
        assert!(check_text(FILE, text, &numbers(&[3, 4, 5, 6]), Source::Preamble).is_empty());
    }

    /// 範囲展開は開始側 mention の `numbers` へまとめられるため、範囲の終端より後ろで
    /// 個別に退役宣言された番号も展開側の `declared` に混入する。この混入を
    /// `unknown_violations` が `scan.retired` で除外しないと、範囲内で正しく退役された
    /// ファイルを「実在しない列挙」と誤検知する (SIM-NEW-todo_routing-L358)。
    #[test]
    fn a_retired_file_inside_a_range_is_not_flagged_as_unknown() {
        let text = "> todo3.md 〜 todo27.md の使い分け (todo15.md は退役)\n";
        let actual: BTreeSet<u32> = (3..=27).filter(|n| *n != 15).collect();
        assert!(check_text(FILE, text, &actual, Source::Preamble).is_empty());
    }

    #[test]
    fn flags_a_retired_file_that_still_exists() {
        let found = messages(
            "> todo3.md / (todo2.md は 退役)\n",
            &numbers(&[2, 3]),
            Source::Preamble,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("退役と書いていますが実在します"),
            "{}",
            found[0]
        );
    }

    /// PR #395 の実害そのもの: facet の上限だけが旧世代のまま取り残された形。
    #[test]
    fn the_facet_must_enumerate_the_whole_corpus() {
        let text = "the planning corpus (`docs/todo.md` + `docs/todo2.md` … `docs/todo4.md`)\n";
        let found = messages(text, &numbers(&[3, 4, 5, 6]), Source::Facet);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found.iter().any(|m| m.contains("実在しない `todo2.md`")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|m| m.contains("todo5.md, todo6.md")),
            "{found:?}"
        );
    }

    /// preamble の走査は区切りで止める。本文のエントリは自分のファイル名を書くため、
    /// 全文を走査すると列挙と区別がつかない。
    #[test]
    fn the_preamble_scan_stops_at_the_boundary() {
        let text = "> todo3.md の使い分け\n\n---\n\n### 順位 1\n\n本文で todo9.md に触れる\n";
        assert!(check_text(FILE, text, &numbers(&[3]), Source::Preamble).is_empty());
    }

    /// facet は全文を走査する (区切りで打ち切らない)。
    #[test]
    fn the_facet_scan_covers_the_whole_file() {
        let text = "corpus\n\n---\n\nAvoid `docs/todo9.md`\n";
        let found = messages(text, &numbers(&[3]), Source::Facet);
        assert!(found.iter().any(|m| m.contains("todo9.md")), "{found:?}");
    }

    /// 散文を挟んだ 2 参照を範囲と読まない。実 preamble の todo10.md の節が
    /// 「todo13.md へ。… PR #185〜#196 era のエントリを todo12.md へ分離」と書いており、
    /// 区切りの有無だけで判定した初版は逆順範囲として誤検出した。
    #[test]
    fn prose_between_two_references_is_not_a_range() {
        let text = "> - **docs/todo10.md**: 新規エントリは todo13.md へ。\
                    2026-06-12 PR #204 で PR #185〜#196 era のエントリを todo12.md へ分離\n";
        assert!(check_text(FILE, text, &numbers(&[10, 12, 13]), Source::Preamble).is_empty());
    }

    #[test]
    fn flags_a_reversed_range() {
        let found = messages(
            "> todo27.md 〜 todo3.md\n",
            &numbers(&[3]),
            Source::Preamble,
        );
        assert!(found.iter().any(|m| m.contains("順序が逆")), "{found:?}");
    }

    /// 誤記した範囲を展開して巨大な集合を作らない。
    #[test]
    fn flags_an_overly_wide_range() {
        let found = messages(
            "> todo3.md 〜 todo9999.md\n",
            &numbers(&[3]),
            Source::Preamble,
        );
        assert!(found.iter().any(|m| m.contains("広すぎます")), "{found:?}");
        assert!(
            !found.iter().any(|m| m.contains("todo500.md")),
            "展開していないこと: {found:?}"
        );
    }

    /// facet は `docs/` の外に住むため、不在は検査 skip (違反にしない)。
    #[test]
    fn skips_when_the_facet_file_is_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let docs = tmp.path().join("docs");
        std::fs::create_dir(&docs).expect("mkdir");
        std::fs::write(docs.join("todo.md"), "> todo3.md\n").expect("write");
        std::fs::write(docs.join("todo3.md"), "").expect("write");
        assert!(check(&docs).unwrap().is_empty());
    }

    /// preamble が読めない場合は Err。空 Vec で素通りさせない (ADR-043)。
    #[test]
    fn an_unreadable_preamble_is_an_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let docs = tmp.path().join("docs");
        std::fs::create_dir(&docs).expect("mkdir");
        let err = check(&docs).unwrap_err();
        assert!(err.contains("todo.md"), "{err}");
    }

    /// facet の routing 記述が実ファイル集合と一致していれば通る。
    #[test]
    fn reads_the_facet_when_it_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let docs = tmp.path().join("docs");
        let facet_dir = tmp.path().join(".takt/facets/instructions");
        std::fs::create_dir_all(&docs).expect("mkdir docs");
        std::fs::create_dir_all(&facet_dir).expect("mkdir facet");
        std::fs::write(docs.join("todo.md"), "> todo3.md 〜 todo4.md\n").expect("write");
        for name in ["todo3.md", "todo4.md"] {
            std::fs::write(docs.join(name), "").expect("write");
        }
        let facet = facet_dir.join("review-todo-whole.md");
        std::fs::write(&facet, "corpus (`docs/todo3.md` … `docs/todo9.md`)\n").expect("write");
        let found = check(&docs).unwrap();
        assert!(
            found.iter().any(|v| v.file.contains("review-todo-whole")),
            "{found:?}"
        );
    }
}
