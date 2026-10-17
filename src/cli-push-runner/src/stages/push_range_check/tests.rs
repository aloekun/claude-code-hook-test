use super::*;

fn undescribed(change: &str, commit: &str, empty: bool) -> Undescribed {
    Undescribed { change_id: change.to_string(), commit_id: commit.to_string(), empty }
}


#[test]
fn revset_quotes_bookmark_and_excludes_root() {
    assert_eq!(
        undescribed_revset("fix/a-b"),
        "(remote_bookmarks(remote=exact:\"origin\")..\"fix/a-b\") & description(exact:\"\") ~ root()"
    );
}

#[test]
fn revset_string_escapes_quote_and_backslash() {
    assert_eq!(revset_string(r#"a"b\c"#), r#""a\"b\\c""#);
}


#[test]
fn empty_output_is_clean() {
    assert_eq!(classify("feat/x", Ok(String::new())), RangeCheck::Clean);
    assert_eq!(classify("feat/x", Ok("\n".to_string())), RangeCheck::Clean);
}

#[test]
fn lines_become_undescribed_commits_with_emptiness() {
    let raw = "kzqyvmlo1234\tab12cd34ef56\tempty\nwrsxtuvp5678\t9876fedcba01\tnonempty\n";
    assert_eq!(
        classify("feat/x", Ok(raw.to_string())),
        RangeCheck::Undescribed {
            bookmark: "feat/x".to_string(),
            commits: vec![
                undescribed("kzqyvmlo1234", "ab12cd34ef56", true),
                undescribed("wrsxtuvp5678", "9876fedcba01", false),
            ],
        }
    );
}

#[test]
fn query_error_is_unavailable() {
    assert_eq!(
        classify("feat/x", Err("lock".to_string())),
        RangeCheck::Unavailable { bookmark: "feat/x".to_string(), reason: "lock".to_string() }
    );
}

/// 解釈できない行を読み飛ばすと Clean へ黙って倒れる (fail-open) ため、Unavailable にする。
#[test]
fn unparsable_line_is_unavailable_not_clean() {
    for raw in ["garbage\n", "a\tb\tmaybe\n", "a\tb\n", "a\tb\tempty\textra\n"] {
        let result = classify("feat/x", Ok(raw.to_string()));
        assert!(
            matches!(result, RangeCheck::Unavailable { .. }),
            "{:?} は Unavailable であるべき: {:?}",
            raw,
            result
        );
    }
}


#[test]
fn clean_does_not_abort() {
    assert_eq!(abort_report(&RangeCheck::Clean), None);
    assert!(report(&[RangeCheck::Clean, RangeCheck::Clean]));
    assert!(report(&[]));
}

#[test]
fn undescribed_report_gives_fix_command_per_emptiness() {
    let result = RangeCheck::Undescribed {
        bookmark: "feat/x".to_string(),
        commits: vec![
            undescribed("kzqyvmlo1234", "ab12cd34ef56", true),
            undescribed("wrsxtuvp5678", "9876fedcba01", false),
        ],
    };
    let (summary, hint) = abort_report(&result).expect("中断するはず");
    assert!(summary.contains("feat/x"), "{}", summary);
    assert!(hint.contains("2 件"), "{}", hint);
    assert!(hint.contains("`jj abandon kzqyvmlo1234`"), "{}", hint);
    assert!(hint.contains("`jj describe -r wrsxtuvp5678 -m \"<説明>\"`"), "{}", hint);
    assert!(hint.contains("ab12cd34ef56"), "{}", hint);
    assert!(
        hint.contains("jj log -r 'remote_bookmarks(remote=exact:\"origin\")..\"feat/x\"'"),
        "{}",
        hint
    );
    assert!(!report(&[RangeCheck::Clean, result]));
}

#[test]
fn unavailable_aborts() {
    let result =
        RangeCheck::Unavailable { bookmark: "feat/x".to_string(), reason: "lock".to_string() };
    let (summary, _) = abort_report(&result).expect("中断するはず");
    assert!(summary.contains("lock"), "{}", summary);
    assert!(!report(&[result]));
}

/// 順位 537 の実 jj 回帰テスト。#484 で 2 回連続した経路を再現する:
/// `pnpm merge-pr` の同期が残す `jj new master@origin` (空・説明なし) の上で
/// `jj new -m X` から作業を始めると、空の親が push 範囲に入る。
mod real_jj {
    use super::super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command as StdCommand;

    fn jj(dir: &Path, args: &[&str]) {
        assert!(
            StdCommand::new("jj")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap_or_else(|e| panic!("jj {:?} 実行失敗: {}", args, e))
                .success(),
            "jj {:?} が失敗",
            args
        );
    }

    fn jj_stdout(dir: &Path, args: &[&str]) -> String {
        let out = StdCommand::new("jj")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("jj {:?} 実行失敗: {}", args, e));
        assert!(out.status.success(), "jj {:?} が失敗", args);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn change_of_head(dir: &Path) -> String {
        jj_stdout(dir, &["log", "--no-graph", "-r", "@", "-T", "change_id.short(12)"])
    }

    /// origin を作り、それを clone した作業 repo を返す。clone 直後は master@origin がある。
    fn clone_from_origin(root: &Path) -> PathBuf {
        let origin = root.join("o");
        std::fs::create_dir_all(&origin).unwrap();
        jj(&origin, &["git", "init", "--colocate"]);
        std::fs::write(origin.join("a.txt"), "base\n").unwrap();
        jj(&origin, &["describe", "-m", "chore: base"]);
        jj(&origin, &["bookmark", "create", "master", "-r", "@"]);
        jj(&origin, &["new"]);
        let work = root.join("w");
        jj(root, &["git", "clone", origin.to_str().unwrap(), work.to_str().unwrap()]);
        work
    }

    fn check(work: &Path, bookmark: &str) -> RangeCheck {
        classify(bookmark, query_undescribed(bookmark, Some(work)))
    }

    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn empty_base_left_by_merge_pr_is_detected_then_clean_after_abandon() {
        let tmp = tempfile::tempdir().unwrap();
        let work = clone_from_origin(tmp.path());

        jj(&work, &["new", "master@origin"]);
        let base_change = change_of_head(&work);
        jj(&work, &["new", "-m", "feat: x"]);
        std::fs::write(work.join("b.txt"), "x\n").unwrap();
        jj(&work, &["bookmark", "create", "feat/x", "-r", "@"]);

        match check(&work, "feat/x") {
            RangeCheck::Undescribed { bookmark, commits } => {
                assert_eq!(bookmark, "feat/x");
                assert_eq!(commits.len(), 1, "{:?}", commits);
                assert_eq!(commits[0].change_id, base_change);
                assert!(commits[0].empty, "merge-pr が残す基準コミットは空のはず");
            }
            other => panic!("説明の無い空の親を検出するはず: {:?}", other),
        }

        jj(&work, &["abandon", &base_change]);
        assert_eq!(check(&work, "feat/x"), RangeCheck::Clean);
    }

    /// 中身のある説明なしコミットは「中身あり」と判定され、describe で clean になる。
    /// push 範囲の外 (bookmark の祖先でない) にある説明なしコミットでは止めない。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn nonempty_in_range_detected_and_outside_range_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let work = clone_from_origin(tmp.path());

        jj(&work, &["new", "master@origin"]);
        std::fs::write(work.join("side.txt"), "side\n").unwrap();

        jj(&work, &["new", "master@origin"]);
        std::fs::write(work.join("c.txt"), "c\n").unwrap();
        let parent_change = change_of_head(&work);
        jj(&work, &["new", "-m", "feat: y"]);
        std::fs::write(work.join("d.txt"), "d\n").unwrap();
        jj(&work, &["bookmark", "create", "feat/y", "-r", "@"]);

        match check(&work, "feat/y") {
            RangeCheck::Undescribed { commits, .. } => {
                assert_eq!(commits.len(), 1, "範囲外の兄弟を含めてはならない: {:?}", commits);
                assert_eq!(commits[0].change_id, parent_change);
                assert!(!commits[0].empty);
            }
            other => panic!("説明の無い中身ありの親を検出するはず: {:?}", other),
        }

        jj(&work, &["describe", "-r", &parent_change, "-m", "feat: parent"]);
        assert_eq!(check(&work, "feat/y"), RangeCheck::Clean);
    }

    /// 照会できない場所 (jj repo でない) では Clean でなく Unavailable に倒れる (fail-closed)。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn outside_repo_is_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(check(tmp.path(), "feat/x"), RangeCheck::Unavailable { .. }));
    }
}
