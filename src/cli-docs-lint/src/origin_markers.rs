//! origin-markers check — 起票由来タグの契約を検査する (機4a、defect-convergence-plan.md § Phase 4)。
//!
//! # 何のためのタグか
//!
//! 「機構を足したあと、defect 由来の起票は本当に減ったか」を決定論で測るための集計軸である。
//! 減っていなければ機構は儀式であり、対象外に置いた網羅性強制 (G2 対策) の再提案が要る。
//! **その判断材料を印象ではなく数で取る**ために、summary 行へ由来マーカーを付ける。
//!
//! # なぜ検査が要るか — 分類が主観だと退出基準を自分で無効化できる
//!
//! defect を `[improvement]` に付け替えるだけで「減った」を作れてしまう。そこで
//! **`[defect:*]` を名乗れる条件を機械検査に落とす**: 詳細エントリに実観測の証拠
//! (PR 番号 / run ID / 宣言された発火回数) があること。
//!
//! **これは floor であって証明ではない。** 証拠らしき文字列があることしか見ておらず、
//! それが本当に当該不具合の観測かは人間が読む。機械が保証するのは「証拠を書かずに defect を
//! 名乗ることはできない」までである。
//!
//! # 境界順位から先だけを見る
//!
//! [`ORIGIN_BOUNDARY_RANK`] 未満の既存行は対象外 (2026-09-02 ユーザー決定)。遡及の一斉
//! 書き換えを避け、**今日以降の起票**だけを測定対象にする。

use std::collections::BTreeMap;
use std::path::Path;

use lib_ledger::{parse_summary_entries, Origin, SummaryEntry, ORIGIN_BOUNDARY_RANK};

use crate::docs_files::{is_summary_file_name, is_todo_file_name, list_docs_files};
use crate::Violation;

/// 実観測の証拠として認める形。
///
/// **狭く取る。** 「N 件」「N 回」のような一般的な数量表現まで認めると、ほぼ全文が証拠と
/// 判定されて検査が空洞化する (fail-open 方向)。PR 番号と run ID は曖昧さが無く、発火回数は
/// **宣言された形** (`発火:`) でだけ受ける — 数えたなら書けるはずで、書かせることに意味がある。
const EVIDENCE_HINT: &str = "PR 番号 (`#123`) / run ID (`run 32589642740`) / 宣言された発火回数 (`発火: 4 回`)";

fn has_evidence(entry_body: &str) -> bool {
    contains_firing_count(entry_body)
        || contains_pr_reference(entry_body)
        || contains_run_id(entry_body)
}

/// `発火: <数値> 回` を含むか。
///
/// **`発火:` の存在だけでは証拠にしない。** 宣言形を要求する意味は「数えたなら書ける」
/// ところにあり、値の無い `発火:` を通すと契約が fail-open になる (CodeRabbit #472)。
fn contains_firing_count(text: &str) -> bool {
    text.split("発火:").skip(1).any(|rest| {
        let after_spaces = rest.trim_start();
        let digits = after_spaces.chars().take_while(char::is_ascii_digit).count();
        digits > 0 && after_spaces[digits..].trim_start().starts_with('回')
    })
}

/// `#<数字>` を含むか (PR / issue 参照)。
fn contains_pr_reference(text: &str) -> bool {
    text.split('#')
        .skip(1)
        .any(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// `run <6 桁以上の数字>` を含むか (GitHub Actions の run ID)。
fn contains_run_id(text: &str) -> bool {
    text.split("run ").skip(1).any(|rest| {
        rest.chars().take_while(char::is_ascii_digit).count() >= 6
    })
}

/// `### 順位 N:` 見出しから次の同レベル見出しの手前までを取り出す。
fn detail_body(markdown: &str, rank: u32) -> Option<&str> {
    let heading = format!("### 順位 {rank}:");
    let start = markdown.find(&heading)?;
    let rest = &markdown[start..];
    let end = rest[heading.len()..]
        .find("\n### ")
        .map(|offset| heading.len() + offset)
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn violation(file: &str, entry: &SummaryEntry, message: String) -> Violation {
    Violation {
        file: file.to_string(),
        line: 0,
        message: format!("順位 {}: {message}", entry.rank),
    }
}

/// 由来マーカーの契約を検査する。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let details = read_detail_files(docs_dir)?;
    let mut violations = Vec::new();
    for path in list_docs_files(docs_dir, is_summary_file_name)? {
        let file = path.display().to_string();
        let markdown = std::fs::read_to_string(&path)
            .map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        for entry in parse_summary_entries(&markdown)? {
            if entry.rank < ORIGIN_BOUNDARY_RANK {
                continue;
            }
            violations.extend(check_entry(&file, &entry, &details));
        }
    }
    Ok(violations)
}

fn check_entry(
    file: &str,
    entry: &SummaryEntry,
    details: &BTreeMap<String, String>,
) -> Vec<Violation> {
    let Some(origin) = entry.origin else {
        return vec![violation(
            file,
            entry,
            format!(
                "由来マーカーがありません。順位 {ORIGIN_BOUNDARY_RANK} 以降の行は \
                 タイトル冒頭に {} のいずれかを付けてください",
                markers_hint()
            ),
        )];
    };
    if !origin.is_defect() {
        return Vec::new();
    }
    let Some(body) = details.get(&entry.detail_file).and_then(|markdown| detail_body(markdown, entry.rank)) else {
        return vec![violation(
            file,
            entry,
            format!(
                "{} を名乗っていますが詳細エントリ ({}) を読めません。証拠の有無を確認できないため拒否します",
                origin.marker(),
                entry.detail_file
            ),
        )];
    };
    if has_evidence(body) {
        return Vec::new();
    }
    vec![violation(
        file,
        entry,
        format!(
            "{} を名乗っていますが詳細エントリに実観測の証拠がありません。\
             証拠として認める形: {EVIDENCE_HINT}。証拠が無いものは [improvement] として登録してください",
            origin.marker()
        ),
    )]
}

fn markers_hint() -> String {
    Origin::all()
        .iter()
        .map(|origin| origin.marker())
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 詳細エントリのファイル名 → 中身。
///
/// **読めないファイルは握り潰さない** — 証拠の有無を確認できないまま緑にしない。
fn read_detail_files(docs_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    for path in list_docs_files(docs_dir, |name| {
        is_todo_file_name(name) && !is_summary_file_name(name)
    })? {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("ファイル名を読めません: {}", path.display()))?
            .to_string();
        let markdown = std::fs::read_to_string(&path)
            .map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        files.insert(name, markdown);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE_HEADER: &str = "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                                |---|---|---|---|---|---|\n";

    fn docs_dir(summary_rows: &str, detail: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("todo-summary.md"),
            format!("{TABLE_HEADER}{summary_rows}"),
        )
        .expect("write summary");
        std::fs::write(dir.path().join("todo1.md"), detail).expect("write detail");
        dir
    }

    fn row(rank: u32, title: &str) -> String {
        format!("| {rank} | 🔧 Tier 2 | **{title}** | todo1.md | S | なし |\n")
    }

    fn entry(rank: u32, body: &str) -> String {
        format!("### 順位 {rank}: サンプル\n\n{body}\n")
    }

    /// **境界より前の行は対象外** — 既存 261 行を遡って書き換えない (ユーザー決定)。
    #[test]
    fn rows_below_the_boundary_are_untouched() {
        let dir = docs_dir(&row(499, "マーカー無しの既存行"), &entry(499, "証拠なし"));
        assert!(check(dir.path()).expect("check").is_empty());
    }

    /// 境界以降でマーカーが無ければ拒否する (fail-closed)。
    #[test]
    fn a_missing_marker_at_or_after_the_boundary_is_rejected() {
        let dir = docs_dir(&row(500, "マーカー無し"), &entry(500, "本文"));
        let violations = check(dir.path()).expect("check");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("由来マーカーがありません"));
    }

    /// 未知のタグはマーカーとして認めない (欠落と同じ扱いに倒す)。
    #[test]
    fn an_unknown_marker_is_treated_as_missing() {
        let dir = docs_dir(&row(500, "[defect:G3] 未知のタグ"), &entry(500, "本文"));
        let violations = check(dir.path()).expect("check");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("由来マーカーがありません"));
    }

    /// `[improvement]` は証拠を要求しない。
    #[test]
    fn an_improvement_needs_no_evidence() {
        let dir = docs_dir(&row(500, "[improvement] 整理"), &entry(500, "証拠なし"));
        assert!(check(dir.path()).expect("check").is_empty());
    }

    /// **証拠なしの `[defect:*]` は拒否する** — これが無いと分類を付け替えるだけで
    /// 退出基準を満たせてしまう。
    #[test]
    fn a_defect_without_evidence_is_rejected() {
        let dir = docs_dir(&row(500, "[defect:G1] 不具合"), &entry(500, "動機だけ書いた本文"));
        let violations = check(dir.path()).expect("check");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("実観測の証拠がありません"));
    }

    /// 認める証拠の 3 形式 (PR 番号 / run ID / 宣言された発火回数)。
    #[test]
    fn the_three_evidence_forms_are_accepted() {
        for body in [
            "PR [#463](https://example.com/pull/463) で実測した",
            "夜間 run 32589642740 で観測",
            "発火: 4 回 (2026-08 の実測)",
        ] {
            let dir = docs_dir(&row(500, "[defect:G2] 不具合"), &entry(500, body));
            assert!(check(dir.path()).expect("check").is_empty(), "body={body}");
        }
    }

    /// **値の無い `発火:` は証拠にしない** (CodeRabbit #472)。宣言形を要求する意味は
    /// 「数えたなら書ける」ところにあり、宣言だけ真似できると契約が fail-open になる。
    #[test]
    fn a_firing_marker_without_a_count_is_not_evidence() {
        for body in ["発火:", "発火: あり", "発火: 数回", "発火: 4 (単位なし)"] {
            let dir = docs_dir(&row(500, "[defect:G1] 不具合"), &entry(500, body));
            assert_eq!(check(dir.path()).expect("check").len(), 1, "body={body}");
        }
    }

    /// 宣言形は数値 + `回` で受ける (空白の有無は問わない)。
    #[test]
    fn a_declared_firing_count_is_evidence() {
        for body in ["発火: 4 回", "発火:12回", "発火:  2  回 (2026-08 実測)"] {
            let dir = docs_dir(&row(500, "[defect:G1] 不具合"), &entry(500, body));
            assert!(check(dir.path()).expect("check").is_empty(), "body={body}");
        }
    }

    /// **一般的な数量表現は証拠にしない** — 認めると検査が空洞化する (fail-open)。
    #[test]
    fn a_bare_quantity_is_not_evidence() {
        for body in ["3 件の指摘があった", "4 回中 2 回で再現", "#hashtag だけ"] {
            let dir = docs_dir(&row(500, "[defect:G1] 不具合"), &entry(500, body));
            assert_eq!(check(dir.path()).expect("check").len(), 1, "body={body}");
        }
    }

    /// 詳細エントリを引けないなら緑にしない (証拠の有無を確認できないため)。
    #[test]
    fn a_defect_without_a_readable_detail_entry_is_rejected() {
        let dir = docs_dir(&row(500, "[defect:G1] 不具合"), &entry(501, "別の順位の本文"));
        let violations = check(dir.path()).expect("check");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("詳細エントリ"));
    }

    /// 詳細エントリの切り出しは**次の見出しの手前まで** (隣のエントリの証拠を借りない)。
    #[test]
    fn evidence_does_not_leak_from_the_next_entry() {
        let detail = format!(
            "{}\n{}",
            entry(500, "証拠なし"),
            entry(501, "PR [#1](https://example.com/pull/1)")
        );
        let dir = docs_dir(&row(500, "[defect:G1] 不具合"), &detail);
        assert_eq!(check(dir.path()).expect("check").len(), 1);
    }
}
