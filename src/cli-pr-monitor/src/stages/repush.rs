use crate::fix_commit::FixCommitState;
use crate::log::log_info;
use crate::stages::push::run_push;

// ─── re-push 判定 (pure function、副作用を注入可能) ───

/// re-push すべきかの判定結果。
/// `IdCaptureFailed` は fail-safe で `NoChange` と同じ扱い (push しない)。
#[derive(Debug, PartialEq)]
pub(crate) enum RepushDecision {
    /// 実質変更なし: push 不要
    NoChange,
    /// 実質変更あり: push 対象
    HasChange,
    /// commit id 取得に失敗: 判定不能 → fail-safe で push しない
    IdCaptureFailed,
}

/// 純粋な dispatch 関数: `(decision, fix_state, allow_auto)` から次の action を返す。
///
/// 実行 (run_push / jj abandon / log) は `execute_repush_flow` 側に委ね、
/// ここは**判断だけ**を行うことで unit test のマトリクス網羅を可能にする。
#[derive(Debug, PartialEq)]
pub(crate) enum RepushAction {
    /// 自動 push を実行 (HasChange + allow_auto)
    AutoPush,
    /// 分離済み fix commit があるので手動確認を促す (HasChange + !allow_auto + Created)
    UserConfirmWithSeparatedFix { commit_id: String },
    /// 通常の手動 push 待ち (HasChange + !allow_auto + None)
    UserConfirmNoSeparation,
    /// 事前作成した空 fix commit を abandon (NoChange + Created)
    CleanupEmptyFixCommit { commit_id: String },
    /// takt no-op で何もしない (NoChange + None)
    SkipNoChange,
    /// commit id 取得失敗で何もしない (IdCaptureFailed)
    FailSafeCaptureFailed,
}

pub(crate) fn decide_repush_action(
    decision: &RepushDecision,
    fix_state: &FixCommitState,
    allow_auto: bool,
) -> RepushAction {
    match (decision, fix_state, allow_auto) {
        (RepushDecision::HasChange, _, true) => RepushAction::AutoPush,
        (RepushDecision::HasChange, FixCommitState::Created { commit_id }, false) => {
            RepushAction::UserConfirmWithSeparatedFix {
                commit_id: commit_id.clone(),
            }
        }
        (RepushDecision::HasChange, FixCommitState::None, false) => {
            RepushAction::UserConfirmNoSeparation
        }
        (RepushDecision::NoChange, FixCommitState::Created { commit_id }, _) => {
            RepushAction::CleanupEmptyFixCommit {
                commit_id: commit_id.clone(),
            }
        }
        (RepushDecision::NoChange, FixCommitState::None, _) => RepushAction::SkipNoChange,
        (RepushDecision::IdCaptureFailed, _, _) => RepushAction::FailSafeCaptureFailed,
    }
}

/// takt 実行前後の commit id と diff 判定関数から repush 要否を決める。
///
/// 二段構え: commit id だけで判定せず、id が変わっていても diff が空なら
/// 「実質変更なし」とみなす。jj が metadata だけ更新するケース (auto-snapshot
/// の timestamp 差分など) で誤 push しないための防御。
///
/// 副作用 (jj 呼び出し) を `diff_empty_fn` として注入することで unit test 可能。
pub(crate) fn decide_repush(
    pre_cid: Option<&str>,
    post_cid: Option<&str>,
    diff_empty_fn: impl FnOnce(&str, &str) -> bool,
) -> RepushDecision {
    match (pre_cid, post_cid) {
        (Some(pre), Some(post)) if pre == post => RepushDecision::NoChange,
        (Some(pre), Some(post)) if diff_empty_fn(pre, post) => RepushDecision::NoChange,
        (Some(_), Some(_)) => RepushDecision::HasChange,
        _ => RepushDecision::IdCaptureFailed,
    }
}

// ─── re-push フロー ───

fn run_auto_push(config: &crate::config::FixConfig, pr_label: &str) {
    log_info(&format!(
        "[action] auto_push: {} の takt 修正を自動 re-push",
        pr_label
    ));
    if run_push(config) {
        log_info("[action] auto_push: 成功");
    } else {
        log_info("[action] auto_push: 失敗 (手動対応が必要)");
    }
}

/// auto_push_severity 設定値から自動 push するか否かを返す。
/// "critical" / "major" => true、"none" => false、未知値 => false (警告ログあり)
pub(crate) fn should_auto_push(setting: &str) -> bool {
    match setting {
        "none" => false,
        "critical" | "major" => true,
        other => {
            log_info(&format!(
                "auto_push_severity に未知の値 '{}' が指定されています。'none' として扱い自動 push をスキップします",
                other
            ));
            false
        }
    }
}

/// takt 実行後の re-push フロー。
///
/// 1. post_takt_cid を捕捉し、pre / post を比較して `decide_repush` で判定
/// 2. `decide_repush_action` で (decision, fix_state, allow_auto) から action を決定
/// 3. action に応じて push / abandon / log を実行
///
/// すべての分岐で `[state]` / `[decision]` / `[action]` プレフィックスのログを残し、
/// 事後に「なぜ push した/しなかったか」を追跡できるようにする。
pub(crate) fn execute_repush_flow(
    fix_config: &crate::config::FixConfig,
    pr_label: &str,
    pre_cid: Option<&str>,
    fix_state: &FixCommitState,
) {
    let post_cid = crate::runner::capture_commit_id();
    log_info(&format!("[state] post_takt_commit_id: {:?}", post_cid));

    let decision = decide_repush(pre_cid, post_cid.as_deref(), crate::runner::diff_is_empty);
    log_info(&format!("[decision] repush: {:?}", decision));

    let allow_auto = should_auto_push(&fix_config.auto_push_severity);
    log_info(&format!(
        "[state] auto_push_setting: {} (allow_auto: {}), fix_state_created: {}",
        fix_config.auto_push_severity,
        allow_auto,
        fix_state.is_created()
    ));

    let action = decide_repush_action(&decision, fix_state, allow_auto);
    log_info(&format!("[decision] action: {:?}", action));

    match action {
        RepushAction::AutoPush => run_auto_push(fix_config, pr_label),
        RepushAction::UserConfirmWithSeparatedFix { commit_id } => {
            log_info(&format!(
                "[action] auto_push スキップ: ユーザー確認待ち (fix commit 分離済み: {})",
                commit_id
            ));
            log_info("[action] 確認後に pnpm push するか、jj describe で再構成してください");
        }
        RepushAction::UserConfirmNoSeparation => {
            log_info("[action] auto_push スキップ: ユーザー確認待ち");
            log_info("[action] 確認後に pnpm push を実行してください");
        }
        RepushAction::CleanupEmptyFixCommit { commit_id } => {
            crate::fix_commit::try_abandon_empty_fix_commit("fix_state=Created:", Some(&commit_id));
        }
        RepushAction::SkipNoChange => {
            log_info("[action] re-push スキップ: takt は実質変更を加えていない");
        }
        RepushAction::FailSafeCaptureFailed => {
            log_info("[action] re-push スキップ: commit id 取得失敗 (fail-safe)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{capture_commit_id, diff_is_empty};

    #[test]
    fn should_auto_push_none_returns_false() {
        assert!(!should_auto_push("none"));
    }

    #[test]
    fn should_auto_push_critical_returns_true() {
        assert!(should_auto_push("critical"));
    }

    #[test]
    fn should_auto_push_major_returns_true() {
        assert!(should_auto_push("major"));
    }

    #[test]
    fn should_auto_push_unknown_value_returns_false() {
        // タイポや未知値は fail-closed: 自動 push しない
        assert!(!should_auto_push("non"));
        assert!(!should_auto_push("Critical"));
        assert!(!should_auto_push(""));
    }

    // ─── decide_repush: commit id + diff の二段構え判定 ───

    #[test]
    fn decide_repush_same_commit_id_returns_no_change() {
        // pre == post: ID 変化なし → diff 確認せずに NoChange
        let d = decide_repush(Some("abc123"), Some("abc123"), |_, _| {
            panic!("diff_empty_fn は呼ばれてはならない (短絡評価)")
        });
        assert_eq!(d, RepushDecision::NoChange);
    }

    #[test]
    fn decide_repush_different_id_empty_diff_returns_no_change() {
        // ID 変化 + diff 空: jj の metadata だけ更新された等、実質変更なし
        let d = decide_repush(Some("abc123"), Some("def456"), |_, _| true);
        assert_eq!(d, RepushDecision::NoChange);
    }

    #[test]
    fn decide_repush_different_id_nonempty_diff_returns_has_change() {
        // ID 変化 + diff 非空: 実質的に変更あり → push 対象
        let d = decide_repush(Some("abc123"), Some("def456"), |_, _| false);
        assert_eq!(d, RepushDecision::HasChange);
    }

    #[test]
    fn decide_repush_pre_cid_none_returns_capture_failed() {
        // pre_cid 取得失敗 → fail-safe
        let d = decide_repush(None, Some("def456"), |_, _| {
            panic!("diff_empty_fn は呼ばれてはならない (capture 失敗時)")
        });
        assert_eq!(d, RepushDecision::IdCaptureFailed);
    }

    #[test]
    fn decide_repush_post_cid_none_returns_capture_failed() {
        // post_cid 取得失敗 → fail-safe
        let d = decide_repush(Some("abc123"), None, |_, _| {
            panic!("diff_empty_fn は呼ばれてはならない (capture 失敗時)")
        });
        assert_eq!(d, RepushDecision::IdCaptureFailed);
    }

    #[test]
    fn decide_repush_both_cid_none_returns_capture_failed() {
        // 両方取得失敗
        let d = decide_repush(None, None, |_, _| {
            panic!("diff_empty_fn は呼ばれてはならない (capture 失敗時)")
        });
        assert_eq!(d, RepushDecision::IdCaptureFailed);
    }

    // ─── decide_repush_action: (decision × fix_state × allow_auto) 3×2×2 マトリクス ───
    //
    // `IdCaptureFailed` では fix_state / allow_auto に関係なく FailSafeCaptureFailed
    // を返す短絡挙動も含め、全分岐を網羅する。

    fn created(id: &str) -> FixCommitState {
        FixCommitState::Created {
            commit_id: id.to_string(),
        }
    }

    #[test]
    fn action_has_change_with_auto_any_state() {
        // HasChange + allow_auto=true: fix_state に関係なく AutoPush
        let a1 = decide_repush_action(&RepushDecision::HasChange, &FixCommitState::None, true);
        let a2 = decide_repush_action(&RepushDecision::HasChange, &created("abc"), true);
        assert_eq!(a1, RepushAction::AutoPush);
        assert_eq!(a2, RepushAction::AutoPush);
    }

    #[test]
    fn action_has_change_no_auto_with_separated_fix_returns_user_confirm_separated() {
        // HasChange + allow_auto=false + Created: 分離済みメッセージ
        let a = decide_repush_action(&RepushDecision::HasChange, &created("abc123"), false);
        assert_eq!(
            a,
            RepushAction::UserConfirmWithSeparatedFix {
                commit_id: "abc123".into()
            }
        );
    }

    #[test]
    fn action_has_change_no_auto_without_separation_returns_user_confirm() {
        // HasChange + allow_auto=false + None: 通常の手動 push 待ち
        let a = decide_repush_action(&RepushDecision::HasChange, &FixCommitState::None, false);
        assert_eq!(a, RepushAction::UserConfirmNoSeparation);
    }

    #[test]
    fn action_no_change_with_created_returns_cleanup_regardless_of_auto() {
        // NoChange + Created: allow_auto に関係なく空 child cleanup
        let a1 = decide_repush_action(&RepushDecision::NoChange, &created("xyz"), true);
        let a2 = decide_repush_action(&RepushDecision::NoChange, &created("xyz"), false);
        let expected = RepushAction::CleanupEmptyFixCommit {
            commit_id: "xyz".into(),
        };
        assert_eq!(a1, expected);
        assert_eq!(a2, expected);
    }

    #[test]
    fn action_no_change_without_separation_returns_skip() {
        // NoChange + None: 何もしない
        let a1 = decide_repush_action(&RepushDecision::NoChange, &FixCommitState::None, true);
        let a2 = decide_repush_action(&RepushDecision::NoChange, &FixCommitState::None, false);
        assert_eq!(a1, RepushAction::SkipNoChange);
        assert_eq!(a2, RepushAction::SkipNoChange);
    }

    #[test]
    fn action_id_capture_failed_always_fail_safe() {
        // IdCaptureFailed: fix_state / allow_auto に関係なく FailSafeCaptureFailed
        let states = [FixCommitState::None, created("abc")];
        for s in &states {
            for &auto in &[true, false] {
                let a = decide_repush_action(&RepushDecision::IdCaptureFailed, s, auto);
                assert_eq!(a, RepushAction::FailSafeCaptureFailed);
            }
        }
    }

    // ─── 統合テスト (外部依存: jj CLI) ───
    //
    // 実 jj プロセスと working copy を使い、PR #43 で観測したバグ(
    // takt が no-op のとき誤 push + description 上書き) の退行を防ぐ最小ケース。
    //
    // 実行方法 (push-runner-config.toml の rust-test group と同じ):
    //   cargo test --manifest-path src/cli-pr-monitor/Cargo.toml -- --ignored --test-threads=1
    //
    // --test-threads=1 は `std::env::set_current_dir` の同時呼び出しを避けるため。
    // push pipeline (push-runner-config.toml) でのみ実行することを想定し、
    // PostToolUse / Stop hook では走らせない。

    /// cwd を Drop タイミングで元に戻す RAII ガード。
    /// panic でテストが中断しても cwd が復元されることを保証する
    /// (複数 `#[ignore]` テストを追加したとき他テストに cwd の副作用を与えないため)。
    struct CwdRestore {
        original: std::path::PathBuf,
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    /// 統合: takt が no-op の場合、decide_repush は NoChange を返し、
    /// auto_push_severity="critical" でも push 判定が false になる。
    /// さらに @ の description が保持されていることを確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_noop_takt_does_not_trigger_push_and_preserves_description() {
        use std::env;
        use std::process::Command as StdCommand;

        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();

        // 1. jj git init
        let init_ok = StdCommand::new("jj")
            .args(["git", "init"])
            .current_dir(repo_dir)
            .status()
            .expect("jj git init 実行失敗")
            .success();
        assert!(init_ok, "jj git init が失敗");

        // 2. ファイル作成 + describe (PR の commit に相当する状態)
        std::fs::write(repo_dir.join("README.md"), "integration test content\n")
            .expect("テストファイル書き込み失敗");
        let original_msg = "test: original description (must be preserved)";
        let describe_ok = StdCommand::new("jj")
            .args(["describe", "-m", original_msg])
            .current_dir(repo_dir)
            .status()
            .expect("jj describe 実行失敗")
            .success();
        assert!(describe_ok, "jj describe が失敗");

        // 3. cli-pr-monitor の helper 関数は cwd 依存のため set_current_dir。
        //    RAII ガードで panic 時にも cwd を元に戻す (panic-safe)。
        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        let _cwd_guard = CwdRestore {
            original: original_cwd,
        };

        // 4. takt 実行前の commit id を capture
        let pre_cid = capture_commit_id();
        assert!(pre_cid.is_some(), "pre_cid が取得できること");

        // 5. takt no-op シミュレーション: 何もしない
        //    (実 takt を呼ぶとコストが高いため、no-op として扱う)

        // 6. takt 実行後の commit id を capture
        let post_cid = capture_commit_id();
        assert_eq!(
            pre_cid, post_cid,
            "takt が no-op のとき commit id は変化しないこと"
        );

        // 7. decide_repush が NoChange を返す
        let decision = decide_repush(pre_cid.as_deref(), post_cid.as_deref(), diff_is_empty);
        assert_eq!(
            decision,
            RepushDecision::NoChange,
            "no-op 時は NoChange 判定"
        );

        // 8. decision=NoChange なので HasChange アームには入らず、
        //    should_auto_push は呼ばれない。critical 設定でも push が走らないことは
        //    step 7 の NoChange アサーションで既に保証されている。

        // 9. description が保持されていること (バグ #2 の退行防止)
        let desc_output = StdCommand::new("jj")
            .args(["log", "-r", "@", "--no-graph", "-T", "description"])
            .current_dir(repo_dir)
            .output()
            .expect("jj log 実行失敗");
        let desc = String::from_utf8_lossy(&desc_output.stdout);
        assert!(
            desc.contains(original_msg),
            "description が保持されていること: got={:?}",
            desc
        );

        // cwd は `_cwd_guard` の Drop で自動復元される
    }

    // ─── 分離型 fix commit (ADR task 4) の統合テスト ───

    /// 統合: takt amend シミュレーションにより、commit graph が
    /// original ← fix(@) の 2 commit 構造になることを確認する。
    ///
    /// pre-takt で `create_fix_commit` を呼び、その後ファイル書き込みで
    /// takt の amend を擬似再現する。PR task 4 の骨子要件。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_fix_commit_separation_creates_two_commits_on_has_change() {
        use crate::fix_commit::{create_fix_commit, FixCommitState};
        use crate::runner::diff_at_is_empty;
        use std::env;
        use std::process::Command as StdCommand;

        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();

        // 1. jj git init
        assert!(
            StdCommand::new("jj")
                .args(["git", "init"])
                .current_dir(repo_dir)
                .status()
                .expect("jj git init 実行失敗")
                .success(),
            "jj git init が失敗"
        );

        // 2. original PR commit 相当を作成
        std::fs::write(repo_dir.join("README.md"), "original content\n")
            .expect("original file 書き込み失敗");
        let original_msg = "feat: original PR commit (must be preserved)";
        assert!(
            StdCommand::new("jj")
                .args(["describe", "-m", original_msg])
                .current_dir(repo_dir)
                .status()
                .expect("jj describe 失敗")
                .success(),
            "jj describe が失敗"
        );

        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        let _cwd_guard = CwdRestore {
            original: original_cwd,
        };

        // 3. create_fix_commit を呼び出し (pre-takt phase)
        let findings = vec![];
        let fix_state = create_fix_commit(Some(99), &findings);
        assert!(
            matches!(fix_state, FixCommitState::Created { .. }),
            "fix commit pre-create が成功すること: got={:?}",
            fix_state
        );

        // 4. @ は空 child (diff 空) でなければならない
        assert!(
            diff_at_is_empty(),
            "pre-create 直後の @ は diff 空であること"
        );

        // 5. pre_takt_cid 捕捉 (decide_repush 用)
        let pre_cid = capture_commit_id();
        assert!(pre_cid.is_some(), "pre_cid が取得できること");

        // 6. takt amend シミュレーション: 新ファイル書き込みで fix を擬似再現
        //    (jj auto-snapshot により @ に自動で amend される)
        std::fs::write(repo_dir.join("fix.rs"), "// CodeRabbit fix content\n")
            .expect("fix file 書き込み失敗");

        // 7. post_takt_cid 捕捉
        let post_cid = capture_commit_id();
        assert!(post_cid.is_some(), "post_cid が取得できること");

        // 8. decide_repush で HasChange を期待
        let decision = decide_repush(pre_cid.as_deref(), post_cid.as_deref(), diff_is_empty);
        assert_eq!(
            decision,
            RepushDecision::HasChange,
            "fix 内容が追加されたので HasChange"
        );

        // 9. commit graph 構造を検証: @ と @- で 2 commit (original + fix)
        let log_output = StdCommand::new("jj")
            .args([
                "log",
                "-r",
                "@- | @",
                "--no-graph",
                "-T",
                "description ++ \"\\n---\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log 失敗");
        let log_str = String::from_utf8_lossy(&log_output.stdout);
        assert!(
            log_str.contains(original_msg),
            "親 commit に original description が残っていること: got={:?}",
            log_str
        );
        assert!(
            log_str.contains("fix(review): apply CodeRabbit fixes for #99"),
            "fix child に自動生成 description があること: got={:?}",
            log_str
        );

        // 10. @- (original) の content は original のみ、@ に fix.rs が含まれること
        let files_at_parent = StdCommand::new("jj")
            .args(["file", "list", "-r", "@-"])
            .current_dir(repo_dir)
            .output()
            .expect("jj file list @- 失敗");
        let parent_files = String::from_utf8_lossy(&files_at_parent.stdout);
        assert!(
            parent_files.contains("README.md"),
            "@- に README.md が残っていること: got={:?}",
            parent_files
        );
        assert!(
            !parent_files.contains("fix.rs"),
            "@- には fix.rs が入っていないこと (= 元 commit は不変): got={:?}",
            parent_files
        );
    }

    /// 統合: takt no-op 時、create_fix_commit で作った空 child を
    /// `diff_at_is_empty` + `jj abandon` で安全に片付けて、
    /// 元の単一 commit 状態に戻ることを確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_fix_commit_cleanup_on_no_change_restores_original_state() {
        use crate::fix_commit::{create_fix_commit, FixCommitState};
        use crate::runner::diff_at_is_empty;
        use std::env;
        use std::process::Command as StdCommand;

        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();

        assert!(StdCommand::new("jj")
            .args(["git", "init"])
            .current_dir(repo_dir)
            .status()
            .expect("jj git init 失敗")
            .success());

        std::fs::write(repo_dir.join("a.txt"), "content\n").expect("ファイル書き込み失敗");
        let original_msg = "feat: original";
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", original_msg])
            .current_dir(repo_dir)
            .status()
            .expect("jj describe 失敗")
            .success());

        let original_cid_before = StdCommand::new("jj")
            .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
            .current_dir(repo_dir)
            .output()
            .expect("jj log 失敗");
        let original_change_id = String::from_utf8_lossy(&original_cid_before.stdout)
            .trim()
            .to_string();
        assert!(
            !original_change_id.is_empty(),
            "original change_id が取れること"
        );

        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        let _cwd_guard = CwdRestore {
            original: original_cwd,
        };

        // pre-takt: fix commit 作成
        let fix_state = create_fix_commit(Some(1), &[]);
        let pre_created_cid = match &fix_state {
            FixCommitState::Created { commit_id } => commit_id.clone(),
            _ => panic!("create_fix_commit が失敗: {:?}", fix_state),
        };

        let pre_cid = capture_commit_id();
        assert_eq!(
            pre_cid.as_deref(),
            Some(pre_created_cid.as_str()),
            "pre_cid = pre_created_cid"
        );

        // takt no-op シミュレーション: 何もしない
        let post_cid = capture_commit_id();

        // decide_repush は NoChange を返す
        let decision = decide_repush(pre_cid.as_deref(), post_cid.as_deref(), diff_is_empty);
        assert_eq!(decision, RepushDecision::NoChange);

        // diff_at_is_empty で true (空 child) を確認
        assert!(diff_at_is_empty(), "no-op 時の @ は diff 空");

        // jj abandon で片付け
        let abandon_ok = StdCommand::new("jj")
            .args(["abandon"])
            .current_dir(repo_dir)
            .status()
            .expect("jj abandon 失敗")
            .success();
        assert!(abandon_ok, "jj abandon が成功すること");

        // abandon 後の状態: jj は常に working copy を確保するため、
        // `@` は新しい空 WC に移り、`@-` が元の original commit になる。
        //
        // 検証すべき本質:
        // 1. original commit は保持されている (change_id でルックアップ可能)
        // 2. fix commit は abandon され、ログから消えている
        // 3. @ は original の子孫 (直接 = @-、または n 世代上)

        // 1) original が残っている
        let original_exists = StdCommand::new("jj")
            .args([
                "log",
                "-r",
                &original_change_id,
                "--no-graph",
                "-T",
                "description",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log 失敗");
        let original_desc = String::from_utf8_lossy(&original_exists.stdout);
        assert!(
            original_desc.contains(original_msg),
            "abandon 後も original commit が保持されていること: got={:?}",
            original_desc
        );

        // 2) 現在の active な先祖 chain に fix description が含まれないこと。
        //    NOTE: jj は abandoned commit でも commit_id 単独で参照すれば description を
        //    返す (op history に残る)。なので特定 ID の lookup ではなく「現在 @ から
        //    辿れる祖先」で fix が消えていることを確認する。
        let active_chain = StdCommand::new("jj")
            .args([
                "log",
                "-r",
                "::@",
                "--no-graph",
                "-T",
                "description ++ \"\\n---\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log 失敗");
        let chain_str = String::from_utf8_lossy(&active_chain.stdout);
        assert!(
            !chain_str.contains("fix(review)"),
            "@ から辿れる active chain に fix commit が残っていないこと: got={:?}",
            chain_str
        );
        assert!(
            chain_str.contains(original_msg),
            "active chain に original description が残っていること: got={:?}",
            chain_str
        );
    }
}
