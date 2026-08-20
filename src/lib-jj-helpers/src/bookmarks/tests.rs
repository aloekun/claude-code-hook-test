use super::*;

/// 順位 386: 探索 revset の構成を pin する。深い側が消えると 3 段制限の
/// 検出不能 (計 9 回実観測) が黙って再発し、remote 側の分離が消えると
/// remote 専用 bookmark の検出 (順位 397) が黙って壊れる。
#[test]
fn search_revsets_pin_depth_independent_variants() {
    assert_eq!(BOOKMARK_SEARCH_REVSETS, &["@", "heads(::@ & bookmarks())"]);
    assert_eq!(
        REMOTE_BOOKMARK_SEARCH_REVSETS,
        &["@", "heads(::@ & remote_bookmarks())"]
    );
    assert_eq!(
        ADVANCE_TARGET_REVSET,
        "heads(::@ ~ description(exact:\"\"))",
        "advance 先は「説明のあるコミット」で選ぶ (jj の push 拒否条件と同じ軸)"
    );
}

#[test]
fn classify_advance_target_by_candidate_count() {
    assert_eq!(classify_advance_target(vec![]), AdvanceTarget::None);
    assert_eq!(
        classify_advance_target(vec!["abc123".into()]),
        AdvanceTarget::Commit("abc123".into())
    );
    assert_eq!(
        classify_advance_target(vec!["abc123".into(), "def456".into()]),
        AdvanceTarget::Ambiguous(vec!["abc123".into(), "def456".into()]),
        "マージ祖先等の複数候補は ambiguous (advance skip) に倒す"
    );
}

#[test]
fn is_trunk_bookmark_known_names_rejected() {
    assert!(is_trunk_bookmark("main"));
    assert!(is_trunk_bookmark("master"));
    assert!(is_trunk_bookmark("trunk"));
    assert!(is_trunk_bookmark("develop"));
    assert!(!is_trunk_bookmark("feat/x"));
    assert!(!is_trunk_bookmark("main-feature"));
}

#[test]
fn parse_bookmark_list_output_empty() {
    assert!(parse_bookmark_list_output("").is_empty());
    assert!(parse_bookmark_list_output("\n\n").is_empty());
}

#[test]
fn parse_bookmark_list_output_single() {
    assert_eq!(parse_bookmark_list_output("feat/x\n"), vec!["feat/x"]);
}

#[test]
fn parse_bookmark_list_output_csv_on_one_line() {
    assert_eq!(
        parse_bookmark_list_output("feat/a,feat/b\n"),
        vec!["feat/a", "feat/b"]
    );
}

#[test]
fn parse_bookmark_list_output_multiple_lines() {
    let raw = "feat/current\nfeat/parent\n";
    assert_eq!(
        parse_bookmark_list_output(raw),
        vec!["feat/current", "feat/parent"]
    );
}

#[test]
fn parse_bookmark_list_output_deduplicates() {
    let raw = "feat/x,feat/x\nfeat/x\n";
    assert_eq!(parse_bookmark_list_output(raw), vec!["feat/x"]);
}

#[test]
fn parse_bookmark_list_output_trims_whitespace() {
    assert_eq!(
        parse_bookmark_list_output("  feat/a ,  feat/b  \n"),
        vec!["feat/a", "feat/b"]
    );
}

#[test]
fn parse_bookmark_list_output_excludes_trunk_bookmarks() {
    assert!(parse_bookmark_list_output("master\n").is_empty());
    assert_eq!(
        parse_bookmark_list_output("master,feat/x\n"),
        vec!["feat/x"]
    );
}

#[test]
fn select_from_revsets_returns_empty_when_all_revsets_empty() {
    let result = select_from_revsets(&["@", "@-"], |_| Vec::new(), None);
    assert!(result.is_empty());
}

#[test]
fn select_from_revsets_prefers_current_over_parent() {
    let result = select_from_revsets(
        &["@", "@-"],
        |r| match r {
            "@" => vec!["feat/current".to_string()],
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        },
        None,
    );
    assert_eq!(result, vec!["feat/current"]);
}

#[test]
fn select_from_revsets_falls_back_to_parent_when_current_empty() {
    // create_pr.rs の --head 自動補完ケース: @ 空 / @- に feature bookmark
    let result = select_from_revsets(
        &["@", "@-"],
        |r| match r {
            "@" => Vec::new(),
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        },
        None,
    );
    assert_eq!(result, vec!["feat/parent"]);
}

#[test]
fn select_from_revsets_stops_at_first_hit() {
    use std::cell::RefCell;
    let calls = RefCell::new(Vec::<String>::new());
    let result = select_from_revsets(
        &["@", "@-", "@--"],
        |r| {
            calls.borrow_mut().push(r.to_string());
            if r == "@-" {
                vec!["feat/hit".to_string()]
            } else {
                Vec::new()
            }
        },
        None,
    );
    assert_eq!(result, vec!["feat/hit"]);
    assert_eq!(*calls.borrow(), vec!["@".to_string(), "@-".to_string()]);
}

#[test]
fn select_from_revsets_invokes_fallback_log_when_non_first_hit() {
    use std::cell::RefCell;
    thread_local! {
        static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    fn record(msg: &str) {
        LOGGED.with(|l| l.borrow_mut().push(msg.to_string()));
    }
    LOGGED.with(|l| l.borrow_mut().clear());

    let result = select_from_revsets(
        &["@", "@-"],
        |r| match r {
            "@" => Vec::new(),
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        },
        Some(record),
    );
    assert_eq!(result, vec!["feat/parent"]);
    LOGGED.with(|l| {
        let logged = l.borrow();
        assert_eq!(logged.len(), 1);
        assert!(logged[0].contains("'@-'"));
        assert!(logged[0].contains("feat/parent"));
    });
}

#[test]
fn select_from_revsets_does_not_invoke_fallback_log_for_first_hit() {
    use std::cell::RefCell;
    thread_local! {
        static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    fn record(msg: &str) {
        LOGGED.with(|l| l.borrow_mut().push(msg.to_string()));
    }
    LOGGED.with(|l| l.borrow_mut().clear());

    let result = select_from_revsets(
        &["@", "@-"],
        |r| match r {
            "@" => vec!["feat/current".to_string()],
            _ => Vec::new(),
        },
        Some(record),
    );
    assert_eq!(result, vec!["feat/current"]);
    LOGGED.with(|l| assert!(l.borrow().is_empty()));
}

/// remote フォールバック (順位 397) の探索順序。
///
/// 「ローカルがあればローカル」「無ければリモート」の 2 分岐に加え、
/// **ローカルが見つかったらリモートを一切引かない** ことを呼び出し記録で固定する
/// (既存 3 クレートへの回帰が無いことの担保。ADR-024)。
mod remote_fallback {
    use super::super::*;
    use std::cell::RefCell;

    fn none(_: &str) -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn prefers_local_when_present() {
        let result = select_with_remote_fallback(
            &["@", "@-"],
            &["@", "@-"],
            |r| match r {
                "@" => vec!["feat/local".to_string()],
                _ => Vec::new(),
            },
            |_| vec!["feat/remote".to_string()],
            None,
        );
        assert_eq!(
            result,
            BookmarkSearch::Local(vec!["feat/local".to_string()])
        );
    }

    /// 夜間ループの PR (`claude/nightly-163@origin`) を人間がマージする状況。
    #[test]
    fn falls_back_to_remote_when_no_local_bookmark() {
        let result = select_with_remote_fallback(
            &["@", "@-"],
            &["@", "@-"],
            none,
            |r| match r {
                "@-" => vec!["claude/nightly-163".to_string()],
                _ => Vec::new(),
            },
            None,
        );
        assert_eq!(
            result,
            BookmarkSearch::RemoteOnly(vec!["claude/nightly-163".to_string()])
        );
    }

    #[test]
    fn not_found_when_neither_side_has_bookmark() {
        let result = select_with_remote_fallback(&["@", "@-"], &["@", "@-"], none, none, None);
        assert_eq!(result, BookmarkSearch::NotFound);
    }

    #[test]
    fn does_not_query_remote_when_local_hits() {
        let remote_calls = RefCell::new(0u32);
        let result = select_with_remote_fallback(
            &["@", "@-", "@--"],
            &["@", "@-", "@--"],
            |r| match r {
                "@-" => vec!["feat/local".to_string()],
                _ => Vec::new(),
            },
            |_| {
                *remote_calls.borrow_mut() += 1;
                vec!["feat/remote".to_string()]
            },
            None,
        );
        assert_eq!(
            result,
            BookmarkSearch::Local(vec!["feat/local".to_string()])
        );
        assert_eq!(
            *remote_calls.borrow(),
            0,
            "ローカルで解決した場合はリモート探索の jj 呼び出しを行わないこと"
        );
    }

    /// 順位 386: リモート探索は**リモート用リスト**で走ること。
    /// 深い側の revset が bookmark フィルタを含むため、ローカル用リストを
    /// リモート探索に流用すると remote 専用 bookmark を検出できない
    /// (退行するとこのテストの remote closure に local 用 revset が渡る)。
    #[test]
    fn remote_search_uses_remote_revset_list() {
        let seen = RefCell::new(Vec::new());
        let result = select_with_remote_fallback(
            &["local-only-revset"],
            &["remote-only-revset"],
            none,
            |r| {
                seen.borrow_mut().push(r.to_string());
                vec!["feat/remote".to_string()]
            },
            None,
        );
        assert_eq!(
            result,
            BookmarkSearch::RemoteOnly(vec!["feat/remote".to_string()])
        );
        assert_eq!(
            *seen.borrow(),
            vec!["remote-only-revset".to_string()],
            "リモート探索にはリモート用 revset リストだけが渡ること"
        );
    }

    #[test]
    fn remote_search_also_walks_revsets_in_order() {
        let result = select_with_remote_fallback(
            &["@", "@-", "@--"],
            &["@", "@-", "@--"],
            none,
            |r| match r {
                "@-" => vec!["feat/near".to_string()],
                "@--" => vec!["feat/far".to_string()],
                _ => Vec::new(),
            },
            None,
        );
        assert_eq!(
            result,
            BookmarkSearch::RemoteOnly(vec!["feat/near".to_string()]),
            "リモート側も近い revset を優先すること"
        );
    }
}

/// 実 jj で「remote 専用 bookmark しか無い」状態を組み、テンプレートと探索が
/// jj の実挙動と一致することを確認する統合テスト (順位 397 の回帰テスト)。
mod real_jj {
    use super::super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command as StdCommand;

    fn jj(dir: &Path, args: &[&str]) {
        let ok = StdCommand::new("jj")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap_or_else(|e| panic!("jj {:?} 実行失敗: {}", args, e))
            .success();
        assert!(ok, "jj {:?} が失敗", args);
    }

    /// origin 側リポジトリを `master` だけを持つ状態で作る。
    fn init_origin_with_master(origin: &Path) {
        std::fs::create_dir_all(origin).unwrap();
        jj(origin, &["git", "init", "--colocate"]);
        std::fs::write(origin.join("base.txt"), "base\n").unwrap();
        jj(origin, &["describe", "-m", "chore: base"]);
        jj(origin, &["bookmark", "create", "master", "-r", "@"]);
        jj(origin, &["new"]);
    }

    /// origin 側へ新しい bookmark を追加する (= bot が remote に PR ブランチを作る)。
    fn push_new_bookmark_on_origin(origin: &Path, name: &str) {
        std::fs::write(origin.join("f.txt"), "x\n").unwrap();
        jj(origin, &["describe", "-m", "feat: x"]);
        jj(origin, &["bookmark", "create", name, "-r", "@"]);
        jj(origin, &["new"]);
    }

    struct CwdRestore {
        original: PathBuf,
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    fn enter(dir: &Path) -> CwdRestore {
        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir).expect("cd");
        CwdRestore { original }
    }

    /// stdout を捕捉する jj 実行 helper (revset の実挙動検証用)。
    fn jj_stdout(dir: &Path, args: &[&str]) -> String {
        let out = StdCommand::new("jj")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("jj {:?} 実行失敗: {}", args, e));
        assert!(out.status.success(), "jj {:?} が失敗", args);
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// 順位 386 の実観測形を組む: bookmark の上に説明なし空コミットが 4 段。
    /// (監視・自動 fix 経路が `jj new` を積んだ状態。実 incident は計 9 回観測)
    fn init_repo_with_deep_bookmark(repo: &Path) {
        std::fs::create_dir_all(repo).unwrap();
        jj(repo, &["git", "init", "--colocate"]);
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        jj(repo, &["describe", "-m", "chore: base"]);
        jj(repo, &["bookmark", "create", "master", "-r", "@"]);
        jj(repo, &["new", "-m", "feat: work"]);
        std::fs::write(repo.join("f.txt"), "x\n").unwrap();
        jj(repo, &["bookmark", "create", "feat/deep", "-r", "@"]);
        for _ in 0..4 {
            jj(repo, &["new"]);
        }
    }

    /// 順位 386 症状 1 の回帰テスト: bookmark が説明なし空コミット 4 段の先に
    /// あっても検出できること。旧構成 (`["@", "@-", "@--"]`) では 3 段しか
    /// 遡れず空振りした (= `pnpm merge-pr` の PR 未検出、計 9 回実観測)。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn deep_bookmark_behind_descless_commits_is_found() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        init_repo_with_deep_bookmark(&repo);
        let _guard = enter(&repo);

        assert!(
            query_bookmarks_at("@-", &StderrMode::Silent).is_empty()
                && query_bookmarks_at("@--", &StderrMode::Silent).is_empty(),
            "旧構成の revset では 4 段先の bookmark に届かない (これが順位 386 の原因)"
        );
        assert_eq!(
            get_jj_bookmarks(StderrMode::Silent, None),
            vec!["feat/deep".to_string()],
            "深さ非依存 revset で 4 段先の bookmark を解決できること"
        );
    }

    /// 順位 386 症状 2 の回帰テスト: advance の移動先 revset が説明なしコミットを
    /// 何段でも飛ばし、bookmark の乗る「説明のあるコミット」をちょうど 1 件返すこと。
    /// 旧規則は説明なしコミットへ bookmark を移し push が
    /// `Won't push commit ... since it has no description` で落ちた (#370)。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn advance_target_revset_skips_descless_commits() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        init_repo_with_deep_bookmark(&repo);
        // 症状 2 の形: 説明なしの @ に uncommitted 変更まで載っている
        std::fs::write(repo.join("dirty.txt"), "dirty\n").unwrap();

        let expected = jj_stdout(&repo, &["log", "-r", "feat/deep", "--no-graph", "-T", "commit_id"]);
        let raw = jj_stdout(
            &repo,
            &["log", "-r", ADVANCE_TARGET_REVSET, "--no-graph", "-T", "commit_id ++ \"\\n\""],
        );
        let ids: Vec<String> = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        assert_eq!(
            classify_advance_target(ids),
            AdvanceTarget::Commit(expected.trim().to_string()),
            "advance 先は説明なし 4 段を飛ばした「説明のあるコミット」1 件に解決すること"
        );
    }

    /// 再現手順は 2026-08-10 に jj 0.42.0 で実測したもの: origin を clone した後に
    /// origin へ bookmark を作り clone 側で fetch すると `feat/x@origin [new] untracked`
    /// となり **ローカル bookmark は作られない** (`git.auto-local-bookmark` 既定値)。
    /// これが夜間ループの PR をマージする際の状態そのものである。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn remote_only_bookmark_is_found_via_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("a");
        let clone = tmp.path().join("b");

        init_origin_with_master(&origin);
        jj(
            tmp.path(),
            &[
                "git",
                "clone",
                origin.to_string_lossy().as_ref(),
                clone.to_string_lossy().as_ref(),
            ],
        );
        push_new_bookmark_on_origin(&origin, "feat/x");
        jj(&clone, &["git", "fetch"]);
        jj(&clone, &["new", "feat/x@origin"]);

        let _guard = enter(&clone);

        assert!(
            query_bookmarks_at("@-", &StderrMode::Silent).is_empty(),
            "fetch しただけの bookmark はローカルには存在しない (これが順位 397 の原因)"
        );
        assert_eq!(
            query_remote_bookmarks_at("@-", &StderrMode::Silent),
            vec!["feat/x".to_string()],
            "リモート追跡 bookmark は remote 名を含まない bare な名前で取れること"
        );
        assert_eq!(
            get_jj_bookmarks_with_remote_fallback(StderrMode::Silent, None),
            BookmarkSearch::RemoteOnly(vec!["feat/x".to_string()]),
            "jj bookmark track 無しで PR ブランチ名を解決できること"
        );
        assert!(
            get_jj_bookmarks(StderrMode::Silent, None).is_empty(),
            "ローカルのみの従来 API は空のまま (この差分が順位 397 の症状)"
        );
    }
}
