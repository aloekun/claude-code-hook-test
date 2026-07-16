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
        head_is_empty_or_assume_not(),
        || {
            run_jj_bookmark_list(PARENT_REVSET)
                .map(|raw| parse_non_trunk_bookmarks(&raw))
                .unwrap_or_default()
        },
    );
    report_outcome(outcome)
}

/// `@` の空判定。判定不能 (jj 実行失敗) 時は「空でない」に倒し、既存の
/// 「bookmark 皆無」案内へフォールバックする。この判定は案内文の出し分けにしか
/// 使わず、push を通すか止めるかの厳格さ ([ADR-043] fail-closed) には影響しない
/// — いずれの分岐でも push は中断する。
fn head_is_empty_or_assume_not() -> bool {
    working_copy_is_empty().unwrap_or(false)
}

/// bookmark_check の判定結果。jj 実行から切り離して単体テスト可能にする
/// (`dispatch_bookmark_advance` と同じ closure 注入の流儀)。
#[derive(Debug, PartialEq)]
enum BookmarkCheckOutcome {
    /// `@` に非 trunk bookmark があり push 可能。
    Proceed(Vec<String>),
    /// `@` が空で push 不可 (T8 incident)。`parent_bookmarks` = `@-` にある bookmark。
    EmptyWorkingCopy { parent_bookmarks: Vec<String> },
    /// `@` は空でないが bookmark が無い。作成案内が正しいケース。
    NoBookmarks,
}

/// 「`@` に bookmark が無い」状態を 2 ケースに切り分ける (T8)。
///
/// `@` に bookmark があれば従来どおり続行する (`@` が空でも変更しない = 既存の
/// 成功経路を退行させない)。無い場合のみ `@` の空判定で案内を出し分ける。
fn decide_bookmark_check(
    bookmarks_at_head: Vec<String>,
    head_is_empty: bool,
    parent_bookmarks: impl FnOnce() -> Vec<String>,
) -> BookmarkCheckOutcome {
    if !bookmarks_at_head.is_empty() {
        return BookmarkCheckOutcome::Proceed(bookmarks_at_head);
    }
    if head_is_empty {
        return BookmarkCheckOutcome::EmptyWorkingCopy {
            parent_bookmarks: parent_bookmarks(),
        };
    }
    BookmarkCheckOutcome::NoBookmarks
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
        BookmarkCheckOutcome::EmptyWorkingCopy { parent_bookmarks } => {
            log_stage("bookmark", &empty_working_copy_summary(&parent_bookmarks));
            log_info(
                "  push 不可: レビュー対象の diff は `@` から取得するため、`@` が空のままでは\n  \
                 AI レビューが skip されたまま push されます。\n  \
                 対処: `jj edit @-` で `@` を bookmark のコミットへ移動して再実行してください\n  \
                 (不要になった空の WIP コミットは `jj abandon <change_id>` で削除できます)",
            );
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

fn empty_working_copy_summary(parent_bookmarks: &[String]) -> String {
    if parent_bookmarks.is_empty() {
        "`@` が空で bookmark もありません".to_string()
    } else {
        format!(
            "`@` が空です (bookmark は @- にあります: {})",
            parent_bookmarks.join(", ")
        )
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

        fn no_parent_bookmarks() -> Vec<String> {
            Vec::new()
        }

        /// incident 再現 (bad): `@` が空 + bookmark が `@-`。
        /// 「bookmark 皆無」(= 作成案内が正しいケース) と取り違えてはならない。
        #[test]
        fn decide_empty_head_with_parent_bookmark_is_not_no_bookmarks() {
            let outcome = decide_bookmark_check(Vec::new(), true, || {
                vec!["perf/lint-screen-evals-opt-in".to_string()]
            });
            assert_eq!(
                outcome,
                BookmarkCheckOutcome::EmptyWorkingCopy {
                    parent_bookmarks: vec!["perf/lint-screen-evals-opt-in".to_string()]
                }
            );
        }

        /// 前段の別症状 (good): bookmark が皆無かつ `@` が空でない場合は
        /// 既存の作成案内が正しいので `NoBookmarks` のまま維持する。
        #[test]
        fn decide_no_bookmarks_when_head_is_not_empty() {
            let outcome = decide_bookmark_check(Vec::new(), false, no_parent_bookmarks);
            assert_eq!(outcome, BookmarkCheckOutcome::NoBookmarks);
        }

        /// `@` が空 + bookmark が皆無 (root commit 直後等) も push 不可だが、
        /// 誤誘導を避けるため `@-` への移動案内側に倒す。
        #[test]
        fn decide_empty_head_without_parent_bookmark_reports_empty_working_copy() {
            let outcome = decide_bookmark_check(Vec::new(), true, no_parent_bookmarks);
            assert_eq!(
                outcome,
                BookmarkCheckOutcome::EmptyWorkingCopy {
                    parent_bookmarks: Vec::new()
                }
            );
        }

        /// 既存の成功経路 (good): `@` に bookmark があれば従来どおり続行する。
        #[test]
        fn decide_proceeds_when_bookmark_is_at_head() {
            let outcome = decide_bookmark_check(vec!["feat/xyz".to_string()], false, || {
                panic!("`@` に bookmark がある場合は @- を照会してはならない")
            });
            assert_eq!(
                outcome,
                BookmarkCheckOutcome::Proceed(vec!["feat/xyz".to_string()])
            );
        }

        /// `@` が空でも bookmark が `@` にあるなら続行する (T8 修正で退行させない)。
        #[test]
        fn decide_proceeds_when_bookmark_at_head_even_if_head_is_empty() {
            let outcome = decide_bookmark_check(vec!["feat/xyz".to_string()], true, || {
                panic!("`@` に bookmark がある場合は @- を照会してはならない")
            });
            assert_eq!(
                outcome,
                BookmarkCheckOutcome::Proceed(vec!["feat/xyz".to_string()])
            );
        }

        /// 2 ケースの取り違えを防ぐ核心: 案内文が bookmark の所在 (`@-`) を名指しする。
        #[test]
        fn summary_names_the_parent_bookmark_so_the_two_cases_are_distinguishable() {
            let summary = empty_working_copy_summary(&["perf/xyz".to_string()]);
            assert!(summary.contains("perf/xyz"), "summary was: {}", summary);
            assert!(summary.contains("@-"), "summary was: {}", summary);
        }

        #[test]
        fn summary_without_parent_bookmark_omits_bookmark_name() {
            let summary = empty_working_copy_summary(&[]);
            assert!(summary.contains("空"), "summary was: {}", summary);
        }
    }
}
