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
///
/// `head_oid` は PR の head commit。**PR を「ブランチ名」ではなく「name + commit」で
/// 束ねるために要る** ([`RemoteBranch`] の doc を参照)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrRecord {
    pub number: u64,
    pub head_ref: String,
    pub head_oid: String,
    pub state: PrState,
}

/// remote ブランチ 1 本。**名前と現在の commit を必ず組で運ぶ。**
///
/// # なぜ名前だけでは足りないか
///
/// PR の履歴は head ref **名**で永続する。ブランチが消えても、同名の ref を後から作れば
/// 過去の PR がそのまま紐づいて見える。初版は名前だけで束ねていたため、決着済み PR と
/// 同名の ref はすべて「その PR のブランチ」= 削除候補になっていた。
///
/// これが [ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 19 の**失敗マーカーを
/// 消していた**。マーカーは base commit を指す空 ref で、同じ順位で過去に PR が出ていれば
/// 名前が衝突する。実測: 順位 324 は PR [#427](https://github.com/aloekun/claude-code-hook-test/pull/427)
/// が 2026-08-30 にマージされた後、2026-08-31 / 09-01 の 2 晩とも「掃除 → 同じ順位を再選択 →
/// agent を 1 回まるごと回して空 diff → マーカー作成」を繰り返した。決定 20 は「境界は
/// 『PR があるか』の 1 点」としていたが、**一度 PR が出た順位では、その 1 点が常に真になる**。
///
/// commit を併せて見れば、マーカー (base commit) と PR の head は別物として区別できる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteBranch {
    pub name: String,
    pub sha: String,
}

/// ブランチ 1 本の分類結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchVerdict {
    /// trunk / 保護ブランチ。提案対象から常に外す。
    Protected,
    /// open (または解釈不能) な PR が紐づく。まだ作業中なので触らない。
    Active { open_prs: Vec<u64> },
    /// 紐づく PR がすべて closed / merged で、**そのうち 1 本は現在の ref を指している**。
    /// 削除提案の対象。
    Stale { closed_prs: Vec<u64> },
    /// 決着済み PR は紐づくが、**どれも現在の ref とは別の commit を指す**。
    /// **提案対象にしない** ([`RemoteBranch`] の doc を参照)。
    Diverged { settled_prs: Vec<u64> },
    /// PR が 1 件も無い。**提案対象にしない** (§ なぜ提案しないか を参照)。
    NoPullRequest,
}

/// 分類済みの 1 行。
///
/// **判定した時点の commit を必ず持ち回る。** 削除を実行するのは呼び手 (`cli-branch-cleanup`)
/// であり、名前だけを渡すと実行側が**自分で観測し直した** ref を消す。判定と実行の間に ref が
/// 動いていれば、それは分類していない別の物である ([`crate::main`] の module doc § 削除はしない)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedBranch {
    pub branch: String,
    /// 判定に使った commit (`git ls-remote` の観測値)。
    pub sha: String,
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
    remote_branches: &[RemoteBranch],
    prs: &[PrRecord],
    configured_trunk: Option<&str>,
) -> Vec<ClassifiedBranch> {
    let mut out: Vec<ClassifiedBranch> = remote_branches
        .iter()
        .map(|branch| ClassifiedBranch {
            branch: branch.name.clone(),
            sha: branch.sha.clone(),
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

fn verdict_for(
    branch: &RemoteBranch,
    prs: &[PrRecord],
    configured_trunk: Option<&str>,
) -> BranchVerdict {
    if is_protected(&branch.name, configured_trunk) {
        return BranchVerdict::Protected;
    }
    let all_prs = prs_for_branch(&branch.name, prs);
    if all_prs.is_empty() {
        return BranchVerdict::NoPullRequest;
    }
    let alive = prs_keeping_branch_alive(&branch.name, prs);
    if !alive.is_empty() {
        return BranchVerdict::Active { open_prs: alive };
    }
    if !a_pr_points_at(branch, prs) {
        return BranchVerdict::Diverged { settled_prs: all_prs };
    }
    BranchVerdict::Stale { closed_prs: all_prs }
}

/// 紐づく PR のいずれかが、**このブランチの現在の commit** を head にしているか。
///
/// **open PR には課さない。** open PR がブランチを守るのは名前の一致だけで足りる
/// ([`prs_keeping_branch_alive`])。open PR の `headRefOid` は push のたびに GitHub 側で
/// 更新されるが、その反映と本 scan の `ls-remote` の間には窓がある。ここで commit 一致を
/// 要求すると、**作業中のブランチが「PR に守られていない」側へ倒れる** — 誤りの向きが
/// 逆になるため、条件は決着済み PR の削除判定にだけ効かせる。
///
/// 空文字どうしを一致と読まない。取得層は空 SHA の行を捨て、`headRefOid` の欠損を `Err` に
/// するため通常は起こらないが、**空 == 空で「PR の head と同じ」に化ける**のは
/// 最も危ない誤りなので値の側でも塞ぐ。
fn a_pr_points_at(branch: &RemoteBranch, prs: &[PrRecord]) -> bool {
    !branch.sha.is_empty()
        && prs
            .iter()
            .any(|pr| pr.head_ref == branch.name && pr.head_oid == branch.sha)
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

/// テスト用の値づくり。`crate::main` 側の test module とも共有する。
///
/// **既定では「ブランチの現在 commit = その名前の PR の head」に揃える。** commit の一致は
/// 通常運用の姿 (PR を出したブランチがそのまま残っている) であり、既定を一致にしておけば
/// 各テストは「名前と状態」という本来の関心だけを書ける。**ずれている状況を見るテストは
/// SHA を明示的に渡す** — そこが本 module の新しい判定点なので、明示された箇所だけを読めば
/// 差が分かる。
#[cfg(test)]
pub(crate) mod test_support {
    use super::{PrRecord, PrState, RemoteBranch};

    /// ブランチ名から決定論的な「そのブランチの head commit」を作る。
    pub(crate) fn head_sha(branch: &str) -> String {
        format!("sha-of-{branch}")
    }

    pub(crate) fn pr(number: u64, head: &str, state: &str) -> PrRecord {
        PrRecord {
            number,
            head_ref: head.to_string(),
            head_oid: head_sha(head),
            state: PrState::parse(state),
        }
    }

    /// PR の head と同じ commit を指すブランチ (= 通常運用の姿)。
    pub(crate) fn branches(names: &[&str]) -> Vec<RemoteBranch> {
        names.iter().map(|name| at(name, &head_sha(name))).collect()
    }

    /// commit を明示するブランチ。ハンドオフマーカーのように PR の head と別の commit を
    /// 指す ref を作るために使う。
    pub(crate) fn at(name: &str, sha: &str) -> RemoteBranch {
        RemoteBranch { name: name.to_string(), sha: sha.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{at, branches, head_sha, pr};
    use super::*;

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

    /// **順位 324 の再現** ([`RemoteBranch`] の doc)。マージ済み PR #427 と同名の ref が、
    /// base commit を指すハンドオフマーカーとして後から作られた形。名前だけで束ねていた
    /// 頃はこれが `Stale` = 削除候補になり、2 晩にわたって同じ順位が再選択された。
    #[test]
    fn a_handoff_marker_sharing_a_name_with_a_merged_pr_is_not_proposed() {
        let marker = at("claude/nightly-324", "base-commit-of-that-night");
        let classified = classify(
            std::slice::from_ref(&marker),
            &[pr(427, "claude/nightly-324", "MERGED")],
            None,
        );
        assert_eq!(
            classified[0].verdict,
            BranchVerdict::Diverged { settled_prs: vec![427] }
        );
        assert!(
            deletion_candidates(&classified).is_empty(),
            "決着済み PR と同名なだけの ref を削除候補に出している"
        );
    }

    /// 対照: **同じ入力で commit だけを PR の head に揃えると `Stale` になる。**
    /// 上のテストが「PR 判定そのものが壊れたから通った」のではないことを固定する
    /// (掃除が一切効かなくなる方向の退行は、削除漏れとして静かに積み上がる)。
    #[test]
    fn the_same_branch_at_the_prs_head_is_still_proposed() {
        let at_head = at("claude/nightly-324", &head_sha("claude/nightly-324"));
        let classified = classify(
            std::slice::from_ref(&at_head),
            &[pr(427, "claude/nightly-324", "MERGED")],
            None,
        );
        assert_eq!(classified[0].verdict, BranchVerdict::Stale { closed_prs: vec![427] });
        assert_eq!(deletion_candidates(&classified).len(), 1);
    }

    /// **open PR は commit がずれていてもブランチを守る。** open PR の `headRefOid` は
    /// push のたびに更新されるため、`ls-remote` との間に窓がある。ここで一致を要求すると
    /// 作業中のブランチが提案対象へ落ちる ([`a_pr_points_at`] の doc)。
    #[test]
    fn an_open_pr_protects_the_branch_even_when_the_commit_moved() {
        let moved = at("feat/x", "just-pushed-commit");
        let classified = classify(std::slice::from_ref(&moved), &[pr(7, "feat/x", "OPEN")], None);
        assert_eq!(classified[0].verdict, BranchVerdict::Active { open_prs: vec![7] });
    }

    /// 決着済み PR が複数あり、**そのうち 1 本でも現在の commit を指していれば** `Stale`。
    /// close → 別 PR を開いて close、のように履歴が積もったブランチで、最後の PR の head に
    /// 留まっているものを掃除できなくしない。
    #[test]
    fn one_settled_pr_at_the_current_commit_is_enough_to_propose() {
        let branch = at("feat/x", "second-head");
        let prs = [
            pr(1, "feat/x", "CLOSED"),
            PrRecord {
                number: 2,
                head_ref: "feat/x".to_string(),
                head_oid: "second-head".to_string(),
                state: PrState::Closed,
            },
        ];
        assert_eq!(
            classify(std::slice::from_ref(&branch), &prs, None)[0].verdict,
            BranchVerdict::Stale { closed_prs: vec![1, 2] }
        );
    }

    /// 空 SHA どうしを一致と読まない。取得層が塞いでいる形だが、**空 == 空で「PR の head と
    /// 同じ」に化ける**のが最も危ない誤りなので値の側でも固める。
    #[test]
    fn an_empty_sha_never_counts_as_pointing_at_a_pr() {
        let empty = at("feat/x", "");
        let mut settled = pr(1, "feat/x", "CLOSED");
        settled.head_oid = String::new();
        let classified = classify(std::slice::from_ref(&empty), &[settled], None);
        assert_eq!(classified[0].verdict, BranchVerdict::Diverged { settled_prs: vec![1] });
        assert!(deletion_candidates(&classified).is_empty());
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
