//! 夜間 PR に台帳の後始末が含まれているかを判定する純粋層 (I/O なし)。
//!
//! # なぜ要るか
//!
//! 夜間ループで**完了を表現するのは、台帳削除コミットがマージされること 1 つだけ**
//! ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 19)。ところがその
//! 削除は「ブランチに載って運ばれるデータ」なので、運搬中に失われても誰も気づかない。
//!
//! 2026-08-30 に実際に失われた: 夜間 PR は `chore(ledger) 台帳削除` (親) →
//! `実装` (子) の 2 コミット構成で、人間が `jj rebase -r <先端>` を使ったため
//! **親が置き去りになった**。#427 / #459 / #461 の 3 本すべてで同じことが起き、
//! 実装だけがマージされて台帳行が残った。2026-09-01 の夜間 run は残った行から
//! 順位 324 を再選択し、実装済みなので diff が空になり red で終わった。
//!
//! # 何を見るか — 行ではなく順位
//!
//! diff のテキストではなく **head の状態**を見る。「順位 N が台帳・順位 table・詳細
//! エントリのどこにも残っていないこと」だけを要求するので、行番号にも文脈行にも
//! 依存しない。運び方 (リベース / squash / 手作業) が何であれ、結果だけを見る。

use std::collections::BTreeSet;

/// 夜間ブランチの命名規約。
const NIGHTLY_BRANCH_PREFIX: &str = "claude/nightly-";

/// 順位が残っていた場所の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// 台帳 `docs/claude-code-web-tasks.md` のタスク表。**再選択の条件はここ**。
    Ledger,
    /// 順位 table `docs/todo-summary*.md`。
    Summary,
    /// 詳細エントリ `docs/todoN.md` の `### 順位 N:` 見出し。
    Detail,
}

impl Place {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Place::Ledger => "台帳",
            Place::Summary => "順位 table",
            Place::Detail => "詳細エントリ",
        }
    }
}

/// 1 ファイルを走査した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scan {
    pub(crate) place: Place,
    pub(crate) file: String,
    pub(crate) ranks: BTreeSet<u32>,
}

/// ブランチ名から順位を読む (I/O なし)。夜間ブランチでなければ `None`。
///
/// **数字だけを受ける。** `claude/nightly-324-retry` のような派生名を順位 324 と
/// 読むと、別物の PR に後始末を要求してしまう。
pub(crate) fn rank_from_branch(branch: &str) -> Option<u32> {
    let digits = branch.trim().strip_prefix(NIGHTLY_BRANCH_PREFIX)?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// 順位が残っている箇所を列挙する (I/O なし)。空なら後始末済み。
pub(crate) fn residue(rank: u32, scans: &[Scan]) -> Vec<&Scan> {
    scans
        .iter()
        .filter(|scan| scan.ranks.contains(&rank))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(place: Place, file: &str, ranks: &[u32]) -> Scan {
        Scan {
            place,
            file: file.to_string(),
            ranks: ranks.iter().copied().collect(),
        }
    }

    #[test]
    fn a_nightly_branch_yields_its_rank() {
        assert_eq!(rank_from_branch("claude/nightly-324"), Some(324));
    }

    #[test]
    fn other_branches_are_not_nightly() {
        assert_eq!(rank_from_branch("master"), None);
        assert_eq!(rank_from_branch("fix/branch-cleanup-exe"), None);
        assert_eq!(rank_from_branch("claude/nightly-"), None);
    }

    /// **派生名を順位として読まない。** 読むと無関係な PR に後始末を要求する。
    #[test]
    fn a_suffixed_branch_name_is_not_a_rank() {
        assert_eq!(rank_from_branch("claude/nightly-324-retry"), None);
        assert_eq!(rank_from_branch("claude/nightly-324/fix"), None);
    }

    /// **incident 再現 (2026-08-30)**: 実装だけがマージされ、3 箇所とも残った形。
    #[test]
    fn the_incident_shape_is_reported_in_all_three_places() {
        let scans = vec![
            scan(Place::Ledger, "docs/claude-code-web-tasks.md", &[324, 455]),
            scan(Place::Summary, "docs/summary-fixture.md", &[324]),
            scan(Place::Detail, "docs/detail-fixture.md", &[324]),
        ];
        let found = residue(324, &scans);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].place, Place::Ledger);
    }

    /// 後始末が済んでいれば空。
    #[test]
    fn a_cleaned_up_rank_has_no_residue() {
        let scans = vec![
            scan(Place::Ledger, "docs/claude-code-web-tasks.md", &[455]),
            scan(Place::Summary, "docs/summary-fixture.md", &[455]),
            scan(Place::Detail, "docs/detail-fixture.md", &[455]),
        ];
        assert!(residue(324, &scans).is_empty());
    }

    /// **部分的な残りも見逃さない。** 台帳だけ消して順位 table を残す形は、台帳から
    /// 見えない孤児になる (詳細エントリとの 1:1 は D3 の entry_pairing が別途見る)。
    #[test]
    fn a_partial_cleanup_still_reports_what_is_left() {
        let scans = vec![
            scan(Place::Ledger, "docs/claude-code-web-tasks.md", &[]),
            scan(Place::Summary, "docs/summary-fixture.md", &[324]),
            scan(Place::Detail, "docs/detail-fixture.md", &[324]),
        ];
        let found = residue(324, &scans);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|s| s.place != Place::Ledger));
    }

    /// 他の順位は判定に影響しない。
    #[test]
    fn unrelated_ranks_are_ignored() {
        let scans = vec![scan(
            Place::Ledger,
            "docs/claude-code-web-tasks.md",
            &[199, 356, 455],
        )];
        assert!(residue(324, &scans).is_empty());
    }
}
