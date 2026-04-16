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
/// 2. `HasChange` の場合のみ `should_auto_push` 設定を確認して push を実行
///
/// すべての分岐で `[state]` / `[decision]` / `[action]` プレフィックスのログを残し、
/// 事後に「なぜ push した/しなかったか」を追跡できるようにする。
pub(crate) fn execute_repush_flow(
    fix_config: &crate::config::FixConfig,
    pr_label: &str,
    pre_cid: Option<&str>,
) {
    let post_cid = crate::runner::capture_commit_id();
    log_info(&format!("[state] post_takt_commit_id: {:?}", post_cid));

    let decision = decide_repush(pre_cid, post_cid.as_deref(), crate::runner::diff_is_empty);
    log_info(&format!("[decision] repush: {:?}", decision));

    match decision {
        RepushDecision::HasChange => {
            // この match アームに入った時点で変更ありが確定しているため、
            // 設定値のみで自動 push 可否を判定する
            let allow_auto = should_auto_push(&fix_config.auto_push_severity);
            log_info(&format!(
                "[state] auto_push_setting: {} (allow_auto: {})",
                fix_config.auto_push_severity, allow_auto
            ));
            if allow_auto {
                log_info(&format!(
                    "[action] auto_push: {} の takt 修正を自動 re-push",
                    pr_label
                ));
                if run_push(fix_config) {
                    log_info("[action] auto_push: 成功");
                } else {
                    log_info("[action] auto_push: 失敗 (手動対応が必要)");
                }
            } else {
                log_info("[action] auto_push スキップ: ユーザー確認待ち");
                log_info("[action] 確認後に pnpm push を実行してください");
            }
        }
        RepushDecision::NoChange => {
            log_info("[action] re-push スキップ: takt は実質変更を加えていない");
        }
        RepushDecision::IdCaptureFailed => {
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
}
