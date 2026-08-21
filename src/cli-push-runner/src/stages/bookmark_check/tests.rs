//! bookmark_check stage のテスト (production は ../bookmark_check.rs)。
//! ファイル長 800 行ガイドライン (順位 147) 遵守のため、順位 288(b) の
//! 回帰テスト追加にあたって diff stage と同じく test mod を切り出した。

use super::*;

#[test]
fn parse_non_trunk_typical_output() {
    let output = "\
feat/xyz: abc1234 add feature
  @origin: abc1234 add feature
main: def5678 initial
  @origin: def5678 initial
";
    assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/xyz"]);
}

#[test]
fn parse_non_trunk_multiple_feature_bookmarks() {
    let output = "\
feat/a: 111 desc
feat/b: 222 desc
main: 333 desc
";
    assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/a", "feat/b"]);
}

#[test]
fn parse_non_trunk_only_trunk_returns_empty() {
    let output = "main: abc123 desc\nmaster: def456 desc\n";
    assert!(parse_non_trunk_bookmarks(output).is_empty());
}

#[test]
fn parse_non_trunk_empty_output_returns_empty() {
    assert!(parse_non_trunk_bookmarks("").is_empty());
}

#[test]
fn parse_non_trunk_skips_indented_remote_lines() {
    let output = "\
feat/xyz: abc1234 desc
  @origin: abc1234 desc
  @upstream: abc1234 desc
";
    assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/xyz"]);
}

#[test]
fn parse_non_trunk_filters_out_master_and_main() {
    let output = "\
feat/branch1: abc desc
master: def desc
feat/branch2: ghi desc
main: jkl desc
";
    assert_eq!(
        parse_non_trunk_bookmarks(output),
        vec!["feat/branch1", "feat/branch2"]
    );
}

#[test]
fn parse_non_trunk_handles_single_feature_bookmark() {
    let output = "feat/single: abc desc\n";
    assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/single"]);
}

/// `classify_head_state` (SIM-NEW-bookmark_check-L165 対応): jj 実行結果から
/// `HeadState` への分類を jj subprocess から切り離して直接検証する。
/// `query_head_state()`/`working_copy_is_empty()` 自体は実 jj repo が要るため
/// 単体テストできないが、fail closed 判定の核心はこの分類ロジックにある。
#[test]
fn classify_head_state_maps_ok_true_to_empty() {
    assert_eq!(classify_head_state(Ok(true)), HeadState::Empty);
}

#[test]
fn classify_head_state_maps_ok_false_to_not_empty() {
    assert_eq!(classify_head_state(Ok(false)), HeadState::NotEmpty);
}

#[test]
fn classify_desc_state_maps_described_to_not_empty() {
    assert_eq!(classify_desc_state(Ok(true)), HeadState::NotEmpty);
}

#[test]
fn classify_desc_state_maps_descless_to_descless() {
    assert_eq!(classify_desc_state(Ok(false)), HeadState::Descless);
}

/// CodeRabbit #431: 説明判定の失敗は push を止めるが、`@` が空とは案内しない。
/// `Unknown` (空判定自体の失敗) と混ぜると「`@` が空です」と事実と異なる案内になる。
#[test]
fn classify_desc_state_maps_err_to_desc_unknown_fail_closed() {
    assert_eq!(
        classify_desc_state(Err("timeout".to_string())),
        HeadState::DescUnknown,
        "説明判定の失敗は push を止める側へ倒す (Won't push の分かりにくい失敗より早期案内)"
    );
}

#[test]
fn decide_desc_unknown_does_not_claim_empty_working_copy() {
    let outcome = decide_bookmark_check(Vec::new(), HeadState::DescUnknown, || {
        panic!("DescUnknown では @- 照会に進まないこと (空前提の案内をしない)")
    });
    assert_eq!(
        outcome,
        BookmarkCheckOutcome::UndeterminedWorkingCopy,
        "空判定は成功しているので EmptyWorkingCopy に倒してはいけない"
    );
}

/// 順位 386: 説明なし `@` は bookmark の有無より先に Descless へ倒し、
/// `jj bookmark create -r @` (push 不能 bookmark の作成) へ誤誘導しない。
#[test]
fn decide_descless_head_yields_descless_outcome_before_bookmark_checks() {
    let outcome = decide_bookmark_check(Vec::new(), HeadState::Descless, || {
        panic!("Descless では @- 照会に進まないこと")
    });
    assert_eq!(outcome, BookmarkCheckOutcome::DesclessWorkingCopy);

    let outcome = decide_bookmark_check(
        vec!["feat/x".to_string()],
        HeadState::Descless,
        || panic!("Descless では @- 照会に進まないこと"),
    );
    assert_eq!(
        outcome,
        BookmarkCheckOutcome::DesclessWorkingCopy,
        "bookmark があっても説明なし @ は push させない"
    );
}

#[test]
fn classify_head_state_maps_err_to_unknown_fail_closed() {
    assert_eq!(
        classify_head_state(Err("jj bookmark list タイムアウト (30s)".to_string())),
        HeadState::Unknown
    );
}

/// T8 incident 再現テスト群 (ADR-049 の流儀: 1 test = 1 failure mode + good/bad)。
///
/// 由来 incident: PR #279 (T1) の dogfood push で発火した以下の状態。
///
/// ```text
/// @   zxxkpomz (empty) "WIP: next work"      ← 空の working copy
/// @-  nvmysvqk perf/lint-screen-evals-opt-in ← bookmark はここ
/// ```
///
/// `advance_jj_bookmarks` が「bookmark を `@-` に自動更新」と報告した直後に、
/// bookmark_check が `@` 厳密一致で「bookmark が見つかりません」と報告し、
/// `jj bookmark create <name> -r @` (= 空コミットに bookmark を付ける破壊的操作)
/// へ誤誘導していた。`docs/push-pipeline-fix-plan.md` §4 T8 の再現記録が仕様。
mod t8_empty_head_misdirection {
    use super::*;

    fn parent_without_bookmarks() -> ParentState {
        ParentState::Available {
            bookmarks: Vec::new(),
        }
    }

    fn parent_with(name: &str) -> ParentState {
        ParentState::Available {
            bookmarks: vec![name.to_string()],
        }
    }

    /// incident 再現 (bad): `@` が空 + bookmark が `@-`。
    /// 「bookmark 皆無」(= 作成案内が正しいケース) と取り違えてはならない。
    #[test]
    fn decide_empty_head_with_parent_bookmark_is_not_no_bookmarks() {
        let outcome = decide_bookmark_check(Vec::new(), HeadState::Empty, || {
            parent_with("perf/lint-screen-evals-opt-in")
        });
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::EmptyWorkingCopy {
                parent: parent_with("perf/lint-screen-evals-opt-in")
            }
        );
    }

    /// 前段の別症状 (good): bookmark が皆無かつ `@` が空でない場合は
    /// 既存の作成案内が正しいので `NoBookmarks` のまま維持する。
    #[test]
    fn decide_no_bookmarks_when_head_is_not_empty() {
        let outcome =
            decide_bookmark_check(Vec::new(), HeadState::NotEmpty, parent_without_bookmarks);
        assert_eq!(outcome, BookmarkCheckOutcome::NoBookmarks);
    }

    /// `@` が空 + `@-` にも bookmark が無い場合も push 不可。
    #[test]
    fn decide_empty_head_without_parent_bookmark_reports_empty_working_copy() {
        let outcome =
            decide_bookmark_check(Vec::new(), HeadState::Empty, parent_without_bookmarks);
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::EmptyWorkingCopy {
                parent: parent_without_bookmarks()
            }
        );
    }

    /// 既存の成功経路 (good): `@` が非空で bookmark があれば続行する。
    #[test]
    fn decide_proceeds_when_head_is_not_empty_and_has_bookmark() {
        let outcome =
            decide_bookmark_check(vec!["feat/xyz".to_string()], HeadState::NotEmpty, || {
                panic!("`@` が非空なら @- を照会してはならない")
            });
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::Proceed(vec!["feat/xyz".to_string()])
        );
    }

    /// PR #280 CodeRabbit Major: bookmark が空の `@` に付いていても中断する。
    /// 続行すると `jj diff -r @` が空になり、祖先の未 push 変更が AI レビューを
    /// 経ずに push される (レビューバイパス)。
    #[test]
    fn decide_empty_head_with_bookmark_at_head_still_aborts() {
        let outcome = decide_bookmark_check(
            vec!["feat/xyz".to_string()],
            HeadState::Empty,
            parent_without_bookmarks,
        );
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::EmptyWorkingCopy {
                parent: parent_without_bookmarks()
            }
        );
    }

    /// SIM-NEW-bookmark_check-L165 再現テスト (bad→fixed): `working_copy_is_empty()`
    /// が jj 不調で失敗し `HeadState::Unknown` になった場合でも、bookmark が `@` に
    /// 付いていれば以前は fail-open で `Proceed` に流れ込み、レビューバイパスを
    /// 再生産していた。fail closed に直した今は `Unknown` も `Empty` と同じく中断する。
    #[test]
    fn decide_unknown_head_state_with_bookmark_at_head_still_aborts() {
        let outcome = decide_bookmark_check(
            vec!["feat/xyz".to_string()],
            HeadState::Unknown,
            parent_without_bookmarks,
        );
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::EmptyWorkingCopy {
                parent: parent_without_bookmarks()
            }
        );
    }

    /// 2 ケースの取り違えを防ぐ核心: 案内文が bookmark の所在 (`@-`) を名指しする。
    #[test]
    fn summary_names_the_parent_bookmark_so_the_two_cases_are_distinguishable() {
        let summary = empty_working_copy_summary(&parent_with("perf/xyz"));
        assert!(summary.contains("perf/xyz"), "summary was: {}", summary);
        assert!(summary.contains("@-"), "summary was: {}", summary);
    }

    #[test]
    fn summary_without_parent_bookmark_omits_bookmark_name() {
        let summary = empty_working_copy_summary(&parent_without_bookmarks());
        assert!(summary.contains("空"), "summary was: {}", summary);
    }

    /// PR #280 CodeRabbit Major: `@-` を確認できないのに `jj edit @-` を案内しない。
    #[test]
    fn hint_does_not_advise_editing_parent_when_parent_is_unavailable() {
        let hint = empty_working_copy_hint(&ParentState::Unavailable);
        assert!(!hint.contains("jj edit @-"), "hint was: {}", hint);
    }

    /// `@-` を確認できた場合は実証済みの回避策 `jj edit @-` を案内する。
    #[test]
    fn hint_advises_editing_parent_when_parent_is_available() {
        let hint = empty_working_copy_hint(&parent_with("perf/xyz"));
        assert!(hint.contains("jj edit @-"), "hint was: {}", hint);
    }

    /// PR #280 simplicity-review warning: `@-` に bookmark が無い場合、`jj edit @-`
    /// だけでは次に `NoBookmarks` で止まるため、bookmark 作成まで案内する。
    #[test]
    fn hint_also_advises_creating_bookmark_when_parent_has_none() {
        let hint = empty_working_copy_hint(&parent_without_bookmarks());
        assert!(hint.contains("jj edit @-"), "hint was: {}", hint);
        assert!(hint.contains("jj bookmark create"), "hint was: {}", hint);
    }

    /// `@-` に bookmark がある場合は移動だけで足りるので、作成案内は出さない。
    #[test]
    fn hint_omits_bookmark_creation_when_parent_already_has_one() {
        let hint = empty_working_copy_hint(&parent_with("perf/xyz"));
        assert!(!hint.contains("jj bookmark create"), "hint was: {}", hint);
    }

    /// 親を確認できない場合の summary も `@-` の所在を騙らない。
    #[test]
    fn summary_when_parent_unavailable_does_not_claim_a_parent_bookmark() {
        let summary = empty_working_copy_summary(&ParentState::Unavailable);
        assert!(summary.contains("空"), "summary was: {}", summary);
        assert!(
            !summary.contains("@- にあります"),
            "summary was: {}",
            summary
        );
    }
}

/// 順位 288(b): `jj bookmark list` 失敗時の fail-open 穴 ([ADR-043])。
///
/// 旧実装は失敗時に `Some(空)` を返して push を続行していた。空リストでは
/// `push` stage が `-b <name>` を組み立てられず bare `jj git push` になり、
/// jj 0.42 の既定は tracked bookmark を全件送る = レビュー範囲
/// (`<default_branch>..@`) 外のコミットが AI レビューを経ずに push される
/// (ADR-045 事故で `--all` を廃止したのと同じ経路)。加えて `@` の空判定 /
/// description 判定の fail-closed 分岐が一度も評価されない。
mod rank288b_bookmark_list_failure {
    use super::*;
    use std::cell::Cell;

    fn bookmark_at_head() -> String {
        "feat/xyz: abc1234 add feature\n".to_string()
    }

    /// incident 再現 (bad): list が失敗したら push を止める。
    #[test]
    fn list_failure_aborts_instead_of_proceeding_with_empty_list() {
        let outcome = decide_from_bookmark_list(
            Err("jj bookmark list タイムアウト (30s)".to_string()),
            || HeadState::NotEmpty,
            parent_unavailable,
        );
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::BookmarkListUnavailable {
                reason: "jj bookmark list タイムアウト (30s)".to_string()
            }
        );
        assert_eq!(report_outcome(outcome), None);
    }

    /// 対 (good): list が成功していれば従来どおり push に進む。
    /// fail-closed 化が正常系まで巻き込んでいないことの対照。
    #[test]
    fn list_success_still_proceeds() {
        let outcome = decide_from_bookmark_list(
            Ok(bookmark_at_head()),
            || HeadState::NotEmpty,
            parent_unavailable,
        );
        assert_eq!(
            outcome,
            BookmarkCheckOutcome::Proceed(vec!["feat/xyz".to_string()])
        );
    }

    /// list 失敗時に `@` の状態を照会しない。中断は確定しており、不調な jj を
    /// もう一度叩いても案内は変わらない (照会結果で案内を出し分けると、
    /// 「`@` が空です」等の事実と異なる案内を再生産する余地が生まれる)。
    #[test]
    fn list_failure_does_not_query_head_state() {
        let queried = Cell::new(false);
        let _ = decide_from_bookmark_list(
            Err("boom".to_string()),
            || {
                queried.set(true);
                HeadState::NotEmpty
            },
            parent_unavailable,
        );
        assert!(!queried.get(), "head state must not be queried on failure");
    }

    /// 案内が「なぜ止めるか」= 実害を述べること。jj エラーの転記だけだと、
    /// 読んだ人が override 手段を探す方向に動く (バイパスは用意していない)。
    #[test]
    fn hint_states_the_actual_harm_not_just_the_jj_error() {
        let (summary, hint) = abort_report(BookmarkCheckOutcome::BookmarkListUnavailable {
            reason: "boom".to_string(),
        });
        assert!(summary.contains("boom"), "summary was: {}", summary);
        assert!(hint.contains("レビュー"), "hint was: {}", hint);
    }

    /// `Some` を返すとき中身は必ず 1 件以上、という不変条件
    /// (`build_push_command` が `-b` を付けられる前提)。
    #[test]
    fn proceed_never_carries_an_empty_bookmark_list() {
        let outcome = decide_from_bookmark_list(
            Ok(bookmark_at_head()),
            || HeadState::NotEmpty,
            parent_unavailable,
        );
        let detected = report_outcome(outcome).expect("proceed");
        assert!(!detected.is_empty());
    }

    fn parent_unavailable() -> ParentState {
        ParentState::Unavailable
    }
}
