//! bookmark 探索 — trunk 判定 / revset 走査 / ローカル・リモート追跡 bookmark の取得。
//!
//! ADR-021 原則 5 (bookmark 検出) と ADR-024 (共有ヘルパー) の実装本体。
//! 副作用 (jj subprocess・ログ) は呼び出し側から注入する方針は crate doc を参照。

use std::process::{Command, Stdio};

/// PR / bookmark 検出から除外する trunk 系 bookmark 名。
pub const TRUNK_BOOKMARKS: &[&str] = &["main", "master", "trunk", "develop"];

/// `TRUNK_BOOKMARKS` に含まれる名前であれば `true`。
pub fn is_trunk_bookmark(name: &str) -> bool {
    TRUNK_BOOKMARKS.contains(&name)
}

/// Bookmark 検索に使用する revset のリスト (近い順 = 優先順)。
///
/// [`select_from_revsets`] は先頭から順に試し、最初に (trunk 除外後の)
/// bookmark が見つかった時点で後続の revset を検索しない
/// ("@" で見つかれば "@--" は触らない)。
///
/// - `@`: 標準 `git` ブランチ運用、または bookmark が現在のコミット上にある場合
/// - `@-`: `jj new` で空 `@` を作った直後 (PR #53 で実測)
/// - `@--`: 連続 `jj new` や中間空コミット運用向けのフォールバック
pub const BOOKMARK_SEARCH_REVSETS: &[&str] = &["@", "@-", "@--"];

/// jj サブプロセスの stderr ハンドリング方針。
///
/// 失敗時の jj stderr (不正な revset 指定や jj 非互換テンプレート等) を
/// どう扱うかを呼び出し側が選ぶ。
pub enum StderrMode {
    /// stderr を捨てる (`Stdio::null`)。CI ログを汚したくない場合。
    Silent,
    /// stderr を捕捉し、非空であれば引数のログ関数に渡す。
    Piped(fn(&str)),
}

/// `jj log` テンプレート出力 (カンマ区切り × 行) からユニークな bookmark 名を抽出する。
/// trunk 系 bookmark は除外する。
///
/// 想定テンプレート: `local_bookmarks.map(|b| b.name()).join(",") ++ "\n"`
pub fn parse_bookmark_list_output(raw: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for line in raw.lines() {
        for name in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if is_trunk_bookmark(name) {
                continue;
            }
            let name = name.to_string();
            if !seen.contains(&name) {
                seen.push(name);
            }
        }
    }
    seen
}

/// 指定 revset の**ローカル** bookmark 名を `jj log` で取得する (I/O)。
///
/// `stderr_mode` で stderr の扱いを指定する。
/// revset 不正や jj テンプレート非互換等の失敗時は空 Vec を返す。
pub fn query_bookmarks_at(revset: &str, stderr_mode: &StderrMode) -> Vec<String> {
    query_bookmarks_with_template(
        revset,
        "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
        stderr_mode,
    )
}

/// 指定 revset の**リモート追跡** bookmark 名を `jj log` で取得する (I/O)。
///
/// テンプレートの `b.name()` は remote 名を含まない bare な bookmark 名を返すため
/// (`claude/nightly-163@origin` → `claude/nightly-163`)、そのまま `gh pr list --head`
/// や `git` の branch 名として使える (jj 0.42.0 で実機確認)。
///
/// colocated リポジトリでは、ローカル bookmark の `@git` 複製も同じ名前で列挙される。
/// 重複は [`parse_bookmark_list_output`] が畳み込むため呼び出し側の考慮は不要。
pub fn query_remote_bookmarks_at(revset: &str, stderr_mode: &StderrMode) -> Vec<String> {
    query_bookmarks_with_template(
        revset,
        "remote_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
        stderr_mode,
    )
}

/// `jj log -T <template>` を実行して bookmark 名リストを得る共通処理。
fn query_bookmarks_with_template(
    revset: &str,
    template: &str,
    stderr_mode: &StderrMode,
) -> Vec<String> {
    let mut cmd = Command::new("jj");
    cmd.args(["log", "-r", revset, "--no-graph", "-T", template])
        .stdout(Stdio::piped());

    cmd.stderr(match stderr_mode {
        StderrMode::Silent => Stdio::null(),
        StderrMode::Piped(_) => Stdio::piped(),
    });

    let output = match cmd.output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            if let StderrMode::Piped(log) = stderr_mode {
                let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                if !stderr.is_empty() {
                    log(&format!(
                        "jj bookmark 取得失敗 (revset={}): {}",
                        revset, stderr
                    ));
                }
            }
            return Vec::new();
        }
        Err(e) => {
            if let StderrMode::Piped(log) = stderr_mode {
                log(&format!("jj コマンド実行失敗: {}", e));
            }
            return Vec::new();
        }
    };

    parse_bookmark_list_output(&String::from_utf8_lossy(&output.stdout))
}

/// 指定 revset を優先順に試し、最初に非空の bookmark リストを得た revset の結果を返す。
///
/// `fallback_log` を渡すと、先頭以外の revset で bookmark が検出された場合に
/// "revset '@-' で bookmark を検出: [...]" 形式のメッセージを記録する。
///
/// テスト用に `query` をクロージャで注入できる pure function。
pub fn select_from_revsets<F>(
    revsets: &[&str],
    query: F,
    fallback_log: Option<fn(&str)>,
) -> Vec<String>
where
    F: Fn(&str) -> Vec<String>,
{
    for (i, revset) in revsets.iter().enumerate() {
        let bookmarks = query(revset);
        if !bookmarks.is_empty() {
            if i > 0 {
                if let Some(log) = fallback_log {
                    log(&format!(
                        "revset '{}' で bookmark を検出: {:?}",
                        revset, bookmarks
                    ));
                }
            }
            return bookmarks;
        }
    }
    Vec::new()
}

/// [`BOOKMARK_SEARCH_REVSETS`] を優先順に走査し、最初に見つかった
/// (trunk 除外後の) bookmark を返す。
///
/// - `stderr_mode`: `jj log` の stderr ハンドリング方針
/// - `fallback_log`: `@` 以外の revset で hit した場合の通知 (`None` なら無通知)
pub fn get_jj_bookmarks(stderr_mode: StderrMode, fallback_log: Option<fn(&str)>) -> Vec<String> {
    select_from_revsets(
        BOOKMARK_SEARCH_REVSETS,
        |r| query_bookmarks_at(r, &stderr_mode),
        fallback_log,
    )
}

/// [`get_jj_bookmarks_with_remote_fallback`] の結果。空 Vec を持つ variant は返らない。
#[derive(Debug, PartialEq, Eq)]
pub enum BookmarkSearch {
    /// ローカル bookmark で解決した (従来と同じ経路)。
    Local(Vec<String>),
    /// ローカル bookmark が無く、リモート追跡 bookmark で解決した。
    /// `jj bookmark track` されていない = ローカルに実体が無い状態なので、
    /// 呼び出し側が bookmark をローカル操作 (`jj bookmark set` 等) の対象にするのは誤り。
    RemoteOnly(Vec<String>),
    /// どちらにも bookmark が無い。
    NotFound,
}

/// ローカル bookmark を優先し、無ければリモート追跡 bookmark へフォールバックして探索する。
///
/// bot が remote に作った PR を人間がマージする経路 (ADR-072 の夜間ループ) では、
/// PR の head が `claude/nightly-163@origin` のようなリモート専用 bookmark しか持たず、
/// [`get_jj_bookmarks`] (ローカルのみ) では検出できない (順位 397 で実測)。
///
/// **ローカルを先に全 revset 走査してからリモートへ移る**二段構成にしてあり、
/// ローカル bookmark が 1 つでも見つかる状況では [`get_jj_bookmarks`] と結果が一致する
/// (共有ライブラリの既存呼び出し側 = push-runner / pr-monitor への回帰を避けるため、
/// ADR-024)。読み取り専用の PR 検出用途を想定した API で、bookmark を書き換える経路は
/// `RemoteOnly` を区別して扱うこと。
pub fn get_jj_bookmarks_with_remote_fallback(
    stderr_mode: StderrMode,
    fallback_log: Option<fn(&str)>,
) -> BookmarkSearch {
    select_with_remote_fallback(
        BOOKMARK_SEARCH_REVSETS,
        |r| query_bookmarks_at(r, &stderr_mode),
        |r| query_remote_bookmarks_at(r, &stderr_mode),
        fallback_log,
    )
}

/// [`get_jj_bookmarks_with_remote_fallback`] の探索順序を、注入した query で検証可能にした pure function。
///
/// `local` / `remote` はそれぞれ revset を受け取り bookmark 名を返す。
pub fn select_with_remote_fallback<L, R>(
    revsets: &[&str],
    local: L,
    remote: R,
    fallback_log: Option<fn(&str)>,
) -> BookmarkSearch
where
    L: Fn(&str) -> Vec<String>,
    R: Fn(&str) -> Vec<String>,
{
    let found = select_from_revsets(revsets, local, fallback_log);
    if !found.is_empty() {
        return BookmarkSearch::Local(found);
    }
    let found = select_from_revsets(revsets, remote, fallback_log);
    if found.is_empty() {
        BookmarkSearch::NotFound
    } else {
        BookmarkSearch::RemoteOnly(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let result = select_with_remote_fallback(&["@", "@-"], none, none, None);
            assert_eq!(result, BookmarkSearch::NotFound);
        }

        #[test]
        fn does_not_query_remote_when_local_hits() {
            let remote_calls = RefCell::new(0u32);
            let result = select_with_remote_fallback(
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

        #[test]
        fn remote_search_also_walks_revsets_in_order() {
            let result = select_with_remote_fallback(
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
}
