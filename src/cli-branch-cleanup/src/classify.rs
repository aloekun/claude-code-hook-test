//! 掃除ループの結果分類 (I/O なし)。
//!
//! # なぜ exe へ移したか
//!
//! 掃除ループは `.github/workflows/nightly-todo.yml` の shell に 3 分岐で書かれていた。
//! 削除経路と lease 一致削除は 2026-08-22 の run 32589642740 で実走観測できたが、
//! **残りの分岐はどれも自然発火を期待できない** — skip 2 分岐と ref 移動は TOCTOU レース
//! (実測窓 約 1.3 秒) を要し、障害経路はネットワーク断・token 失効といった外部障害
//! (TOCTOU とは別の条件) を要する (順位 467 D-1)。
//!
//! [ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 1 は「回帰テストの場が
//! 無い判定を無人経路に置かない」と定めている。分類を純関数へ移し、レースを再現せずに
//! 全分岐をテストで固定する。
//!
//! # 意味論は現行 step のコメントが正
//!
//! App token / lease の意味は移送で変えない — `--force-with-lease` は「観測した SHA から
//! 動いていなければ削除する」であり、**動いていたら中止する**のが目的である
//! (他経路の作業を消さないため)。

/// `git ls-remote --heads <url> refs/heads/<branch>` の観測結果 (I/O なし)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RefObservation {
    /// ref が在り、先頭が指す SHA。
    Present(String),
    /// 出力が空 = ref が無い。
    Absent,
    /// コマンド自体が失敗した (ネットワーク / 認証)。
    Failed(String),
}

/// 削除 (lease 付き push) の結果 (I/O なし)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeleteAttempt {
    Succeeded,
    /// 失敗。`detail` は redact 済みの診断文字列。
    Failed(String),
}

/// 1 ブランチの処理結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 削除した。
    Deleted,
    /// 観測時点で既に無かった (前回の掃除や人手の削除)。
    SkippedAlreadyGone,
    /// 削除に失敗したが、再確認したら消えていた (レースの正常側)。
    SkippedVanishedDuringDelete,
    /// 観測後に ref が動いた。他経路の作業があるので**消さずに止める**。
    AbortedRefMoved(String),
    /// 削除は拒否されたが、ref は観測時の SHA のまま在る。**ref 移動ではない** —
    /// branch protection / 権限不足 / server-side hook はこの形になる。
    DeleteRejected(String),
    /// 観測 / 再確認そのものが失敗した (ネットワーク / 認証)。
    Failed(String),
}

impl Outcome {
    /// 掃除ループ全体を赤で終わらせるか。
    pub(crate) fn is_failure(&self) -> bool {
        matches!(
            self,
            Outcome::AbortedRefMoved(_) | Outcome::DeleteRejected(_) | Outcome::Failed(_)
        )
    }

    /// ログ 1 行 (redact 済みの入力しか受け取らない)。
    pub(crate) fn message(&self, branch: &str) -> String {
        match self {
            Outcome::Deleted => format!("削除: {branch}"),
            Outcome::SkippedAlreadyGone => format!("既に削除済みのため skip: {branch}"),
            Outcome::SkippedVanishedDuringDelete => {
                format!("削除直前に消えていたため skip: {branch}")
            }
            Outcome::AbortedRefMoved(detail) => format!(
                "削除を中止: {branch} (観測後に ref が動いた = 他経路の作業がある): {detail}"
            ),
            Outcome::DeleteRejected(detail) => format!(
                "削除を拒否された: {branch} (ref は観測時のまま。branch protection / 権限 / hook を疑う): {detail}"
            ),
            Outcome::Failed(detail) => format!("ref の確認に失敗: {branch}: {detail}"),
        }
    }
}

/// 観測 → 削除 → (失敗時のみ) 再確認 の結果を 1 つの [`Outcome`] にする (I/O なし)。
///
/// `delete` / `recheck` は、前段の結果によって呼ばれないことがあるため [`Option`] で受ける。
/// **呼ばれなかった段が `None`** であり、呼び出し側の I/O 層はこの契約に従って値を渡す。
pub(crate) fn classify(
    observation: &RefObservation,
    delete: Option<&DeleteAttempt>,
    recheck: Option<&RefObservation>,
) -> Outcome {
    match observation {
        RefObservation::Failed(detail) => Outcome::Failed(detail.clone()),
        RefObservation::Absent => Outcome::SkippedAlreadyGone,
        RefObservation::Present(observed) => match delete {
            Some(DeleteAttempt::Succeeded) => Outcome::Deleted,
            Some(DeleteAttempt::Failed(detail)) => {
                classify_after_failed_delete(observed, detail, recheck)
            }
            None => Outcome::Failed("削除を試行していません (呼び出し側の契約違反)".to_string()),
        },
    }
}

/// 削除が失敗した後の分岐。**再確認の結果でしか区別できない**。
///
/// **ref が在るだけでは「動いた」と言えない。** lease の失敗文言は「消えた」「動いた」
/// 「拒否された」を区別しない (実測: どれも `stale info`) ため ref の実在で判定するが、
/// branch protection / 権限不足 / server-side hook による拒否では **ref は観測時の SHA の
/// まま残る**。SHA まで見て初めて「他経路の作業がある」と言える (CodeRabbit #466)。
/// どちらも red で止める点は変わらない — 変えたのは人間に出す診断だけである。
fn classify_after_failed_delete(
    observed: &str,
    detail: &str,
    recheck: Option<&RefObservation>,
) -> Outcome {
    match recheck {
        Some(RefObservation::Absent) => Outcome::SkippedVanishedDuringDelete,
        Some(RefObservation::Present(current)) if current == observed => {
            Outcome::DeleteRejected(detail.to_string())
        }
        Some(RefObservation::Present(_)) => Outcome::AbortedRefMoved(detail.to_string()),
        Some(RefObservation::Failed(recheck_detail)) => Outcome::Failed(recheck_detail.clone()),
        None => Outcome::Failed("削除失敗後の再確認をしていません (呼び出し側の契約違反)".to_string()),
    }
}

/// `git ls-remote` の 1 行目から SHA を読む (I/O なし)。
///
/// 出力が空なら [`RefObservation::Absent`]。行は `<sha>\t<ref>` で、SHA だけを使う。
pub(crate) fn observe_from_output(first_line: &str) -> RefObservation {
    if first_line.trim().is_empty() {
        return RefObservation::Absent;
    }
    // NOTE: 行頭を trim しない。先頭 tab を落とすと、空の SHA 欄が ref 名に化ける。
    let sha = first_line.trim_end().split('\t').next().unwrap_or("");
    if is_object_id(sha) {
        RefObservation::Present(sha.to_string())
    } else {
        RefObservation::Failed(format!(
            "ls-remote の出力を解釈できません: {}",
            first_line.trim()
        ))
    }
}

/// git の object id の形か (16 進のみ、空でない)。
///
/// **形を確かめてから lease に渡す** — ref 名などが混ざったまま `--force-with-lease` の
/// 右辺へ入ると、意図しない比較になる。
fn is_object_id(candidate: &str) -> bool {
    !candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn present() -> RefObservation {
        RefObservation::Present("3000737e0c1a".to_string())
    }

    /// 通常経路: ref が在り、削除が成功する (2026-08-22 の run で実走観測済み)。
    #[test]
    fn a_present_ref_deleted_successfully_is_deleted() {
        let outcome = classify(&present(), Some(&DeleteAttempt::Succeeded), None);
        assert_eq!(outcome, Outcome::Deleted);
        assert!(!outcome.is_failure());
    }

    /// **skip 分岐 1**: 観測時点で既に無い。TOCTOU レースでしか自然発火しない経路。
    #[test]
    fn an_absent_ref_is_skipped() {
        let outcome = classify(&RefObservation::Absent, None, None);
        assert_eq!(outcome, Outcome::SkippedAlreadyGone);
        assert!(!outcome.is_failure());
    }

    /// **skip 分岐 2**: 削除は失敗したが、再確認したら消えていた (レースの正常側)。
    #[test]
    fn a_ref_that_vanished_during_delete_is_skipped() {
        let outcome = classify(
            &present(),
            Some(&DeleteAttempt::Failed("stale info".to_string())),
            Some(&RefObservation::Absent),
        );
        assert_eq!(outcome, Outcome::SkippedVanishedDuringDelete);
        assert!(!outcome.is_failure());
    }

    /// **中止**: 観測後に ref が動いた。他経路の作業があるので消さない。
    #[test]
    fn a_moved_ref_aborts_instead_of_deleting() {
        let outcome = classify(
            &present(),
            Some(&DeleteAttempt::Failed("stale info".to_string())),
            Some(&RefObservation::Present("beef1234".to_string())),
        );
        assert_eq!(outcome, Outcome::AbortedRefMoved("stale info".to_string()));
        assert!(outcome.is_failure(), "ref が動いたら赤で止める");
    }

    /// **拒否**: 削除は失敗したが ref は観測時の SHA のまま。branch protection / 権限不足 /
    /// server-side hook はこの形になる。**「他経路の作業がある」と誤診断しない**
    /// (CodeRabbit #466)。止めることは変わらない。
    #[test]
    fn a_rejected_delete_is_not_reported_as_a_moved_ref() {
        let outcome = classify(
            &present(),
            Some(&DeleteAttempt::Failed("protected branch".to_string())),
            Some(&present()),
        );
        assert_eq!(
            outcome,
            Outcome::DeleteRejected("protected branch".to_string())
        );
        assert!(outcome.is_failure(), "拒否も red で止める");
        let message = outcome.message("claude/nightly-1");
        assert!(!message.contains("動いた"), "{message}");
    }

    /// **障害経路 1**: 観測そのものが失敗した (ネットワーク / 認証)。
    #[test]
    fn a_failed_observation_is_a_failure() {
        let outcome = classify(
            &RefObservation::Failed("could not read Username".to_string()),
            None,
            None,
        );
        assert!(outcome.is_failure());
        assert!(outcome.message("b").contains("could not read Username"));
    }

    /// **障害経路 2**: 削除失敗後の再確認が失敗した。
    #[test]
    fn a_failed_recheck_is_a_failure() {
        let outcome = classify(
            &present(),
            Some(&DeleteAttempt::Failed("push failed".to_string())),
            Some(&RefObservation::Failed("timeout".to_string())),
        );
        assert_eq!(outcome, Outcome::Failed("timeout".to_string()));
        assert!(outcome.is_failure());
    }

    /// 呼び出し側が段を飛ばしたら失敗に倒す (「削除していないのに成功」を作らない)。
    #[test]
    fn missing_stages_are_failures_not_successes() {
        assert!(classify(&present(), None, None).is_failure());
        assert!(classify(
            &present(),
            Some(&DeleteAttempt::Failed("x".to_string())),
            None
        )
        .is_failure());
    }

    /// `git ls-remote` の実出力形式から SHA を読む。
    #[test]
    fn ls_remote_output_yields_the_sha() {
        assert_eq!(
            observe_from_output("3000737e0c1a\trefs/heads/claude/nightly-228"),
            RefObservation::Present("3000737e0c1a".to_string())
        );
    }

    /// 空出力は「ref が無い」であって失敗ではない。
    #[test]
    fn empty_ls_remote_output_means_absent() {
        assert_eq!(observe_from_output(""), RefObservation::Absent);
        assert_eq!(observe_from_output("   \n"), RefObservation::Absent);
    }

    /// 解釈できない出力は失敗に倒す (空と区別する)。
    #[test]
    fn unparsable_ls_remote_output_is_a_failure() {
        assert!(matches!(
            observe_from_output("\trefs/heads/x"),
            RefObservation::Failed(_)
        ));
    }

    /// ログ行に redact 前の URL が混ざらないこと (入力が redact 済みである契約の確認)。
    #[test]
    fn messages_only_contain_the_given_detail() {
        let outcome = Outcome::AbortedRefMoved("stale info: refs/heads/x".to_string());
        let message = outcome.message("claude/nightly-1");
        assert!(message.contains("claude/nightly-1"));
        assert!(message.contains("stale info"));
        assert!(!message.contains("x-access-token"));
    }
}
