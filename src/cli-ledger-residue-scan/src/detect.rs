//! マージ済みなのに台帳に残っている順位を見つける純粋層 (I/O なし)。
//!
//! # B1 (`cli-ledger-removal-check`) との違い
//!
//! B1 は **1 本の PR** を見る。`claude/nightly-<順位>` を head とする PR に対し、その順位が
//! 台帳から消えていることをマージ前に要求する。したがって**これから壊れるのを止める**層で、
//! 既に master に入ってしまった残骸には効かない。
//!
//! 本 module は逆向きに、**台帳の全順位**を見て「その順位の夜間 PR が既にマージ済みなら
//! 残骸」と判定する。2026-08-30 に 3 本 (#427 / #459 / #461) が同時に壊れ、13 日間
//! 誰も気づかなかったのは、この向きの検査がどこにも無かったためである。
//!
//! # 射程 — 夜間 PR 由来のみ
//!
//! 判定材料は `claude/nightly-<順位>` というブランチ名だけである。人間が別名のブランチで
//! 同じ作業を実装して後始末を忘れた場合、順位を引く手掛かりが無いので**検出できない**。
//! この限界は出力にも明記する — 「0 件」を「台帳は健全」と読み違えさせないため。

use std::collections::BTreeSet;

use lib_ledger::rank_from_nightly_branch;

/// マージ済み PR 1 件 (gh の JSON から読んだもの)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MergedPr {
    pub(crate) number: u64,
    pub(crate) head_ref: String,
    pub(crate) merged_at: String,
}

/// 台帳に残っている順位と、その根拠になったマージ済み PR。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Residue {
    pub(crate) rank: u32,
    pub(crate) pr: u64,
    pub(crate) merged_at: String,
}

impl Residue {
    pub(crate) fn line(&self) -> String {
        format!(
            "[NIGHTLY_LEDGER_RESIDUE] 順位 {}: PR #{} が {} にマージ済みなのに台帳へ残っています",
            self.rank,
            self.pr,
            date_of(&self.merged_at)
        )
    }
}

/// ISO8601 の日付部分だけを出す。時刻まで出しても人間の判断材料にならない。
fn date_of(merged_at: &str) -> &str {
    match merged_at.split_once('T') {
        Some((date, _)) => date,
        None => merged_at,
    }
}

/// 台帳に残る順位のうち、夜間 PR が既にマージ済みのものを返す (I/O なし)。
///
/// **同じ順位に複数の PR がある場合は最も新しいマージを採る。** 再投入 (決定 20) で
/// 同じブランチ名の PR が複数できることがあり、古い方を根拠に出すと調査が迷子になる。
pub(crate) fn residue(ledger_ranks: &BTreeSet<u32>, merged: &[MergedPr]) -> Vec<Residue> {
    let mut found: Vec<Residue> = Vec::new();
    for pr in merged {
        let Some(rank) = rank_from_nightly_branch(&pr.head_ref) else {
            continue;
        };
        if !ledger_ranks.contains(&rank) {
            continue;
        }
        match found.iter_mut().find(|r| r.rank == rank) {
            Some(existing) if existing.merged_at < pr.merged_at => {
                existing.pr = pr.number;
                existing.merged_at = pr.merged_at.clone();
            }
            Some(_) => {}
            None => found.push(Residue {
                rank,
                pr: pr.number,
                merged_at: pr.merged_at.clone(),
            }),
        }
    }
    found.sort_by_key(|r| r.rank);
    found
}

/// 除外用の CSV (`--exclude-ranks` へ渡す形)。残骸が無ければ空文字。
pub(crate) fn ranks_csv(found: &[Residue]) -> String {
    found
        .iter()
        .map(|r| r.rank.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(number: u64, head_ref: &str, merged_at: &str) -> MergedPr {
        MergedPr {
            number,
            head_ref: head_ref.to_string(),
            merged_at: merged_at.to_string(),
        }
    }

    fn ledger(ranks: &[u32]) -> BTreeSet<u32> {
        ranks.iter().copied().collect()
    }

    /// **incident 再現 (2026-08-30)**: 3 本の実装だけがマージされ、台帳行が残った形。
    #[test]
    fn the_incident_ranks_are_all_reported() {
        let merged = vec![
            pr(427, "claude/nightly-324", "2026-08-30T12:22:36Z"),
            pr(459, "claude/nightly-412", "2026-08-30T13:00:00Z"),
            pr(461, "claude/nightly-457", "2026-08-30T14:00:00Z"),
        ];
        let found = residue(&ledger(&[324, 412, 457, 455]), &merged);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(ranks_csv(&found), "324,412,457");
        assert!(found[0].line().contains("PR #427"), "{}", found[0].line());
        assert!(found[0].line().contains("2026-08-30"), "{}", found[0].line());
    }

    /// 後始末が済んでいれば残骸ゼロ (台帳に無い順位は見ない)。
    #[test]
    fn a_cleaned_up_rank_is_not_residue() {
        let merged = vec![pr(427, "claude/nightly-324", "2026-08-30T12:22:36Z")];
        assert!(residue(&ledger(&[455]), &merged).is_empty());
        assert_eq!(ranks_csv(&[]), "");
    }

    /// 夜間ブランチでない PR は判定材料にしない (射程の明示)。
    #[test]
    fn non_nightly_branches_are_ignored() {
        let merged = vec![
            pr(468, "feat/nightly-ledger-removal-check", "2026-09-02T10:03:42Z"),
            pr(467, "docs/ledger-stale-rank-cleanup", "2026-09-02T07:10:19Z"),
        ];
        assert!(residue(&ledger(&[324, 455]), &merged).is_empty());
    }

    /// **派生ブランチ名を順位として読まない。**
    #[test]
    fn a_suffixed_branch_name_is_not_a_rank() {
        let merged = vec![pr(999, "claude/nightly-324-retry", "2026-09-01T00:00:00Z")];
        assert!(residue(&ledger(&[324]), &merged).is_empty());
    }

    /// 同じ順位に複数の PR があれば**新しい方**を根拠に出す (再投入されたタスク)。
    #[test]
    fn the_newest_merge_wins_for_a_reintroduced_rank() {
        let merged = vec![
            pr(100, "claude/nightly-324", "2026-08-01T00:00:00Z"),
            pr(427, "claude/nightly-324", "2026-08-30T12:22:36Z"),
        ];
        let found = residue(&ledger(&[324]), &merged);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].pr, 427);
    }

    /// 出力順は順位昇順 (入力の並びに依存させない — 報告の diff が読めなくなる)。
    #[test]
    fn output_is_sorted_by_rank() {
        let merged = vec![
            pr(461, "claude/nightly-457", "2026-08-30T14:00:00Z"),
            pr(427, "claude/nightly-324", "2026-08-30T12:22:36Z"),
        ];
        assert_eq!(ranks_csv(&residue(&ledger(&[324, 457]), &merged)), "324,457");
    }
}
