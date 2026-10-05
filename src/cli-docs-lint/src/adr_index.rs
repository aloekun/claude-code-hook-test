//! adr-index check — ADR の採番・CLAUDE.md の索引・ステータスタグの整合を検査する
//! (順位 272 / 357)。
//!
//! # なぜ要るか
//!
//! - **採番の衝突**: PR #261 と PR #260 が並行して ADR-052 を起草し、rebase 時にファイル名・
//!   本文タイトル・参照 10 箇所以上の置換が要った。衝突は land するまで誰も検出していなかった
//! - **索引タグの乖離**: ADR-047 の本体は「却下」に確定したのに、CLAUDE.md の索引は
//!   `*(試験運用)*` のまま残った (PR #340 で発見)。導入時の実測では ADR-038 (本体は「採用」) も
//!   同じ状態だった
//!
//! # 何を ADR とみなすか
//!
//! `docs/adr/adr-NNN-*.md` のうち、H1 が `# ADR-NNN:` (コロン付き) で始まるもの。
//! 同じ番号の付属資料 (`adr-056-step-timings.md` / `adr-072-verification-log.md`) は H1 が
//! この形ではないので ADR として数えない。ただし**付属資料には同じ番号の ADR 本体が要る** —
//! H1 を書き損じた ADR が付属資料として扱われ、検査を素通りするのを防ぐため。
//!
//! # ステータスの比較
//!
//! 本体の `## ステータス` (旧形式は `## Status`) の最初の行の先頭語と、索引行のタグを
//! それぞれ [`Status`] に分類して比べる。タグなしと `*(Supersedes ...)*` (補足の注記) は
//! 採用系とみなす。本体の先頭語が既知の語でなければ違反にする — 読めないステータスを
//! 「一致」として通さない ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
//!
//! CLAUDE.md が無いリポジトリ (派生プロジェクト) では索引の検査をしない。`docs/adr/` が
//! 無ければ何も検査しない。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::docs_files::list_docs_files;
use crate::Violation;

const INDEX_FILE: &str = "CLAUDE.md";
const ADR_DIR: &str = "adr";
const INDEX_LINK_PREFIX: &str = "docs/adr/";

/// ADR の状態を、索引タグと本体のステータスで共通の 4 区分に寄せたもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 承認済み / 採用 / 本採用 / Accepted、および索引のタグなし。
    Adopted,
    Trial,
    Rejected,
    Superseded,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Adopted => "採用系 (承認済み / 採用 / 本採用 / Accepted)",
            Status::Trial => "試験運用",
            Status::Rejected => "却下",
            Status::Superseded => "Superseded",
        }
    }
}

/// 本体のステータス行の先頭語を分類する。未知の語は `None`。
fn body_status(line: &str) -> Option<Status> {
    let word: String = line
        .trim_start_matches(|c: char| c == '*' || c.is_whitespace())
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '(' && *c != '（' && *c != '*')
        .collect();
    match word.as_str() {
        "承認済み" | "採用" | "本採用" | "Accepted" => Some(Status::Adopted),
        "試験運用" => Some(Status::Trial),
        "却下" => Some(Status::Rejected),
        "Superseded" => Some(Status::Superseded),
        _ => None,
    }
}

/// 索引行のリンクより後ろ (タグ部分) を分類する。
fn index_status(tail: &str) -> Status {
    let Some(tag) = tail
        .split_once("*(")
        .and_then(|(_, rest)| rest.split_once(")*"))
        .map(|(tag, _)| tag.trim())
    else {
        return Status::Adopted;
    };
    if tag.starts_with("Superseded") {
        Status::Superseded
    } else if tag.starts_with("試験運用") {
        Status::Trial
    } else if tag.starts_with("却下") {
        Status::Rejected
    } else {
        Status::Adopted
    }
}

/// `docs/adr/` の 1 ファイル (I/O 済み)。
pub struct AdrFile {
    pub name: String,
    pub content: String,
}

/// ファイル名の番号 (`adr-056-step-timings.md` → 56)。
fn file_number(name: &str) -> Option<u32> {
    name.strip_prefix("adr-")?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// H1 が `# ADR-NNN:` (番号の後ろに `(仮)` のような括弧書きを 1 つ挟んでもよい) なら
/// その番号。付属資料の `# ADR-072 検証記録 — ...` はコロンが無いので `None`。
fn heading_number(content: &str) -> Option<u32> {
    let rest = content.lines().next()?.strip_prefix("# ADR-")?;
    let digits_len = rest.chars().take_while(char::is_ascii_digit).count();
    let number = rest[..digits_len].parse().ok()?;
    let after = rest[digits_len..].trim_start();
    let after = match after.strip_prefix('(').or_else(|| after.strip_prefix('（')) {
        Some(inner) => inner.split_once([')', '）'])?.1.trim_start(),
        None => after,
    };
    after.starts_with(':').then_some(number)
}

/// `## ステータス` / `## Status` の直後の最初の非空行と、その行番号 (1 始まり)。
fn status_line(content: &str) -> Option<(usize, &str)> {
    let mut lines = content.lines().enumerate();
    lines.find(|(_, l)| matches!(l.trim(), "## ステータス" | "## Status"))?;
    lines.find(|(_, l)| !l.trim().is_empty()).map(|(i, l)| (i + 1, l))
}

/// 索引の 1 行 (`- [ADR-NNN: ...](docs/adr/<file>)<tail>`)。
struct IndexEntry<'a> {
    line: usize,
    label_number: u32,
    file: &'a str,
    tail: &'a str,
}

fn index_entries(index: &str) -> Vec<IndexEntry<'_>> {
    index
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let rest = line.strip_prefix("- [ADR-")?;
            let (digits, rest) = rest.split_once(':')?;
            let (_, rest) = rest.split_once("](")?;
            let (target, tail) = rest.split_once(')')?;
            Some(IndexEntry {
                line: i + 1,
                label_number: digits.parse().ok()?,
                file: target.strip_prefix(INDEX_LINK_PREFIX)?,
                tail,
            })
        })
        .collect()
}

/// 番号 → その番号を H1 に持つ ADR ファイル群。
type AdrsByNumber<'a> = BTreeMap<u32, Vec<&'a AdrFile>>;

/// 全検査の本体 (I/O なし)。`index` は CLAUDE.md の (表示名, 内容)。
pub fn check_text(adr_dir: &str, files: &[AdrFile], index: Option<(&str, &str)>) -> Vec<Violation> {
    let at = |name: &str, line: usize, message: String| Violation {
        file: format!("{adr_dir}/{name}"),
        line,
        message,
    };
    let (adrs, mut violations) = check_numbering(files, &at);
    if let Some((index_name, index_text)) = index {
        violations.extend(check_index(files, &adrs, index_name, index_text, &at));
    }
    violations
}

/// 272: H1 とファイル名の番号一致 / 重複採番 / 付属資料の持ち主。
fn check_numbering<'a>(
    files: &'a [AdrFile],
    at: &impl Fn(&str, usize, String) -> Violation,
) -> (AdrsByNumber<'a>, Vec<Violation>) {
    let mut violations = Vec::new();
    let mut adrs: AdrsByNumber = BTreeMap::new();
    let mut companions = Vec::new();
    for file in files {
        match heading_number(&file.content) {
            Some(n) => {
                if file_number(&file.name) != Some(n) {
                    violations.push(at(
                        &file.name,
                        1,
                        format!("ファイル名の番号と H1 の番号 (ADR-{n:03}) が一致しません"),
                    ));
                }
                adrs.entry(n).or_default().push(file);
            }
            None => companions.push(file),
        }
    }
    for (n, same) in &adrs {
        if same.len() > 1 {
            let names: Vec<&str> = same.iter().map(|f| f.name.as_str()).collect();
            violations.push(at(
                &same[1].name,
                1,
                format!("ADR-{n:03} が重複して採番されています: {}", names.join(" / ")),
            ));
        }
    }
    for file in companions {
        let has_owner = file_number(&file.name).is_some_and(|n| adrs.contains_key(&n));
        if !has_owner {
            violations.push(at(
                &file.name,
                1,
                "H1 が `# ADR-NNN:` ではなく、同じ番号の ADR 本体もありません (H1 の書き損じか、付属資料の置き場所の誤り)".to_string(),
            ));
        }
    }
    (adrs, violations)
}

/// 272 + 357: 索引のリンク先・ラベル番号・掲載漏れと、ステータスタグの一致。
fn check_index(
    files: &[AdrFile],
    adrs: &AdrsByNumber,
    index_name: &str,
    index_text: &str,
    at: &impl Fn(&str, usize, String) -> Violation,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let by_name: BTreeMap<&str, &AdrFile> = files.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut indexed = BTreeSet::new();
    for entry in index_entries(index_text) {
        let in_index = |message: String| Violation {
            file: index_name.to_string(),
            line: entry.line,
            message,
        };
        let Some(file) = by_name.get(entry.file) else {
            violations.push(in_index(format!("索引のリンク先 {INDEX_LINK_PREFIX}{} がありません", entry.file)));
            continue;
        };
        let Some(n) = heading_number(&file.content) else {
            violations.push(in_index(format!("索引のリンク先 {} は ADR ではありません (H1 が `# ADR-NNN:` でない)", entry.file)));
            continue;
        };
        indexed.insert(n);
        if entry.label_number != n {
            violations.push(in_index(format!(
                "索引ラベルの番号 (ADR-{:03}) とリンク先の番号 (ADR-{n:03}) が一致しません",
                entry.label_number
            )));
        }
        match status_mismatch(file, n, &entry, at) {
            Some(Mismatch::Body(v)) => violations.push(v),
            Some(Mismatch::Index(message)) => violations.push(in_index(message)),
            None => {}
        }
    }
    for (n, same) in adrs {
        if !indexed.contains(n) {
            violations.push(at(&same[0].name, 1, format!("ADR-{n:03} が {index_name} の索引にありません")));
        }
    }
    violations
}

enum Mismatch {
    /// ADR 本体の側の問題 (ステータス節が無い / 先頭語を読めない)。
    Body(Violation),
    /// 索引タグと本体のステータスの不一致 (索引行に報告する)。
    Index(String),
}

/// 357: 索引タグと本体のステータスを比べる。
fn status_mismatch(
    file: &AdrFile,
    n: u32,
    entry: &IndexEntry,
    at: &impl Fn(&str, usize, String) -> Violation,
) -> Option<Mismatch> {
    let Some((status_at, line)) = status_line(&file.content) else {
        let message = "`## ステータス` (または `## Status`) の節がありません".to_string();
        return Some(Mismatch::Body(at(&file.name, 1, message)));
    };
    let Some(body) = body_status(line) else {
        let message = format!("ステータスの先頭語を読めません: {}", line.trim());
        return Some(Mismatch::Body(at(&file.name, status_at, message)));
    };
    let tag = index_status(entry.tail);
    (tag != body).then(|| {
        Mismatch::Index(format!(
            "ADR-{n:03} の索引タグ ({}) が本体のステータス ({}) と一致しません",
            tag.label(),
            body.label()
        ))
    })
}

/// `docs_dir/adr/` と、`docs_dir` の親にある CLAUDE.md を検査する。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let adr_dir = docs_dir.join(ADR_DIR);
    if !adr_dir.is_dir() {
        return Ok(Vec::new());
    }
    let paths = list_docs_files(&adr_dir, |name| name.starts_with("adr-") && name.ends_with(".md"))?;
    let mut files = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("ファイル名を読めません: {}", path.display()))?
            .to_string();
        let content = fs::read_to_string(&path).map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        files.push(AdrFile { name, content });
    }
    let index_path = docs_dir.parent().map(|root| root.join(INDEX_FILE));
    let index_text = match &index_path {
        Some(p) if p.is_file() => {
            Some(fs::read_to_string(p).map_err(|e| format!("{} を読めません: {e}", p.display()))?)
        }
        _ => None,
    };
    let index_name = index_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    Ok(check_text(
        &adr_dir.display().to_string(),
        &files,
        index_text.as_deref().map(|t| (index_name.as_str(), t)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adr(name: &str, number: u32, status: &str) -> AdrFile {
        AdrFile {
            name: name.to_string(),
            content: format!("# ADR-{number:03}: タイトル\n\n## ステータス\n\n{status}\n\n## コンテキスト\n"),
        }
    }

    fn companion(name: &str) -> AdrFile {
        AdrFile {
            name: name.to_string(),
            content: "# 観測記録\n\n本文\n".to_string(),
        }
    }

    fn index_line(number: u32, file: &str, tag: &str) -> String {
        format!("- [ADR-{number:03}: タイトル](docs/adr/{file}){tag}\n")
    }

    fn run(files: &[AdrFile], index: &str) -> Vec<Violation> {
        check_text("docs/adr", files, Some(("CLAUDE.md", index)))
    }

    fn messages(violations: &[Violation]) -> Vec<&str> {
        violations.iter().map(|v| v.message.as_str()).collect()
    }

    /// 整合している状態は通る (タグなし = 採用系、付属資料は同番号の本体があれば可)。
    #[test]
    fn a_consistent_repository_passes() {
        let files = [
            adr("adr-001-a.md", 1, "Accepted (2026-03-16)"),
            adr("adr-002-b.md", 2, "試験運用 (2026-05-10)"),
            adr("adr-003-c.md", 3, "**却下 (2026-07-19 確定)**"),
            adr("adr-004-d.md", 4, "Superseded by ADR-010"),
            companion("adr-002-observations.md"),
        ];
        let index = [
            index_line(1, "adr-001-a.md", ""),
            index_line(2, "adr-002-b.md", " *(試験運用)*"),
            index_line(3, "adr-003-c.md", " *(却下)*"),
            index_line(4, "adr-004-d.md", " *(Superseded by ADR-010)*"),
            "- ADR-005: *(永久欠番)*\n".to_string(),
        ]
        .concat();
        assert!(run(&files, &index).is_empty(), "{:?}", run(&files, &index));
    }

    /// 272 (a): 同じ番号の ADR が 2 本あれば落とす (PR #260 / #261 の ADR-052 衝突)。
    #[test]
    fn duplicate_numbers_are_detected() {
        let files = [adr("adr-052-a.md", 52, "試験運用"), adr("adr-052-b.md", 52, "試験運用")];
        let found = check_text("docs/adr", &files, None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("ADR-052 が重複"), "{found:?}");
    }

    /// 272 (c): ファイル名と H1 の番号のずれを落とす (rebase で片方だけ直した形)。
    #[test]
    fn file_and_heading_number_mismatch_is_detected() {
        let files = [adr("adr-053-a.md", 52, "試験運用")];
        let found = check_text("docs/adr", &files, None);
        assert_eq!(messages(&found), vec!["ファイル名の番号と H1 の番号 (ADR-052) が一致しません"]);
    }

    /// H1 を書き損じた ADR は、同じ番号の本体が無い付属資料として落ちる。
    #[test]
    fn a_companion_without_its_adr_is_detected() {
        let found = check_text("docs/adr", &[companion("adr-090-x.md")], None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("同じ番号の ADR 本体もありません"), "{found:?}");
    }

    /// 272 (b): 索引のリンク切れ / ラベル番号のずれ / 掲載漏れ。
    #[test]
    fn index_inconsistencies_are_detected() {
        let files = [adr("adr-001-a.md", 1, "承認済み"), adr("adr-002-b.md", 2, "承認済み")];
        let index = [index_line(1, "adr-009-missing.md", ""), index_line(3, "adr-002-b.md", "")].concat();
        let found = run(&files, &index);
        let texts = messages(&found);
        assert_eq!(found.len(), 3, "{texts:?}");
        assert!(texts.iter().any(|m| m.contains("adr-009-missing.md がありません")), "{texts:?}");
        assert!(texts.iter().any(|m| m.contains("索引ラベルの番号 (ADR-003)")), "{texts:?}");
        assert!(texts.iter().any(|m| m.contains("ADR-001 が CLAUDE.md の索引にありません")), "{texts:?}");
    }

    /// 357: 本体が却下なのに索引が試験運用 (ADR-047 の実例) / 本体が採用なのに試験運用 (ADR-038)。
    #[test]
    fn status_tag_mismatches_are_detected() {
        let files = [
            adr("adr-047-a.md", 47, "**却下 (2026-07-19 ユーザー承認により確定)**"),
            adr("adr-038-b.md", 38, "採用 (2026-05-15、Phase E 採否判定で昇格)"),
        ];
        let index = [
            index_line(47, "adr-047-a.md", " *(試験運用)*"),
            index_line(38, "adr-038-b.md", " *(試験運用)*"),
        ]
        .concat();
        let found = run(&files, &index);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|v| v.file == "CLAUDE.md"), "{found:?}");
        assert!(found[0].message.contains("ADR-047 の索引タグ (試験運用) が本体のステータス (却下)"));
    }

    /// `*(Supersedes ...)*` は補足の注記で、ステータスではない (採用系として扱う)。
    #[test]
    fn a_supersedes_annotation_counts_as_adopted() {
        let files = [adr("adr-015-a.md", 15, "承認済み (2026-04-14)")];
        let index = index_line(15, "adr-015-a.md", " *(Supersedes ADR-008 の一部)*");
        assert!(run(&files, &index).is_empty());
    }

    /// 読めないステータスは一致として通さない (fail-closed)。節が無い場合も同じ。
    #[test]
    fn unreadable_or_missing_status_is_rejected() {
        let mut no_section = adr("adr-002-b.md", 2, "");
        no_section.content = "# ADR-002: タイトル\n\n本文\n".to_string();
        let files = [adr("adr-001-a.md", 1, "検討中 (2026-10-01)"), no_section];
        let index = [index_line(1, "adr-001-a.md", ""), index_line(2, "adr-002-b.md", "")].concat();
        let texts = messages(&run(&files, &index)).join("\n");
        assert!(texts.contains("ステータスの先頭語を読めません: 検討中"), "{texts}");
        assert!(texts.contains("節がありません"), "{texts}");
    }

    /// H1 の形: `(仮)` を挟む形は ADR、コロンの無い形 (付属資料) は ADR ではない。
    #[test]
    fn heading_forms_are_classified() {
        assert_eq!(heading_number("# ADR-023 (仮): タイトル\n"), Some(23));
        assert_eq!(heading_number("# ADR-025（仮）: タイトル\n"), Some(25));
        assert_eq!(heading_number("# ADR-001: タイトル\n"), Some(1));
        assert_eq!(heading_number("# ADR-072 検証記録 — 観測\n"), None);
        assert_eq!(heading_number("# takt step 別所要時間\n"), None);
    }

    /// 索引のリンク先が付属資料なら ADR ではないとして落とす。
    #[test]
    fn an_index_link_to_a_companion_is_rejected() {
        let files = [adr("adr-072-a.md", 72, "試験運用"), companion("adr-072-log.md")];
        let index = [index_line(72, "adr-072-a.md", " *(試験運用)*"), index_line(72, "adr-072-log.md", "")].concat();
        let found = run(&files, &index);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("ADR ではありません"), "{found:?}");
    }
}
