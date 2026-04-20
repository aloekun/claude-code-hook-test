//! takt 自動修正後の auto re-push で bookmark が旧 commit に取り残されないよう
//! advance する helper。PR #50 で cli-push-runner に入った
//! `push_jj_bookmark::advance_jj_bookmarks` を cli-pr-monitor 用に port したもの。
//!
//! # 背景 (PR #53)
//!
//! `run_push` は `jj new` → push の 2 段構成だが、takt fix が `@` を amend すると
//! 旧 commit が obsolete 化する。この状態で `jj git push --bookmark <name>` を
//! 実行しても bookmark は旧 commit のまま動かず、remote に修正差分が届かない。
//!
//! # 設計
//!
//! 2 段構えで bookmark の前進先を決定する:
//!
//! 1. revset ベース: `(trunk()..target) & bookmarks()` で trunk から target までに
//!    出現する bookmark を列挙。通常ブランチ運用はこちらで解決する
//! 2. fallback: `jj bookmark list` の出力から非 trunk bookmark を抽出
//!    (obsolete commit 上の bookmark も含まれるため、PR #53 症状をここで救済)。
//!    安全策として非 trunk bookmark が 1 つだけのときにのみ前進させる
//!
//! # 共通化 (ADR-024)
//!
//! cli-push-runner 側と同一ロジック。まず port で機能等価を確認し、
//! 後続で `lib-jj-helpers` への集約を検討する (todo.md task 5 の設計メモ参照)。
use std::process::Command;

use lib_jj_helpers::{is_trunk_bookmark, parse_bookmark_list_output as parse_jj_log_output};

use crate::log::log_info;

pub(crate) fn advance_jj_bookmarks() -> Result<(), String> {
    let target = match determine_target_revision()? {
        Some(rev) => rev,
        None => return Ok(()), // root commit 等で有効な target がない
    };
    let bookmarks = get_bookmarks_in_range(&target)?;

    if bookmarks.is_empty() {
        // Fallback: takt fix が @ を amend すると旧 commit が obsolete になり、
        // revset ベースの検索では発見できない。`jj bookmark list` は obsolete
        // commit 上の bookmark も返すため、こちらで再探索する。
        return advance_bookmarks_via_list(&target);
    }

    apply_bookmarks(&bookmarks, &target, "");
    Ok(())
}

fn apply_bookmarks(bookmarks: &[String], target: &str, label: &str) {
    for bookmark in bookmarks {
        match set_bookmark(bookmark, target) {
            Ok(()) => log_info(&format!(
                "[action] bookmark '{}' を {} に自動更新{}",
                bookmark, target, label
            )),
            Err(e) => {
                log_info(&format!(
                    "[action] bookmark '{}' の更新失敗{} (続行): {}",
                    bookmark, label, e
                ));
            }
        }
    }
}

/// `jj bookmark list` の出力から非 trunk ローカル bookmark を取得し、target に前進させる。
/// revset ベースの `get_bookmarks_in_range` が空を返した場合のフォールバック。
///
/// 安全策: 非 trunk bookmark が 1 つだけの場合のみ前進させる。
/// 複数ある場合は無関係な bookmark を誤って移動するリスクがあるためスキップする。
fn advance_bookmarks_via_list(target: &str) -> Result<(), String> {
    let bookmarks = get_local_bookmarks_from_list()?;
    dispatch_bookmark_advance(&bookmarks, target, |b, t| {
        apply_bookmarks(b, t, " (fallback)")
    });
    Ok(())
}

fn dispatch_bookmark_advance(
    bookmarks: &[String],
    target: &str,
    apply: impl FnOnce(&[String], &str),
) {
    match bookmarks.len() {
        0 => {
            log_info("[state] ローカル bookmark が見つかりません (新規ブランチ等)");
        }
        1 => {
            log_info(&format!(
                "[action] fallback: bookmark '{}' を {} に前進させます",
                bookmarks[0], target
            ));
            apply(bookmarks, target);
        }
        _ => {
            // 複数の非 trunk bookmark がある場合、無関係な bookmark を
            // 誤って移動するリスクがあるためスキップする
            log_info(&format!(
                "[state] 複数の bookmark ({}) が存在するため fallback 更新をスキップします: {}",
                bookmarks.len(),
                bookmarks.join(", ")
            ));
        }
    }
}

fn determine_target_revision() -> Result<Option<String>, String> {
    const EMPTY_COMMIT_SENTINEL: &str = "empty";
    let output = run_jj_log(
        "@",
        &format!("if(empty, \"{EMPTY_COMMIT_SENTINEL}\", \"content\")"),
    )?;
    if output.trim() == EMPTY_COMMIT_SENTINEL {
        match run_jj_log("@-", "commit_id") {
            Ok(_) => Ok(Some("@-".to_string())),
            Err(_) => {
                log_info("[state] @ が root commit のため bookmark 自動更新をスキップします");
                Ok(None)
            }
        }
    } else {
        Ok(Some("@".to_string()))
    }
}

fn get_bookmarks_in_range(target: &str) -> Result<Vec<String>, String> {
    let revsets = [
        format!("(trunk()..{}) & bookmarks()", target),
        format!("(main..{}) & bookmarks()", target),
        format!("(master..{}) & bookmarks()", target),
    ];

    const BOOKMARK_TEMPLATE: &str = "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"";

    for revset in &revsets {
        match run_jj_log(revset, BOOKMARK_TEMPLATE) {
            Ok(output) => {
                let bookmarks = parse_jj_log_output(&output);
                if !bookmarks.is_empty() {
                    return Ok(bookmarks);
                }
            }
            Err(_) => continue,
        }
    }

    // revset で検出できない場合、呼び出し側 (advance_jj_bookmarks) が
    // `jj bookmark list` ベースの fallback に進む。ここで「スキップ」と出すと
    // fallback が成功したときに「スキップ → 自動更新」の矛盾ログになるため、
    // revset 段階は中立な文言に留める。
    log_info("[state] revset では bookmark を検出できないため fallback 判定に進みます");
    Ok(Vec::new())
}

const JJ_TIMEOUT_SECS: u64 = 30;

fn run_jj(args: &[&str], error_prefix: &str) -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {}", error_prefix, e))?;

    let stdout_handle =
        crate::runner::drain_pipe(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        crate::runner::drain_pipe(child.stderr.take().expect("stderr must be piped"));

    let status = crate::runner::wait_with_timeout(error_prefix, &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("{}: {}", error_prefix, e))?;

    let stdout_text = stdout_handle.join().unwrap_or_default();
    let stderr_text = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!(
            "{}: タイムアウト ({}s)",
            error_prefix, JJ_TIMEOUT_SECS
        )),
        Some(s) if s.success() => Ok(stdout_text),
        Some(_) => Err(stderr_text.trim().to_string()),
    }
}

fn set_bookmark(name: &str, target: &str) -> Result<(), String> {
    run_jj(
        &["bookmark", "set", "-r", target, "--", name],
        "jj bookmark set 実行失敗",
    )?;
    Ok(())
}

fn run_jj_log(revset: &str, template: &str) -> Result<String, String> {
    run_jj(
        &["log", "-r", revset, "--no-graph", "-T", template],
        "jj log 実行失敗",
    )
}

/// `jj bookmark list` の出力をパースし、非 trunk のローカル bookmark 名を返す。
/// 出力形式: "name: commit_id description\n  @origin: commit_id\n"
/// インデントで始まる行はリモート追跡情報なのでスキップする。
fn get_local_bookmarks_from_list() -> Result<Vec<String>, String> {
    let output = run_jj(&["bookmark", "list"], "jj bookmark list 実行失敗")?;
    Ok(dedup(parse_bookmark_list_output(&output)))
}

fn parse_bookmark_list_output(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('\t'))
        .filter_map(|line| line.split(':').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
        .collect()
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- dedup ---

    #[test]
    fn dedup_preserves_order_and_removes_duplicates() {
        let input = vec![
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
            "b".to_string(),
        ];
        assert_eq!(dedup(input), vec!["a", "b", "c"]);
    }

    #[test]
    fn dedup_empty_returns_empty() {
        assert_eq!(dedup(Vec::new()), Vec::<String>::new());
    }

    #[test]
    fn dedup_single_unchanged() {
        assert_eq!(dedup(vec!["x".to_string()]), vec!["x"]);
    }

    // --- parse_bookmark_list_output ---

    #[test]
    fn parse_bookmark_list_typical_output() {
        let output = "\
feat/xyz: abc1234 add feature
  @origin: abc1234 add feature
main: def5678 initial
  @origin: def5678 initial
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/xyz"]);
    }

    #[test]
    fn parse_bookmark_list_multiple_feature_bookmarks() {
        let output = "\
feat/a: 111 desc
feat/b: 222 desc
main: 333 desc
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/a", "feat/b"]);
    }

    #[test]
    fn parse_bookmark_list_empty_output() {
        assert_eq!(parse_bookmark_list_output(""), Vec::<String>::new());
    }

    #[test]
    fn parse_bookmark_list_only_trunk() {
        let output = "main: abc123 desc\nmaster: def456 desc\n";
        assert_eq!(parse_bookmark_list_output(output), Vec::<String>::new());
    }

    #[test]
    fn parse_bookmark_list_skips_indented_remote_lines() {
        let output = "\
feat/xyz: abc1234 desc
  @origin: abc1234 desc
  @upstream: abc1234 desc
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/xyz"]);
    }

    // --- dispatch_bookmark_advance ---

    #[test]
    fn dispatch_zero_bookmarks_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(&[], "abc123", |_, _| called.set(true));
        assert!(!called.get());
    }

    #[test]
    fn dispatch_one_bookmark_calls_apply_with_correct_args() {
        let captured = std::cell::RefCell::new(None::<(Vec<String>, String)>);
        dispatch_bookmark_advance(&["feat/xyz".to_string()], "abc123", |b, t| {
            *captured.borrow_mut() = Some((b.to_vec(), t.to_string()))
        });
        assert_eq!(
            *captured.borrow(),
            Some((vec!["feat/xyz".to_string()], "abc123".to_string()))
        );
    }

    #[test]
    fn dispatch_multiple_bookmarks_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(
            &["feat/a".to_string(), "feat/b".to_string()],
            "abc123",
            |_, _| called.set(true),
        );
        assert!(!called.get());
    }

    // ─── 統合テスト (外部依存: jj CLI) ───
    //
    // 実 jj プロセスと working copy を使い、PR #53 で観測した症状 (auto re-push で
    // bookmark が旧 commit から動かない) の退行を防ぐ。
    //
    // 実行方法 (push-runner-config.toml の rust-test group と同じ):
    //   cargo test --manifest-path src/cli-pr-monitor/Cargo.toml -- --ignored --test-threads=1
    //
    // --test-threads=1 は `std::env::set_current_dir` の同時呼び出しを避けるため。
    // push pipeline (push-runner-config.toml) でのみ実行することを想定し、
    // PostToolUse / Stop hook では走らせない。

    /// cwd を Drop タイミングで元に戻す RAII ガード (ADR-025)。
    /// panic でテストが中断しても cwd が復元されることを保証する。
    struct CwdRestore {
        original: std::path::PathBuf,
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    /// 指定した name の bookmark が指す commit id を取得する。見つからなければ None。
    /// revset として bookmark 名を直接指定し、解決先 commit の id を取得する。
    fn bookmark_commit_id(repo: &std::path::Path, name: &str) -> Option<String> {
        let output = std::process::Command::new("jj")
            .args([
                "log",
                "-r",
                name,
                "--no-graph",
                "-T",
                "commit_id ++ \"\\n\"",
            ])
            .current_dir(repo)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&output.stdout);
        let first = s.lines().next().map(|line| line.trim().to_string())?;
        if first.is_empty() {
            None
        } else {
            Some(first)
        }
    }

    /// 統合: `jj new` 直後 (empty @) で bookmark が旧 commit にある状態から
    /// `advance_jj_bookmarks` を呼ぶと、bookmark が `@-` (fix commit) に前進する。
    ///
    /// PR #53 の症状再現: auto re-push 直前に bookmark が動かず remote 未反映だった。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_advance_moves_bookmark_to_parent_after_jj_new() {
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

        // 2. C1 を作成: ファイル追加 + describe
        std::fs::write(repo_dir.join("a.txt"), "content1\n").expect("write a.txt 失敗");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", "C1"])
            .current_dir(repo_dir)
            .status()
            .expect("jj describe C1 失敗")
            .success());

        // 3. bookmark feat/test を C1 (=@) に設定
        assert!(StdCommand::new("jj")
            .args(["bookmark", "create", "feat/test", "-r", "@"])
            .current_dir(repo_dir)
            .status()
            .expect("jj bookmark create 失敗")
            .success());
        let c1_id = bookmark_commit_id(repo_dir, "feat/test").expect("feat/test の c1_id 取得失敗");

        // 4. C2 を作成: jj new + ファイル追加 + describe (takt fix 相当の子 commit)
        assert!(StdCommand::new("jj")
            .args(["new"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new C2 失敗")
            .success());
        std::fs::write(repo_dir.join("b.txt"), "content2\n").expect("write b.txt 失敗");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", "C2 (fix commit)"])
            .current_dir(repo_dir)
            .status()
            .expect("jj describe C2 失敗")
            .success());

        // 5. run_push の step 1 相当: jj new で @ を空にする
        //    (@ = empty, @- = C2, bookmark feat/test は C1 のまま)
        assert!(StdCommand::new("jj")
            .args(["new"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new (empty @) 失敗")
            .success());

        // advance 呼び出し前に @- の commit id を取得 (= C2、fix commit 相当)。
        // PR #53 症状の退行防止として「@- に動いた」ことを厳密に検証するため。
        let expected_id = bookmark_commit_id(repo_dir, "@-").expect("@- の id 取得失敗");

        // 6. cwd を tempdir に切り替え (advance_jj_bookmarks は cwd 依存)
        let original_cwd = std::env::current_dir().expect("cwd 取得失敗");
        std::env::set_current_dir(repo_dir).expect("cd 失敗");
        let _cwd_guard = CwdRestore {
            original: original_cwd,
        };

        // 7. advance_jj_bookmarks 実行
        //    期待: target = @- (C2)、bookmark feat/test が C1 → C2 に前進
        let result = advance_jj_bookmarks();
        assert!(result.is_ok(), "advance_jj_bookmarks が失敗: {:?}", result);

        // 8. bookmark が @- (fix commit C2) に前進したことを厳密に確認。
        //    assert_ne! で「stuck at C1 ではない」ことも併記し、失敗時の triage を分かりやすくする。
        let after_id = bookmark_commit_id(repo_dir, "feat/test").expect("advance 後の id 取得失敗");
        assert_ne!(
            after_id, c1_id,
            "bookmark feat/test が前進していない (stuck at C1={})",
            c1_id
        );
        assert_eq!(
            after_id, expected_id,
            "bookmark feat/test が期待した commit (@-={}) ではなく {} に動いた",
            expected_id, after_id
        );

        // cwd は `_cwd_guard` の Drop で自動復元される
    }
}
