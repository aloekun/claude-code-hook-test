use lib_report_formatter::Finding;

use crate::log::log_info;
use crate::runner::{capture_commit_id, diff_at_is_empty, run_cmd_direct, JJ_CMD_TIMEOUT_SECS};

use super::description::{build_fix_commit_description, FixCommitState};

/// pre-takt で fix commit を新規作成する (`jj new -m "..."`)。
///
/// 成功時: `FixCommitState::Created { commit_id }` を返す。@ は空 child を指す状態。
/// 失敗時: `FixCommitState::None` を返す (fallback = 分離なしで元の flow へフォールバック)。
///
/// `jj new` が成功したが `capture_commit_id` で commit id を追跡できない場合は、
/// 作成済みの空 child が orphan にならないよう即座に abandon を試みる
/// (fail-safe: 追跡不能 child を remote に残さない)。
pub(crate) fn create_fix_commit(pr_number: Option<u64>, findings: &[Finding]) -> FixCommitState {
    let desc = build_fix_commit_description(pr_number, findings);
    let (ok, output) = run_cmd_direct("jj", &["new", "-m", &desc], &[], JJ_CMD_TIMEOUT_SECS);
    if !ok {
        log_info(&format!(
            "[action] fix commit 分離 skip: jj new 失敗: {}",
            output
        ));
        return FixCommitState::None;
    }
    match capture_commit_id() {
        Some(cid) => {
            log_info(&format!("[state] fix commit pre-created: {}", cid));
            FixCommitState::Created { commit_id: cid }
        }
        None => {
            log_info(
                "[state] fix commit 作成後の commit id capture 失敗 (orphan child を cleanup)",
            );
            try_abandon_empty_fix_commit("create_fix_commit id capture 失敗:", None);
            FixCommitState::None
        }
    }
}

/// 空 fix commit を安全に abandon する。
///
/// `commit_id` が `Some(expected)` のとき: 現在の `@` が `expected` と一致する場合のみ
/// abandon を実行する。不一致または capture 失敗時は `[warn]` を出してスキップする。
/// `commit_id` が `None` のとき: 従来通り diff チェックのみで判定する。
///
/// diff あり判定失敗時は abandon をスキップ (fail-safe: 誤 abandon 防止)。
///
/// abandon 成功後は `reparent_at_to_pr_tip` で `@` を PR tip 直下に戻す
/// (task 6: cleanup 後の @ 孤児化を解消)。
pub(crate) fn try_abandon_empty_fix_commit(context: &str, commit_id: Option<&str>) {
    if let Some(expected) = commit_id {
        match capture_commit_id().as_deref() {
            Some(current) if current == expected => {}
            Some(current) => {
                log_info(&format!(
                    "[warn] {} expected={}, current={} abandon を見送り",
                    context, expected, current
                ));
                return;
            }
            None => {
                log_info(&format!(
                    "[warn] {} expected={}, current=<capture失敗> abandon を見送り",
                    context, expected
                ));
                return;
            }
        }
    }

    if diff_at_is_empty() {
        let label = commit_id.map_or_else(String::new, |id| format!(" ({})", id));
        log_info(&format!(
            "[action] {} 空 fix commit を abandon{}",
            context, label
        ));
        let (ok, out) = run_cmd_direct("jj", &["abandon"], &[], JJ_CMD_TIMEOUT_SECS);
        if !ok {
            log_info(&format!(
                "[action] jj abandon 失敗 (手動片付け推奨): {}",
                out
            ));
            return;
        }
        reparent_at_to_pr_tip(context);
    } else {
        log_info(&format!(
            "[warn] {} fix commit に diff あり、abandon を見送り",
            context
        ));
    }
}

/// `@` を PR tip (単一 local bookmark の指す commit) 直下に再配置する。
///
/// `jj abandon` 直後の `@` は stale な空 commit の上に残ることがあり
/// (task 6 背景: PR #64 で 3 回発生)、次の `jj new` がそこに積まれる。
/// これを解消するため、bookmark が指す PR tip を解決して `jj new -r <tip>` で
/// `@` を PR tip の直接子に戻す。
///
/// 以下のケースは fail-safe でスキップする:
/// - PR tip 解決失敗 (bookmark なし / 複数 bookmark で曖昧 / 取得失敗)
/// - 既に `@-` が PR tip と一致 (redundant な空 commit を作らない)
/// - `jj new -r <tip>` 自体の失敗 (ログのみで処理を継続)
fn reparent_at_to_pr_tip(context: &str) {
    let pr_tip = match crate::stages::push_jj_bookmark::resolve_pr_tip_commit_id() {
        Some(id) => id,
        None => {
            log_info(&format!(
                "[state] {} PR tip bookmark を特定できず re-parent スキップ",
                context
            ));
            return;
        }
    };

    if parent_commit_id_is(&pr_tip) {
        log_info(&format!(
            "[state] {} @ は既に PR tip ({}) 直下、re-parent 不要",
            context, pr_tip
        ));
        return;
    }

    let (ok, out) = run_cmd_direct("jj", &["new", "-r", &pr_tip], &[], JJ_CMD_TIMEOUT_SECS);
    if ok {
        log_info(&format!(
            "[action] {} @ を PR tip ({}) 直下に re-parent",
            context, pr_tip
        ));
    } else {
        log_info(&format!(
            "[action] {} @ の re-parent 失敗 (手動対応): {}",
            context, out
        ));
    }
}

/// `@-` (親 commit) の id が `expected` と一致するか判定する。
/// 取得失敗時は `false` (= 不一致扱いで reparent を試行) を返す。
fn parent_commit_id_is(expected: &str) -> bool {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["log", "-r", "@-", "--no-graph", "-T", "commit_id"],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    ok && out.trim() == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 統合: `create_fix_commit` の fail-safe cleanup 動作を確認する。
    ///
    /// `capture_commit_id` 失敗を直接 inject できないため、代わりに
    /// `try_abandon_empty_fix_commit(_, None)` を直接呼んで「空 child が cleanup される」
    /// 挙動 (= None 分岐が依拠する唯一の副作用) が jj で動くことを確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_try_abandon_empty_fix_commit_without_id_drops_orphan_child() {
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

        std::fs::write(repo_dir.join("a.txt"), "x\n").expect("write failed");
        let original_msg = "feat: original";
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", original_msg])
            .current_dir(repo_dir)
            .status()
            .expect("describe")
            .success());

        assert!(StdCommand::new("jj")
            .args(["new", "-m", "fix(review): orphan test"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new")
            .success());

        let original_cwd = env::current_dir().expect("cwd");
        env::set_current_dir(repo_dir).expect("cd");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore {
            original: original_cwd,
        };

        try_abandon_empty_fix_commit("test:", None);

        let log_out = StdCommand::new("jj")
            .args([
                "log",
                "-r",
                "::@",
                "--no-graph",
                "-T",
                "description ++ \"\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log");
        let log_str = String::from_utf8_lossy(&log_out.stdout);
        assert!(
            !log_str.contains("fix(review): orphan test"),
            "orphan child が abandon されていない: {:?}",
            log_str
        );
        assert!(
            log_str.contains(original_msg),
            "元 commit が残っていること: {:?}",
            log_str
        );
    }

    /// 統合: task 6 の再現 — `pnpm push` 後の空 WC の上に fix commit が
    /// 作られた状態で cleanup すると、`@` が stale な空 commit に残らず、
    /// PR tip (bookmark の指す commit) 直下に自動で re-parent されることを確認する。
    ///
    /// 検証対象シナリオ (PR #64 で 3 回発生):
    /// - `C1 (bookmark) ← C1' (empty, from pnpm push) ← Y (fix commit, @)`
    /// - takt が NoChange で Y を abandon した後、従来は `@- == C1'` に残っていた
    /// - 修正後は `@- == C1` (PR tip) に戻る
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_try_abandon_reparents_at_to_pr_tip_after_cleanup() {
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

        std::fs::write(repo_dir.join("a.txt"), "content\n").expect("write a.txt 失敗");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", "feat: PR body"])
            .current_dir(repo_dir)
            .status()
            .expect("describe C1 失敗")
            .success());
        let c1_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log C1");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert!(!c1_id.is_empty());

        assert!(StdCommand::new("jj")
            .args(["bookmark", "create", "feat/task6", "-r", "@"])
            .current_dir(repo_dir)
            .status()
            .expect("bookmark create 失敗")
            .success());

        assert!(StdCommand::new("jj")
            .args(["new"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new (C1') 失敗")
            .success());

        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore {
            original: original_cwd,
        };

        let fix_state = create_fix_commit(Some(64), &[]);
        let fix_cid = match &fix_state {
            FixCommitState::Created { commit_id } => commit_id.clone(),
            _ => panic!("create_fix_commit 失敗: {:?}", fix_state),
        };

        try_abandon_empty_fix_commit("test:", Some(&fix_cid));

        let parent_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@-", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log @-");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            parent_id, c1_id,
            "@- が PR tip (bookmark feat/task6) の指す commit と一致すること: got={:?}",
            parent_id
        );

        assert!(diff_at_is_empty(), "reparent 後の @ は空 WC");

        let bookmark_tip = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "feat/task6", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log bookmark");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            bookmark_tip, c1_id,
            "bookmark が動かされていないこと: got={:?}",
            bookmark_tip
        );
    }

    /// 統合: bookmark が複数ある場合 (stacked PR 等) は reparent をスキップし、
    /// `jj abandon` のデフォルト配置 (親の上に新規 WC) に任せる fail-safe 挙動を確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_try_abandon_skips_reparent_with_multiple_bookmarks() {
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

        std::fs::write(repo_dir.join("a.txt"), "content\n").expect("write 失敗");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", "feat: base"])
            .current_dir(repo_dir)
            .status()
            .expect("describe 失敗")
            .success());

        for name in &["feat/stack-a", "feat/stack-b"] {
            assert!(StdCommand::new("jj")
                .args(["bookmark", "create", name, "-r", "@"])
                .current_dir(repo_dir)
                .status()
                .expect("bookmark create 失敗")
                .success());
        }

        assert!(StdCommand::new("jj")
            .args(["new"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new 失敗")
            .success());
        let c1_prime_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore {
            original: original_cwd,
        };

        let fix_state = create_fix_commit(Some(1), &[]);
        let fix_cid = match &fix_state {
            FixCommitState::Created { commit_id } => commit_id.clone(),
            _ => panic!("create_fix_commit 失敗"),
        };

        try_abandon_empty_fix_commit("test:", Some(&fix_cid));

        let parent_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@-", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log @-");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            parent_id, c1_prime_id,
            "複数 bookmark 時は reparent スキップ、@- は C1' のまま: got={:?}",
            parent_id
        );
    }
}
