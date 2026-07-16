//! Bookmark check stage — 順位 2 (PR #85 T1-3)
//!
//! `jj git push` は bookmark が必要だが、jj 環境では新規ブランチで bookmark を
//! 作成し忘れる落とし穴がある (PR #85 で初回 `pnpm push` が bookmark 未設定 →
//! `Nothing changed` で終了し、158s かけた quality_gate + takt review が無駄に
//! なった実証ベース)。本 stage は pipeline 最早期 (`scratch_file_warning` の前)
//! で `jj bookmark list` を確認し、非 trunk bookmark が無ければ即 error 終了して
//! 後続 stage の無駄実行を防ぐ。
//!
//! Stage 配置: `run_pipeline` の最早期 (scratch_file_warning の前)。bookmark 不在
//! は push 自体が不可能な状態のため、最優先で fail-fast する。
//!
//! 中断理由は 2 ケースあり、案内文を出し分ける (T8 / `BookmarkCheckOutcome`)。
//! 両者を同じ文面に潰すと、`@` が空のときに `jj bookmark create -r @` (= 空コミットに
//! bookmark を付ける破壊的操作) へ誤誘導する。中断メッセージは本 stage が出力し、
//! `main.rs` 側では重複させない。
//!
//! fail-open: `jj bookmark list` 実行失敗 (timeout / 起動失敗) 時は warning ログ
//! のみで push を続行する。jj 不調で push 自体を止めない設計。
//!
//! 設計上の non-config: `jj git push` は bookmark を必須とする仕様で、本 stage を
//! バイパスする正当な use case は存在しない。よって `[bookmark_check]` config
//! section は追加せず、常に有効。

use std::process::Command;

use lib_jj_helpers::is_trunk_bookmark;

use super::push_jj_bookmark::{advance_jj_bookmarks, working_copy_is_empty};
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;

/// `@` が空だった場合に bookmark の所在を診断する revset (T8)。
/// `advance_jj_bookmarks` の前進先と同一 (`working_copy_is_empty` が真のときの target)。
const PARENT_REVSET: &str = "@-";

/// bookmark 検出の対象 revset: **現在の workspace の `@` が指す bookmark のみ**
/// (順位 290 / PR #269・#271 CodeRabbit Major)。
///
/// 設計判断 (PR #271 で確定): bookmark の「所有権 (どの workspace のものか)」は
/// **履歴 (revset) から復元できない**。`::@ ~ ::trunk()` (自ブランチ線) を試みたが、
/// 他 workspace が作った trunk 未マージのコミットの上で作業すると、そのコミットを指す
/// 他 workspace の bookmark が `::@` に混入する (CodeRabbit Major)。revset での所有権推定を
/// 諦め、push stage の `-b` 付与対象を「今 push したい作業 = `@` に付いた bookmark」に
/// 限定する。これにより他 workspace の bookmark 混入を構造的に排除する (安全側)。
///
/// トレードオフ: stacked bookmark (feature/base → feature/api → feature/ui を `@` 先頭で
/// 一括 push) の運用では `@` の bookmark だけでは不足する。ただし現状その運用実績はなく、
/// 必要になった時点で明示オプトインの stack push モード (`[push] stack_push` 等) を追加する
/// 拡張余地を残す (todo 登録済み)。所有権を厳密に扱うには bookmark/workspace の別 metadata
/// 管理が必要だが、現用途では過剰。
const OWN_WORKSPACE_BOOKMARKS_REVSET: &str = "@";

/// `OWN_WORKSPACE_BOOKMARKS_REVSET` (`@` 厳密一致) で bookmark 存在を検査する前に、
/// `advance_jj_bookmarks()` (push stage が使う既存の前進処理と同一) で `@` より手前に
/// 残っている bookmark を前進させる (simplicity review 指摘対応: takt fix / 手動
/// `jj describe` で `@` が bookmark より先に進んだ状態のまま `pnpm push` を再実行すると、
/// advance 前に厳密一致で検査してしまい push stage の自動修復が走る前に pipeline が
/// 中断していた)。`None` = 非 trunk bookmark が無く push 不可 (pipeline 中断)。
pub(crate) fn run_bookmark_check() -> Option<Vec<String>> {
    advance_lagging_bookmark();
    detect_own_workspace_bookmarks()
}

/// `advance_jj_bookmarks()` を実行し、失敗時は fail-open で警告ログのみ出す
/// (advance はあくまで検査精度を上げるための前処理で、失敗しても検査自体は続行する)。
fn advance_lagging_bookmark() {
    if let Err(e) = advance_jj_bookmarks() {
        log_info(&format!(
            "bookmark_check: bookmark 自動更新失敗、検査を続行します: {}",
            e
        ));
    }
}

/// `jj bookmark list` で非 trunk なローカル bookmark の存在を確認し、
/// 検出した bookmark 名を返す。`None` = 非 trunk bookmark が無く push 不可 (pipeline 中断)。
///
/// 検出した名前は push stage の `-b <name>` 組み立てに使う (ADR-045 事故 follow-up:
/// `--all` push が他 workspace の bookmark を巻き込む問題の対策)。
///
/// fail-open: jj 実行失敗時は warning ログのみで `Some(空)` を返し、push 自体は止めない
/// (push stage は空リストなら base コマンドをそのまま実行する)。
fn detect_own_workspace_bookmarks() -> Option<Vec<String>> {
    let raw = match run_jj_bookmark_list(OWN_WORKSPACE_BOOKMARKS_REVSET) {
        Ok(output) => output,
        Err(e) => {
            log_info(&format!(
                "bookmark_check: jj bookmark list 失敗、検査を skip して push を続行します: {}",
                e
            ));
            return Some(Vec::new());
        }
    };
    let outcome = decide_bookmark_check(
        parse_non_trunk_bookmarks(&raw),
        query_head_state(),
        query_parent_state,
    );
    report_outcome(outcome)
}

/// `@-` の状態を照会する。照会失敗と「親はあるが bookmark 無し」を潰さない
/// (PR #280 CodeRabbit Major): 潰すと `@-` の存在を確認できていないのに
/// `jj edit @-` を案内してしまい、T8 で直したはずの「実行不能な案内」を再生産する。
fn query_parent_state() -> ParentState {
    match run_jj_bookmark_list(PARENT_REVSET) {
        Ok(raw) => ParentState::Available {
            bookmarks: parse_non_trunk_bookmarks(&raw),
        },
        Err(e) => {
            log_info(&format!(
                "bookmark_check: @- の照会に失敗、親を確認できないものとして案内します: {}",
                e
            ));
            ParentState::Unavailable
        }
    }
}

/// `@` の空判定結果。判定不能 (jj 実行失敗) を「空」「空でない」のどちらにも潰さない
/// (SIM-NEW-bookmark_check-L165 対応: `ParentState` と同じ流儀)。
#[derive(Debug, PartialEq)]
enum HeadState {
    /// `@` は空でない。
    NotEmpty,
    /// `@` は空。
    Empty,
    /// 判定に失敗した (jj 不調)。`decide_bookmark_check` は `Empty` と同じ扱いにし
    /// push を止める ([ADR-043] fail-closed): 「空でない」に倒すと、bookmark が
    /// 空の `@` に残っているケースで `Proceed` に流れ込み、PR #280 で塞いだ
    /// レビューバイパス (祖先の未レビュー変更が push される) を再生産する。
    Unknown,
}

/// `working_copy_is_empty()` の実行結果を `HeadState` に分類する。jj 実行から
/// 切り離して単体テスト可能にする (`decide_bookmark_check` と同じ流儀)。
fn classify_head_state(result: Result<bool, String>) -> HeadState {
    match result {
        Ok(true) => HeadState::Empty,
        Ok(false) => HeadState::NotEmpty,
        Err(e) => {
            log_info(&format!(
                "bookmark_check: @ の空判定に失敗、fail closed で空として扱います: {}",
                e
            ));
            HeadState::Unknown
        }
    }
}

/// `@` の空判定を照会する。判定不能時は fail closed で `HeadState::Unknown` を返す。
fn query_head_state() -> HeadState {
    classify_head_state(working_copy_is_empty())
}

/// `@` が空だったときの `@-` の状態。`jj edit @-` を案内してよいかを決める。
#[derive(Debug, PartialEq)]
enum ParentState {
    /// `@-` の照会に失敗した (root commit で親が無い / jj 不調)。存在を確認できて
    /// いないので `jj edit @-` は案内しない。
    Unavailable,
    /// `@-` は存在する。`bookmarks` = そこにある非 trunk bookmark (空もあり得る)。
    Available { bookmarks: Vec<String> },
}

/// bookmark_check の判定結果。jj 実行から切り離して単体テスト可能にする
/// (`dispatch_bookmark_advance` と同じ closure 注入の流儀)。
#[derive(Debug, PartialEq)]
enum BookmarkCheckOutcome {
    /// `@` が非空で非 trunk bookmark があり push 可能。
    Proceed(Vec<String>),
    /// `@` が空で push 不可 (T8 incident)。
    EmptyWorkingCopy { parent: ParentState },
    /// `@` は空でないが bookmark が無い。作成案内が正しいケース。
    NoBookmarks,
}

/// push 可否を 3 ケースに切り分ける (T8)。
///
/// **`@` の空判定を最優先する** (PR #280 CodeRabbit Major)。レビュー対象の diff は
/// `[diff] command = "jj diff -r @"` で取得するため、`@` が空のまま push すると
/// 祖先の未 push 変更が AI レビューを経ずにリモートへ出る。bookmark が空の `@` に
/// 付いていても同じ穴が開くため、bookmark の有無より先に `@` の空を弾く
/// (`advance_jj_bookmarks` は非 trunk bookmark が 2 つ以上あると fallback 更新を
/// skip するため、bookmark が空の `@` に残る状態は実在する)。
///
/// `head_state` は `Empty` だけでなく `Unknown` (jj 実行失敗で判定不能) でも
/// `EmptyWorkingCopy` に倒す (SIM-NEW-bookmark_check-L165 対応)。`Unknown` を
/// `NotEmpty` 側に倒すと、bookmark が空の `@` に残っているケースで `Proceed` に
/// 流れ込み、上記のレビューバイパスを再生産するため、判定不能自体が
/// push を止めるかどうかの分岐に直接影響する ([ADR-043] fail-closed)。
fn decide_bookmark_check(
    bookmarks_at_head: Vec<String>,
    head_state: HeadState,
    parent: impl FnOnce() -> ParentState,
) -> BookmarkCheckOutcome {
    if head_state != HeadState::NotEmpty {
        return BookmarkCheckOutcome::EmptyWorkingCopy { parent: parent() };
    }
    if bookmarks_at_head.is_empty() {
        return BookmarkCheckOutcome::NoBookmarks;
    }
    BookmarkCheckOutcome::Proceed(bookmarks_at_head)
}

fn report_outcome(outcome: BookmarkCheckOutcome) -> Option<Vec<String>> {
    match outcome {
        BookmarkCheckOutcome::Proceed(bookmarks) => {
            log_stage(
                "bookmark",
                &format!(
                    "非 trunk bookmark 検出 ({} 件): {}",
                    bookmarks.len(),
                    bookmarks.join(", ")
                ),
            );
            Some(bookmarks)
        }
        BookmarkCheckOutcome::EmptyWorkingCopy { parent } => {
            log_stage("bookmark", &empty_working_copy_summary(&parent));
            log_info(&empty_working_copy_hint(&parent));
            None
        }
        BookmarkCheckOutcome::NoBookmarks => {
            log_stage("bookmark", "ローカル bookmark (非 trunk) が見つかりません");
            log_info(
                "  push 不可: `jj git push` は bookmark が必要です。\n  \
                 対処: `jj bookmark create <name> -r @` で bookmark を作成して再実行してください\n  \
                 例: `jj bookmark create feat/my-feature -r @`",
            );
            None
        }
    }
}

fn empty_working_copy_summary(parent: &ParentState) -> String {
    match parent {
        ParentState::Unavailable => "`@` が空です (親コミットを確認できません)".to_string(),
        ParentState::Available { bookmarks } if bookmarks.is_empty() => "`@` が空です".to_string(),
        ParentState::Available { bookmarks } => format!(
            "`@` が空です (bookmark は @- にあります: {})",
            bookmarks.join(", ")
        ),
    }
}

/// `@` が空のときの対処案内。`@-` の存在を確認できた場合にのみ `jj edit @-` を案内する
/// (PR #280 CodeRabbit Major: 実行不能な案内を出さない)。
///
/// `@-` に bookmark が無い場合は `jj edit @-` だけでは push 可能にならない
/// (次は `NoBookmarks` で止まる) ため、bookmark 作成まで含めて 1 度に案内する
/// (PR #280 simplicity-review warning: 根本解決にならない案内を出さない)。
fn empty_working_copy_hint(parent: &ParentState) -> String {
    let reason = "  push 不可: レビュー対象の diff は `@` から取得するため、`@` が空のままでは\n  \
                  AI レビューが skip されたまま push されます。\n";
    let abandon_note =
        "  (不要になった空の WIP コミットは `jj abandon <change_id>` で削除できます)";
    match parent {
        ParentState::Unavailable => format!(
            "{}  対処: push する変更を `@` に作成するか、`jj edit <change_id>` で既存の\n  \
             コミットへ移動してから再実行してください",
            reason
        ),
        ParentState::Available { bookmarks } if bookmarks.is_empty() => format!(
            "{}  対処: `jj edit @-` で `@` を 1 つ前のコミットへ移動し、\n  \
             `jj bookmark create <name> -r @` で bookmark を作成してから再実行してください\n{}",
            reason, abandon_note
        ),
        ParentState::Available { .. } => format!(
            "{}  対処: `jj edit @-` で `@` を 1 つ前のコミットへ移動して再実行してください\n{}",
            reason, abandon_note
        ),
    }
}

fn parse_non_trunk_bookmarks(raw: &str) -> Vec<String> {
    raw.lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('\t'))
        .filter_map(|line| line.split(':').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
        .collect()
}

fn run_jj_bookmark_list(revset: &str) -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(["bookmark", "list", "-r", revset])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj bookmark list 起動失敗: {}", e))?;

    let stdout_handle = lib_subprocess::drain_pipe_unlimited(
        child.stdout.take().expect("stdout must be piped"),
    );
    let stderr_handle = lib_subprocess::drain_pipe_unlimited(
        child.stderr.take().expect("stderr must be piped"),
    );

    let status =
        lib_subprocess::wait_with_timeout_basic("jj bookmark list", &mut child, JJ_TIMEOUT_SECS)
            .map_err(|e| format!("jj bookmark list wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj bookmark list タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
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
            assert!(!summary.contains("@- にあります"), "summary was: {}", summary);
        }
    }
}
