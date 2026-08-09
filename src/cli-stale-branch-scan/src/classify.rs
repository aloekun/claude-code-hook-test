//! 残存ブランチ判定の純粋コア (順位 395、ADR-031 の機械層)。
//!
//! I/O を一切行わない。`git ls-remote` / `gh pr list` の実行と出力パースは [`crate::collect`]
//! が担い、本 module は読み取り済みの値だけを受け取って分類する。
//!
//! # なぜ「数える主体」と「判断する主体」を分けるか
//!
//! [ADR-067](../../../docs/adr/adr-067-phase-b-unattended-fix-push.md) / ADR-071 § 決定 4 と
//! 同型。外部コマンドに依存しない純関数にしておけば、GitHub 到達性・認証・実データの状態に
//! 依らず境界条件をテストで固定できる。実運用で踏むのは「PR が複数ある」「reopen された」
//! のような組み合わせで、それらは実 API を叩かずに再現したい。

/// PR の状態。`gh pr list --json state` の値に対応する。
///
/// **未知の値は [`PrState::Unknown`] へ落とし、`Open` と同じ扱い (= 保護側) にする。**
/// GitHub が state を追加したときに、解釈できない PR を持つブランチが「PR 無し」と誤判定
/// されて削除提案に載るのが最悪の失敗なので、曖昧さは残す側へ倒す。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
    Unknown,
}

impl PrState {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "OPEN" => PrState::Open,
            "CLOSED" => PrState::Closed,
            "MERGED" => PrState::Merged,
            _ => PrState::Unknown,
        }
    }

    /// この PR がブランチを「まだ生きている」側に留めるか。
    ///
    /// `Unknown` が `true` なのは上記のとおり保護側へ倒すため。
    fn keeps_branch_alive(self) -> bool {
        matches!(self, PrState::Open | PrState::Unknown)
    }
}

/// 1 件の PR。判定に要る最小限だけを持つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrRecord {
    pub number: u64,
    pub head_ref: String,
    pub state: PrState,
}

/// ブランチ 1 本の分類結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchVerdict {
    /// trunk / 保護ブランチ。提案対象から常に外す。
    Protected,
    /// open (または解釈不能) な PR が紐づく。まだ作業中なので触らない。
    Active { open_prs: Vec<u64> },
    /// 紐づく PR がすべて closed / merged。**削除提案の対象**。
    Stale { closed_prs: Vec<u64> },
    /// PR が 1 件も無い。**提案対象にしない** (§ なぜ提案しないか を参照)。
    NoPullRequest,
}

/// 分類済みの 1 行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedBranch {
    pub branch: String,
    pub verdict: BranchVerdict,
}

/// trunk 判定。ここに載る名前は何があっても削除提案に出さない。
///
/// `lib_jj_helpers::is_trunk_bookmark` の呼び出し (関数自体の共有ではない) で
/// `main`/`master`/`trunk`/`develop` を additive に保護する。関数を丸ごと共有しない
/// (呼ぶだけに留める) のは、あちらが jj の bookmark 名 (ローカル概念) を対象にするのに対し
/// こちらは remote ref 名を対象にするため。将来ずれる可能性のある 2 つの概念を 1 関数に
/// 束ねはしないが、trunk 名の**値**の出どころは 1 箇所 (`TRUNK_BOOKMARKS`) に揃える
/// (SIM-NEW-cli-stale-branch-scan-classify-L283: 独自 hardcode がここからずれて
/// `develop` 等の trunk を守れていなかった)。
///
/// `configured_trunk` は `push-runner-config.toml` の `default_branch` (未設定/読み取り
/// 失敗時は `None`)。`TRUNK_BOOKMARKS` に無い名前をリポジトリが trunk として設定していても
/// 保護対象に含める。
fn is_protected(branch: &str, configured_trunk: Option<&str>) -> bool {
    lib_jj_helpers::is_trunk_bookmark(branch)
        || branch == "HEAD"
        || configured_trunk == Some(branch)
}

/// remote ブランチ一覧と PR 一覧から、各ブランチの扱いを決める。
///
/// `configured_trunk` は [`is_protected`] 参照。
///
/// 出力はブランチ名の昇順で決定論的に並ぶ (同じ入力なら同じレポートになる)。
pub fn classify(
    remote_branches: &[String],
    prs: &[PrRecord],
    configured_trunk: Option<&str>,
) -> Vec<ClassifiedBranch> {
    let mut out: Vec<ClassifiedBranch> = remote_branches
        .iter()
        .map(|branch| ClassifiedBranch {
            branch: branch.clone(),
            verdict: verdict_for(branch, prs, configured_trunk),
        })
        .collect();
    out.sort_by(|a, b| a.branch.cmp(&b.branch));
    out.dedup_by(|a, b| a.branch == b.branch);
    out
}

/// ブランチを生かし続けている PR 番号を返す。
///
/// **1 本でも該当があればそのブランチは Active。** ブランチに複数 PR が紐づく形
/// (close 後に別 PR を開いた / reopen された) は実際に起こり、閉じた側だけを見て
/// 削除提案に載せると**作業中のブランチを消す提案**になる。
fn prs_keeping_branch_alive(branch: &str, prs: &[PrRecord]) -> Vec<u64> {
    sorted(
        prs.iter()
            .filter(|pr| pr.head_ref == branch && pr.state.keeps_branch_alive())
            .map(|pr| pr.number)
            .collect(),
    )
}

fn prs_for_branch(branch: &str, prs: &[PrRecord]) -> Vec<u64> {
    sorted(prs.iter().filter(|pr| pr.head_ref == branch).map(|pr| pr.number).collect())
}

fn verdict_for(branch: &str, prs: &[PrRecord], configured_trunk: Option<&str>) -> BranchVerdict {
    if is_protected(branch, configured_trunk) {
        return BranchVerdict::Protected;
    }
    let all_prs = prs_for_branch(branch, prs);
    if all_prs.is_empty() {
        return BranchVerdict::NoPullRequest;
    }
    let alive = prs_keeping_branch_alive(branch, prs);
    if !alive.is_empty() {
        return BranchVerdict::Active { open_prs: alive };
    }
    BranchVerdict::Stale { closed_prs: all_prs }
}

fn sorted(mut v: Vec<u64>) -> Vec<u64> {
    v.sort_unstable();
    v
}

/// 削除提案の対象だけを取り出す。
pub fn deletion_candidates(classified: &[ClassifiedBranch]) -> Vec<&ClassifiedBranch> {
    classified
        .iter()
        .filter(|c| matches!(c.verdict, BranchVerdict::Stale { .. }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(number: u64, head: &str, state: &str) -> PrRecord {
        PrRecord { number, head_ref: head.to_string(), state: PrState::parse(state) }
    }

    fn branches(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn verdict(branch: &str, prs: &[PrRecord]) -> BranchVerdict {
        classify(&branches(&[branch]), prs, None).remove(0).verdict
    }

    #[test]
    fn closed_and_merged_prs_leave_a_stale_branch() {
        assert_eq!(
            verdict("claude/nightly-203", &[pr(365, "claude/nightly-203", "CLOSED")]),
            BranchVerdict::Stale { closed_prs: vec![365] }
        );
        assert_eq!(
            verdict("feat/x", &[pr(1, "feat/x", "MERGED")]),
            BranchVerdict::Stale { closed_prs: vec![1] }
        );
    }

    #[test]
    fn an_open_pr_keeps_the_branch_active() {
        assert_eq!(
            verdict("feat/x", &[pr(7, "feat/x", "OPEN")]),
            BranchVerdict::Active { open_prs: vec![7] }
        );
    }

    /// ブランチに複数 PR が紐づく形 (close 後に開き直した / reopen)。
    /// **1 本でも open があれば Active**。閉じた側だけ見て消す提案を出さない。
    #[test]
    fn a_single_open_pr_outweighs_any_number_of_closed_ones() {
        let prs = [
            pr(1, "feat/x", "CLOSED"),
            pr(2, "feat/x", "MERGED"),
            pr(3, "feat/x", "OPEN"),
        ];
        assert_eq!(verdict("feat/x", &prs), BranchVerdict::Active { open_prs: vec![3] });
    }

    /// 未知の state は open 扱い (保護側)。GitHub が state を追加しても、解釈できない
    /// PR を持つブランチが削除提案に載らない。
    #[test]
    fn an_unparseable_state_protects_the_branch() {
        assert_eq!(
            verdict("feat/x", &[pr(9, "feat/x", "DRAFT_SOMETHING_NEW")]),
            BranchVerdict::Active { open_prs: vec![9] }
        );
        assert_eq!(PrState::parse(""), PrState::Unknown);
        assert_eq!(PrState::parse("open"), PrState::Unknown, "小文字は受理しない");
    }

    #[test]
    fn trunk_is_never_a_candidate() {
        for name in ["master", "main", "HEAD"] {
            assert_eq!(verdict(name, &[pr(1, name, "MERGED")]), BranchVerdict::Protected);
        }
    }

    /// `lib_jj_helpers::TRUNK_BOOKMARKS` にのみ含まれる trunk 名 (`trunk` / `develop`) も
    /// 保護対象になる (SIM-NEW-cli-stale-branch-scan-classify-L283 の回帰固定)。
    #[test]
    fn trunk_bookmarks_names_beyond_master_and_main_are_protected() {
        for name in ["trunk", "develop"] {
            assert_eq!(verdict(name, &[pr(1, name, "MERGED")]), BranchVerdict::Protected);
        }
    }

    /// `push-runner-config.toml` の `default_branch` (= `configured_trunk`) が
    /// `TRUNK_BOOKMARKS` に無い名前でも保護対象になる。
    #[test]
    fn configured_trunk_outside_trunk_bookmarks_is_protected() {
        let classified = classify(
            &branches(&["release"]),
            &[pr(1, "release", "MERGED")],
            Some("release"),
        );
        assert_eq!(classified[0].verdict, BranchVerdict::Protected);
    }

    /// PR が 1 件も無いブランチは提案しない。作業中の WIP や、まだ PR を開いていない
    /// ブランチを消す提案になるため (§ なぜ提案しないか)。
    #[test]
    fn a_branch_without_any_pr_is_not_proposed() {
        assert_eq!(verdict("wip/scratch", &[]), BranchVerdict::NoPullRequest);
        let classified = classify(&branches(&["wip/scratch"]), &[], None);
        assert!(deletion_candidates(&classified).is_empty());
    }

    /// `claude/nightly-*` を除外しない (2026-08-09 ユーザー判断)。除外すると
    /// クローズ済み夜間 PR のブランチが永久に残り、同じ順位が選べなくなる。
    #[test]
    fn nightly_branches_are_included_not_excluded() {
        let classified = classify(
            &branches(&["claude/nightly-203"]),
            &[pr(365, "claude/nightly-203", "CLOSED")],
            None,
        );
        let candidates = deletion_candidates(&classified);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].branch, "claude/nightly-203");
    }

    /// 出力はブランチ名昇順で決定論的。同じ入力なら同じレポートになる
    /// (週次で diff を取る運用のため)。
    #[test]
    fn output_is_sorted_and_deduplicated() {
        let classified = classify(
            &branches(&["zeta", "alpha", "alpha", "master"]),
            &[pr(1, "zeta", "CLOSED"), pr(2, "alpha", "CLOSED")],
            None,
        );
        let names: Vec<&str> = classified.iter().map(|c| c.branch.as_str()).collect();
        assert_eq!(names, vec!["alpha", "master", "zeta"]);
    }

    /// 他ブランチの PR は判定に混ざらない (prefix 一致ではなく完全一致)。
    #[test]
    fn pr_matching_is_exact_not_prefix() {
        assert_eq!(
            verdict("feat/x", &[pr(1, "feat/x-extended", "OPEN")]),
            BranchVerdict::NoPullRequest
        );
    }
}
