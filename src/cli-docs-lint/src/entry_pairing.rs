//! entry_pairing check — 順位 table の行と `docs/todoN.md` の詳細エントリの 1:1 対応を検査する
//! (順位 441、defect-convergence-plan.md § Phase D の D3)。
//!
//! # 由来
//!
//! 2026-08-12 に todo14.md の孤児エントリ 4 件が約 3 週間検出されず滞留していた。既存の
//! validator (preamble / cross_ref / priority_inversion) はどれも 2 文書の対応を見ていない。
//!
//! 起票時 (順位 441) の案は「タイトル文字列の突合、完全一致は求めず許容度を設計する」
//! だったが**採らなかった**。許容度を持たせると lint は通るのに `cli-ledger-cleanup` の
//! 照合は落ちる、というズレが残るためである。[ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md)
//! § 改訂 (2026-08-26) で結合キーを順位へ移したので、**同じ鍵で検査する**。
//!
//! # 何を見るか
//!
//! - **方向 A**: 順位 table の各行に対し、宣言先ファイルへ `### 順位 N: ...` が 1 件だけ在る
//! - **方向 B1**: `### 順位 N: ...` の N が順位 table に無い (採番だけ残った孤児)
//! - **方向 B2**: 順位を持たない `### ` 見出しが**タスクエントリの形**をしている (採番漏れ)
//!
//! # タスクエントリの見分け方
//!
//! `todoN.md` の `### ` 見出しには、タスクではないもの (週次レビューの束ね節 / 由来別の
//! チェックリスト / 決定や却下の記録) が混ざる。これらに順位を振ると台帳へ実在しない
//! タスクが増えるため、検査対象から外す必要がある。
//!
//! **判別子は「`**動機**` を含み、かつ `#### 完了基準` を持つ」**。
//! [ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md) § 新規エントリ template
//! が両方を要求しており、束ね節や記録類は完了基準を持たない。
//!
//! 2026-08-26 の実測: 採番済み 257 件のうち 251 件 (98%) が該当し、順位を持たない 19 件の
//! うち該当したのは 5 件だけだった。その 5 件は実際に採番漏れで、同日 493-497 を採番した。
//! 残る 14 件 (束ね節 11 / 決定の記録 1 / 却下の記録 1 / 単発の観測記録 1) は 1 件も該当しない。
//!
//! **構造だけで見分けており、見出しに印を付ける規約を足していない。** 既存 276 件の
//! 見出しを書き換えずに済み、規約を守る義務を人間に課さない
//! ([ADR-042](../../../docs/adr/adr-042-rule-vs-mechanism-boundary.md))。

use crate::docs_files::{list_docs_files, list_summary_files};
use crate::Violation;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// 順位 table の 1 行。
struct SummaryEntry {
    rank: u32,
    detail_file: String,
    source_line: usize,
    source_file: String,
}

/// `### 順位 N: ...` の N を読む (I/O なし)。
///
/// **コロンまでを厳密に見る** — 前方一致だと `順位 19` が `順位 193` に当たる
/// (`lib-ledger` の `heading_rank` と同じ契約)。
fn heading_rank(line: &str) -> Option<u32> {
    let rest = line.trim_end().strip_prefix("### 順位 ")?;
    let (digits, _) = rest.split_once(':')?;
    digits.trim().parse::<u32>().ok()
}

/// 見出し配下の本文がタスクエントリの形をしているか (I/O なし)。
fn looks_like_task_entry(body: &[&str]) -> bool {
    body.iter().any(|l| l.contains("**動機**"))
        && body.iter().any(|l| l.trim_end().starts_with("#### 完了基準"))
}

/// `todoN.md` の `### ` 見出し 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DetailHeading {
    line: usize,
    /// `### 順位 N:` の N。前置形でなければ `None`。
    rank: Option<u32>,
    /// 本文がタスクエントリの形をしているか (束ね節・記録との判別)。
    is_task: bool,
}

/// ファイル名 → その中の `### ` 見出し。
type HeadingsByFile = BTreeMap<String, Vec<DetailHeading>>;

/// `todoN.md` から `### ` 見出しを取り出す。
fn detail_headings(content: &str) -> Vec<DetailHeading> {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("### ") {
            continue;
        }
        let end = lines
            .iter()
            .enumerate()
            .skip(i + 1)
            .find(|(_, l)| l.starts_with("### ") || l.starts_with("## "))
            .map_or(lines.len(), |(j, _)| j);
        out.push(DetailHeading {
            line: i + 1,
            rank: heading_rank(line),
            is_task: looks_like_task_entry(&lines[i + 1..end]),
        });
    }
    out
}

/// 順位 table の行を読む。`| 順位 | Tier | タスク | ファイル | ...` の 4 列目が宣言先。
fn summary_entries(file: &str, content: &str) -> Vec<SummaryEntry> {
    let mut out = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if !line.trim_start().starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() < 6 {
            continue;
        }
        let Ok(rank) = cells[1].trim().parse::<u32>() else {
            continue;
        };
        let detail_file = cells[4].trim().to_string();
        if !detail_file.starts_with("todo") || !detail_file.ends_with(".md") {
            continue;
        }
        out.push(SummaryEntry {
            rank,
            detail_file,
            source_line: i + 1,
            source_file: file.to_string(),
        });
    }
    out
}

pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let mut entries = Vec::new();
    for path in list_summary_files(docs_dir)? {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("順位 table ファイル名が不正です ({})", path.display()))?
            .to_string();
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("順位 table を読めません ({}): {e}", path.display()))?;
        entries.extend(summary_entries(&name, &content));
    }
    if entries.is_empty() {
        return Err("順位 table から 1 行も読めませんでした (false-green guard)".to_string());
    }

    let mut headings = read_all_detail_files(docs_dir)?;
    for entry in &entries {
        if headings.contains_key(&entry.detail_file) {
            continue;
        }
        let path = docs_dir.join(&entry.detail_file);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("詳細ファイルを読めません ({}): {e}", path.display()))?;
        headings.insert(entry.detail_file.clone(), detail_headings(&content));
    }

    Ok(evaluate(&entries, &headings))
}

/// `docs_dir` 直下の `todoN.md` を**全件**読む。
///
/// **順位 table が参照するファイルだけを読んではいけない** (CodeRabbit #452)。行がすべて
/// 完了して消えたファイルは参照されなくなり、そこに残った孤児や未採番タスクが走査対象から
/// 外れる — 方向 B1 / B2 が false-green になる。**この検査が拾うべき状態そのもの**なので、
/// 参照の有無に関係なく全件を読む。
fn read_all_detail_files(docs_dir: &Path) -> Result<HeadingsByFile, String> {
    let mut out = HeadingsByFile::new();
    for path in list_docs_files(docs_dir, is_detail_file_name)? {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("詳細ファイルを読めません ({name}): {e}"))?;
        out.insert(name, detail_headings(&content));
    }
    Ok(out)
}

/// `todoN.md` (N は省略可) の形か。`todo-summary.md` 等は詳細ファイルではない。
fn is_detail_file_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let Some(digits) = stem.strip_prefix("todo") else {
        return false;
    };
    digits.is_empty() || digits.chars().all(|c| c.is_ascii_digit())
}

/// 順位 → その見出しが在る (ファイル, 行) の全出現箇所 (ファイル横断)。
fn rank_occurrences(headings: &HeadingsByFile) -> BTreeMap<u32, Vec<(&str, usize)>> {
    let mut occurrences: BTreeMap<u32, Vec<(&str, usize)>> = BTreeMap::new();
    for (file, hs) in headings {
        for h in hs {
            if let Some(r) = h.rank {
                occurrences.entry(r).or_default().push((file.as_str(), h.line));
            }
        }
    }
    occurrences
}

/// 方向 A: 宣言先ファイル内に詳細エントリが 1 件だけ在るか。
fn declared_file_count_violation(entry: &SummaryEntry, headings: &HeadingsByFile) -> Option<Violation> {
    let found = headings
        .get(&entry.detail_file)
        .map_or(0, |hs| hs.iter().filter(|h| h.rank == Some(entry.rank)).count());
    if found == 1 {
        return None;
    }
    Some(Violation {
        file: entry.source_file.clone(),
        line: entry.source_line,
        message: format!(
            "順位 {rank} の詳細エントリが {file} に {found} 件あります (1 件であるべき)。\
             見出しは `### 順位 {rank}: <タイトル>` の形にしてください",
            rank = entry.rank,
            file = entry.detail_file
        ),
    })
}

/// 宣言先以外のファイルにも同じ順位の見出しが在るか (SIM-NEW-entry_pairing-L396 のクロスファイル重複検査)。
fn cross_file_duplicate_violations(
    entry: &SummaryEntry,
    occurrences_by_rank: &BTreeMap<u32, Vec<(&str, usize)>>,
) -> Vec<Violation> {
    let Some(occs) = occurrences_by_rank.get(&entry.rank) else {
        return Vec::new();
    };
    occs.iter()
        .filter(|(file, _)| *file != entry.detail_file)
        .map(|(file, line)| Violation {
            file: file.to_string(),
            line: *line,
            message: format!(
                "順位 {rank} の詳細エントリが宣言先の {declared} 以外に {file} にも存在します\
                 (クロスファイル重複)。同じ順位を複数ファイルへ重複させず、{declared} 側の\
                 1 件に統一してください",
                rank = entry.rank,
                declared = entry.detail_file,
                file = file
            ),
        })
        .collect()
}

/// 方向 B1/B2: 詳細エントリ側から見た孤児 (採番だけ残った) / 採番漏れタスク。
fn orphan_and_unnumbered_violations(
    headings: &HeadingsByFile,
    ranked: &BTreeSet<u32>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (file, hs) in headings {
        for DetailHeading { line, rank, is_task } in hs {
            match rank {
                Some(r) if !ranked.contains(r) => violations.push(Violation {
                    file: file.clone(),
                    line: *line,
                    message: format!(
                        "順位 {r} の詳細エントリがありますが、順位 table に行がありません。\
                         完了済みなら削除し、未完了なら順位 table へ行を追加してください"
                    ),
                }),
                None if *is_task => violations.push(Violation {
                    file: file.clone(),
                    line: *line,
                    message: "タスクエントリの形 (動機 + 完了基準) をしていますが順位がありません。\
                              採番して順位 table へ行を追加し、見出しを `### 順位 N: <タイトル>` に\
                              してください (束ね節や記録であれば完了基準を持たせないこと)"
                        .to_string(),
                }),
                _ => {}
            }
        }
    }
    violations
}

/// 3 方向の照合 (I/O なし)。
fn evaluate(entries: &[SummaryEntry], headings: &HeadingsByFile) -> Vec<Violation> {
    let ranked: BTreeSet<u32> = entries.iter().map(|e| e.rank).collect();
    let occurrences_by_rank = rank_occurrences(headings);

    let mut violations = Vec::new();
    let mut seen_ranks: BTreeMap<u32, usize> = BTreeMap::new();
    for entry in entries {
        let occurrence = seen_ranks.entry(entry.rank).or_insert(0);
        *occurrence += 1;
        if *occurrence > 1 {
            violations.push(duplicate_summary_row_violation(entry, *occurrence));
            continue;
        }
        violations.extend(declared_file_count_violation(entry, headings));
        violations.extend(cross_file_duplicate_violations(entry, &occurrences_by_rank));
    }
    violations.extend(orphan_and_unnumbered_violations(headings, &ranked));
    violations
}

/// 順位 table 側に同じ順位が 2 行以上ある (CodeRabbit #452)。
///
/// **2 行目以降を違反にして方向 A の照合から外す。** 外さないと、詳細エントリが 1 件しか
/// 無いのに両方の行が「1 件ある」で通り、1:1 対応の破れを見逃す。順位は追記型 ID で
/// 再利用しない ([ADR-033](../../../docs/adr/adr-033-todo-numbering-simplification.md)
/// § 改訂 2026-08-16) ため、重複は常に誤りである。
fn duplicate_summary_row_violation(entry: &SummaryEntry, occurrence: usize) -> Violation {
    Violation {
        file: entry.source_file.clone(),
        line: entry.source_line,
        message: format!(
            "順位 {} が順位 table に {occurrence} 件目として重複しています。\
             順位は追記型 ID で再利用しないため、重複は採番の誤りです",
            entry.rank
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, file: &str) -> SummaryEntry {
        SummaryEntry {
            rank,
            detail_file: file.to_string(),
            source_line: 1,
            source_file: "todo-summary2.md".to_string(),
        }
    }

    fn head(line: usize, rank: Option<u32>, is_task: bool) -> DetailHeading {
        DetailHeading { line, rank, is_task }
    }

    fn heads(v: &[DetailHeading]) -> HeadingsByFile {
        BTreeMap::from([("todo1.md".to_string(), v.to_vec())])
    }

    fn heads_across(files: &[(&str, &[DetailHeading])]) -> HeadingsByFile {
        files.iter().map(|(file, v)| (file.to_string(), v.to_vec())).collect()
    }

    /// 一時 docs ディレクトリを作り、[`check`] を**実ファイル経由で**呼ぶ。
    ///
    /// `evaluate` を直接叩くテストだけでは、**どのファイルを読むかを決める層**が固定
    /// されない。実際 `read_all_detail_files` を空 Map に差し替える変異が素通りした
    /// (CodeRabbit #452 の Major と同じ経路)。
    fn check_in_temp_docs(files: &[(&str, &str)]) -> Vec<Violation> {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).expect("write");
        }
        check(dir.path()).expect("check")
    }

    const MINIMAL_SUMMARY: &str = "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                                   |---|---|---|---|---|---|\n\
                                   | 10 | T1 | **A** | todo1.md | S | なし |\n";
    const TASK_BODY: &str = "> **動機**: x\n\n#### 完了基準\n\n- done\n";

    /// **順位 table から参照されないファイルも走査対象**である (CodeRabbit #452)。
    /// 行がすべて完了して消えたファイルに孤児が残ると、参照だけを頼りにする実装では
    /// 方向 B1 / B2 が false-green になる。**実ファイル経由で確かめる。**
    #[test]
    fn an_unreferenced_detail_file_is_still_scanned_from_disk() {
        let violations = check_in_temp_docs(&[
            ("todo-summary.md", MINIMAL_SUMMARY),
            ("todo-summary2.md", "# 空\n"),
            ("todo1.md", &format!("# TODO\n\n### 順位 10: A\n\n{TASK_BODY}")),
            ("todo2.md", &format!("# TODO\n\n### 順位 99: 孤児\n\n{TASK_BODY}")),
        ]);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].file, "todo2.md", "{:?}", violations[0]);
        assert!(violations[0].message.contains("順位 99"), "{:?}", violations[0]);
    }

    /// 参照されているファイルだけの構成では違反 0 (false-positive guard)。
    #[test]
    fn a_clean_docs_dir_reports_nothing() {
        let violations = check_in_temp_docs(&[
            ("todo-summary.md", MINIMAL_SUMMARY),
            ("todo-summary2.md", "# 空\n"),
            ("todo1.md", &format!("# TODO\n\n### 順位 10: A\n\n{TASK_BODY}")),
        ]);
        assert!(violations.is_empty(), "{violations:?}");
    }

    /// **順位 table ファイルは固定 2 要素ではなく prefix scan で列挙する**
    /// (simplicity-review SIM-NEW-entry_pairing-L44)。`todo-summary3.md` のような
    /// 将来の分割 part も、固定配列への追記なしに走査対象へ入ることを保証する。
    #[test]
    fn a_third_split_summary_part_is_scanned_without_code_changes() {
        let violations = check_in_temp_docs(&[
            ("todo-summary.md", MINIMAL_SUMMARY),
            ("todo-summary2.md", "# 空\n"),
            (
                "todo-summary3.md",
                "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                 |---|---|---|---|---|---|\n\
                 | 20 | T1 | **B** | todo1.md | S | なし |\n",
            ),
            (
                "todo1.md",
                &format!("# TODO\n\n### 順位 10: A\n\n{TASK_BODY}### 順位 20: B\n\n{TASK_BODY}"),
            ),
        ]);
        assert!(violations.is_empty(), "{violations:?}");
    }

    /// 固定 2 要素配列時代は `todo-summary2.md` の欠落が read エラーになっていたが、
    /// prefix scan では**存在する part だけ**を検査すればよい (分割前の単一 part 構成でも
    /// 動作する回帰テスト)。
    #[test]
    fn a_single_summary_part_without_a_second_file_still_works() {
        let violations = check_in_temp_docs(&[
            ("todo-summary.md", MINIMAL_SUMMARY),
            ("todo1.md", &format!("# TODO\n\n### 順位 10: A\n\n{TASK_BODY}")),
        ]);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn an_unreferenced_detail_file_is_still_checked() {
        let headings = BTreeMap::from([
            ("todo1.md".to_string(), vec![head(5, Some(10), true)]),
            ("todo2.md".to_string(), vec![head(3, Some(99), true), head(8, None, true)]),
        ]);
        let v = evaluate(&[entry(10, "todo1.md")], &headings);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().all(|x| x.file == "todo2.md"), "{v:?}");
    }

    #[test]
    fn only_todo_numbered_files_count_as_detail_files() {
        assert!(is_detail_file_name("todo.md"));
        assert!(is_detail_file_name("todo25.md"));
        assert!(!is_detail_file_name("todo-summary.md"));
        assert!(!is_detail_file_name("todo-summary2.md"));
        assert!(!is_detail_file_name("bugfix-batch-plan.md"));
        assert!(!is_detail_file_name("todo25.txt"));
    }

    /// **順位 table 側の重複順位を見逃さない** (CodeRabbit #452)。
    /// 詳細エントリが 1 件しか無いのに 2 行あると、重複を弾かない実装では両方が
    /// 方向 A を通過してしまう。
    #[test]
    fn a_duplicated_rank_in_the_summary_table_is_a_violation() {
        let v = evaluate(
            &[entry(10, "todo1.md"), entry(10, "todo1.md")],
            &heads(&[head(5, Some(10), true)]),
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("重複"), "{:?}", v[0]);
    }

    #[test]
    fn heading_rank_requires_the_prefixed_form_with_a_colon() {
        assert_eq!(heading_rank("### 順位 10: タイトル"), Some(10));
        assert_eq!(heading_rank("### タイトル (順位 10)"), None);
        assert_eq!(heading_rank("### 順位 10 タイトル"), None);
        assert_eq!(heading_rank("#### 順位 10: タイトル"), None);
    }

    /// **順位 19 の検査が 順位 193 に当たってはいけない。**
    #[test]
    fn a_rank_prefix_does_not_match_a_longer_rank() {
        assert_eq!(heading_rank("### 順位 193: タイトル"), Some(193));
        assert_ne!(heading_rank("### 順位 193: タイトル"), Some(19));
    }

    #[test]
    fn a_task_entry_needs_both_motivation_and_completion_criteria() {
        assert!(looks_like_task_entry(&["> **動機**: x", "#### 完了基準", "y"]));
        assert!(!looks_like_task_entry(&["> **動機**: x", "#### 作業計画"]));
        assert!(!looks_like_task_entry(&["#### 完了基準", "y"]));
        assert!(!looks_like_task_entry(&["> 週次レビューで採用した findings。"]));
    }

    #[test]
    fn a_paired_entry_is_clean() {
        assert!(evaluate(&[entry(10, "todo1.md")], &heads(&[head(5, Some(10), true)])).is_empty());
    }

    /// 方向 A: 順位 table に行があるのに詳細エントリが無い。
    #[test]
    fn a_summary_row_without_a_detail_entry_is_a_violation() {
        let v = evaluate(&[entry(10, "todo1.md")], &heads(&[head(5, Some(11), true)]));
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].message.contains("順位 10 の詳細エントリが"), "{:?}", v[0]);
    }

    /// 方向 B1: 採番だけ残った孤児。
    #[test]
    fn a_detail_entry_whose_rank_is_absent_from_the_table_is_a_violation() {
        let v = evaluate(
            &[entry(10, "todo1.md")],
            &heads(&[head(5, Some(10), true), head(9, Some(99), true)]),
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("順位 99"), "{:?}", v[0]);
    }

    /// 方向 B2: タスクの形なのに採番されていない。
    #[test]
    fn an_unnumbered_task_entry_is_a_violation() {
        let v = evaluate(&[entry(10, "todo1.md")], &heads(&[head(5, Some(10), true), head(9, None, true)]));
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("順位がありません"), "{:?}", v[0]);
    }

    /// **束ね節・記録は違反にしない。** ここを間違えると台帳に実在しないタスクが増える。
    #[test]
    fn a_grouping_section_without_a_rank_is_not_a_violation() {
        let v = evaluate(&[entry(10, "todo1.md")], &heads(&[head(5, Some(10), true), head(9, None, false)]));
        assert!(v.is_empty(), "{v:?}");
    }

    /// 同じ順位の詳細エントリが 2 件あるのも違反 (どちらを消すか決まらない)。
    #[test]
    fn a_duplicated_rank_is_a_violation() {
        let v = evaluate(
            &[entry(10, "todo1.md")],
            &heads(&[head(5, Some(10), true), head(9, Some(10), true)]),
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("2 件"), "{:?}", v[0]);
    }

    /// クロスファイル版: 宣言先 (todo1.md) 側は 1 件だけで正しいのに、別ファイル
    /// (todo2.md) に同じ順位の見出しが誤って紛れ込んだケースも違反にする
    /// (SIM-NEW-entry_pairing-L396: 方向 A/B はいずれもファイル内/順位の有無しか
    /// 見ておらず、このクロスファイル重複を見逃していた)。
    #[test]
    fn a_duplicated_rank_across_files_is_a_violation() {
        let v = evaluate(
            &[entry(10, "todo1.md")],
            &heads_across(&[
                ("todo1.md", &[head(5, Some(10), true)]),
                ("todo2.md", &[head(9, Some(10), true)]),
            ]),
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].file, "todo2.md");
        assert!(v[0].message.contains("クロスファイル重複"), "{:?}", v[0]);
    }
}
