//! jj helper utilities shared across cli-* crates.
//!
//! ADR-021 (jj 変更検出の原則、特に原則 5 の bookmark 検出) と
//! ADR-024 (共通 jj ヘルパーライブラリ、本採用) で定めた shared primitives
//! の実装。
//!
//! # 設計方針
//!
//! - **副作用 (jj subprocess、log 出力) は呼び出し側から注入**
//! - **stderr ハンドリングは `StderrMode` で選択**: cli-pr-monitor は `Silent`
//!   (CI ログの汚染回避)、cli-merge-pipeline は `Piped` (失敗原因の診断)
//! - **ログ prefix は呼び出し側の `log_info` 関数に委譲**: 各クレート固有の
//!   prefix (`[post-pr-monitor]` / `[merge-pipeline]` 等) を崩さない
//!
//! # 公開 API
//!
//! - [`TRUNK_BOOKMARKS`] / [`is_trunk_bookmark`]: trunk 系 bookmark 判定 (3 クレート共有)
//! - [`BOOKMARK_SEARCH_REVSETS`]: `@` / `@-` / `@--` の優先順位リスト
//! - [`StderrMode`]: jj サブプロセスの stderr 方針
//! - [`parse_bookmark_list_output`]: `jj log` テンプレート出力のパース (pure)
//! - [`query_bookmarks_at`]: 指定 revset の bookmark 取得 (I/O)
//! - [`select_from_revsets`]: revset リストを優先順に試す pure function
//! - [`get_jj_bookmarks`]: 上記を組み合わせた high-level エントリポイント
//! - [`resolve_git_dir`] / [`inject_git_dir_for_gh`]: 非 colocated jj workspace
//!   での gh 用 `GIT_DIR` 導出と自動注入 (ADR-045 恒久対策候補 1)
//! - [`pipeline_lock`]: 実行中 pipeline と Stop hook 品質ゲートの相互排他 (順位 280)

pub mod pipeline_lock;

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

/// 指定 revset の bookmark 名を `jj log` で取得する (I/O)。
///
/// `stderr_mode` で stderr の扱いを指定する。
/// revset 不正や jj テンプレート非互換等の失敗時は空 Vec を返す。
pub fn query_bookmarks_at(revset: &str, stderr_mode: &StderrMode) -> Vec<String> {
    let mut cmd = Command::new("jj");
    cmd.args([
        "log",
        "-r",
        revset,
        "--no-graph",
        "-T",
        "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
    ])
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

/// [`resolve_git_dir`] の結果。
pub enum GitDirResolution {
    /// cwd に `.git` が存在する (colocated) — 注入不要
    NotNeeded,
    /// 非 colocated jj workspace — 導出した git dir
    Resolved(std::path::PathBuf),
    /// 導出失敗 (jj リポジトリ外 / layout 不整合 / fs エラー)
    Unresolved(String),
}

/// workspace root から gh 用の git dir を導出する (I/O は fs 読み取りのみ)。
///
/// jj の secondary workspace (`jj workspace add` で作成) は colocated 化されず
/// `.git` を持たないため、gh がリポジトリを解決できない (ADR-045)。
/// jj の on-disk layout を辿って main リポジトリの git dir を求める:
///
/// 1. `<root>/.git` があれば [`GitDirResolution::NotNeeded`]
/// 2. `<root>/.jj/repo` がファイルなら内容が main repo store へのパス
///    (相対なら `<root>/.jj/` 基準)。ディレクトリなら自身が main workspace
/// 3. `<store>/store/git_target` の内容 (相対なら `<store>/store/` 基準) が
///    colocated git dir。git_target が無ければ jj 内部 store の `store/git`
pub fn resolve_git_dir(workspace_root: &std::path::Path) -> GitDirResolution {
    if workspace_root.join(".git").exists() {
        return GitDirResolution::NotNeeded;
    }

    let repo_entry = workspace_root.join(".jj").join("repo");
    let repo_store = if repo_entry.is_file() {
        match std::fs::read_to_string(&repo_entry) {
            Ok(content) => resolve_relative_to(content.trim(), &workspace_root.join(".jj")),
            Err(e) => {
                return GitDirResolution::Unresolved(format!(".jj/repo 読み取り失敗: {}", e))
            }
        }
    } else if repo_entry.is_dir() {
        repo_entry
    } else {
        return GitDirResolution::Unresolved(
            ".jj/repo が見つかりません (jj リポジトリ外?)".to_string(),
        );
    };

    let store = repo_store.join("store");
    let git_target = store.join("git_target");
    let git_dir = if git_target.is_file() {
        match std::fs::read_to_string(&git_target) {
            Ok(content) => resolve_relative_to(content.trim(), &store),
            Err(e) => {
                return GitDirResolution::Unresolved(format!("git_target 読み取り失敗: {}", e))
            }
        }
    } else {
        store.join("git")
    };

    match git_dir.canonicalize() {
        Ok(p) => GitDirResolution::Resolved(strip_windows_verbatim_prefix(&p)),
        Err(e) => GitDirResolution::Unresolved(format!(
            "導出した git dir が存在しません ({}): {}",
            git_dir.display(),
            e
        )),
    }
}

/// パス文字列を解決する: 絶対ならそのまま、相対なら `base` 基準で連結。
fn resolve_relative_to(path_str: &str, base: &std::path::Path) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(path_str);
    if p.is_absolute() {
        p
    } else {
        base.join(p)
    }
}

/// Windows の `canonicalize` が付ける verbatim prefix (`\\?\`) を剥がす。
/// git / gh は素のパスで動作し、`\\?\` 付きは外部ツールで問題を起こしやすい。
fn strip_windows_verbatim_prefix(p: &std::path::Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(stripped) => std::path::PathBuf::from(stripped),
        None => p.to_path_buf(),
    }
}

/// 非 colocated jj workspace で `GIT_DIR` を自動注入する (ADR-045 恒久対策候補 1)。
///
/// exe の main() 冒頭で 1 回呼ぶ。プロセス env に設定するため、以降に spawn する
/// gh 子プロセス全体へ伝播する。jj 自身は `GIT_DIR` を無視するため jj 操作には
/// 影響しない (ADR-045 で確認済み)。
///
/// - 既に `GIT_DIR` が設定済み → 尊重して no-op (手動指定・CI 環境を壊さない)
/// - cwd に `.git` がある colocated 環境 → no-op
/// - 導出失敗 → warning ログのみで続行 (fail-soft — colocated では本機能自体が
///   不要であり、失敗時の挙動は従来と同じ「gh が repo 解決に失敗」に留まるため)
pub fn inject_git_dir_for_gh(log_info: fn(&str)) {
    if std::env::var_os("GIT_DIR").is_some() {
        return;
    }
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(_) => return,
    };
    match resolve_git_dir(&cwd) {
        GitDirResolution::NotNeeded => {}
        GitDirResolution::Resolved(git_dir) => {
            std::env::set_var("GIT_DIR", &git_dir);
            log_info(&format!(
                "[env] GIT_DIR 自動注入 (非 colocated jj workspace): {}",
                git_dir.display()
            ));
        }
        GitDirResolution::Unresolved(reason) => {
            log_info(&format!(
                "[env] GIT_DIR 導出失敗 (gh の repo 解決は失敗する可能性): {}",
                reason
            ));
        }
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

    /// tempdir に jj の on-disk layout を模擬構築する (jj バイナリ不要の unit test 用)。
    /// 実レイアウトは 2026-07-03 に実機確認: secondary の `.jj/repo` はファイルで
    /// main store への相対パス、colocated main の `store/git_target` は `../../../.git`。
    mod git_dir {
        use super::super::*;
        use std::fs;

        fn make_colocated_main(root: &std::path::Path) {
            fs::create_dir_all(root.join(".git")).unwrap();
            fs::create_dir_all(root.join(".jj/repo/store")).unwrap();
            fs::write(root.join(".jj/repo/store/git_target"), "../../../.git").unwrap();
        }

        fn make_secondary_workspace(ws: &std::path::Path, main_store_rel: &str) {
            fs::create_dir_all(ws.join(".jj")).unwrap();
            fs::write(ws.join(".jj/repo"), main_store_rel).unwrap();
        }

        #[test]
        fn colocated_root_is_not_needed() {
            let tmp = tempfile::tempdir().unwrap();
            make_colocated_main(tmp.path());
            assert!(matches!(
                resolve_git_dir(tmp.path()),
                GitDirResolution::NotNeeded
            ));
        }

        #[test]
        fn secondary_workspace_resolves_to_main_git_dir() {
            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            let ws = tmp.path().join("ws");
            make_colocated_main(&main);
            make_secondary_workspace(&ws, "../../main/.jj/repo");

            match resolve_git_dir(&ws) {
                GitDirResolution::Resolved(p) => {
                    let expected = main.join(".git").canonicalize().unwrap();
                    assert_eq!(p.canonicalize().unwrap(), expected);
                    assert!(
                        !p.to_string_lossy().starts_with(r"\\?\"),
                        "verbatim prefix は剥がされていること: {:?}",
                        p
                    );
                }
                other => panic!("Resolved を期待: {:?}", debug_name(&other)),
            }
        }

        #[test]
        fn secondary_workspace_with_absolute_store_path_resolves() {
            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            let ws = tmp.path().join("ws");
            make_colocated_main(&main);
            let abs = main.join(".jj").join("repo");
            make_secondary_workspace(&ws, &abs.to_string_lossy());

            assert!(matches!(
                resolve_git_dir(&ws),
                GitDirResolution::Resolved(_)
            ));
        }

        #[test]
        fn main_workspace_without_git_target_falls_back_to_internal_store() {
            let tmp = tempfile::tempdir().unwrap();
            fs::create_dir_all(tmp.path().join(".jj/repo/store/git")).unwrap();

            match resolve_git_dir(tmp.path()) {
                GitDirResolution::Resolved(p) => {
                    assert!(p.ends_with("git"), "内部 git store を指すこと: {:?}", p);
                }
                other => panic!("Resolved を期待: {:?}", debug_name(&other)),
            }
        }

        #[test]
        fn non_jj_directory_is_unresolved() {
            let tmp = tempfile::tempdir().unwrap();
            assert!(matches!(
                resolve_git_dir(tmp.path()),
                GitDirResolution::Unresolved(_)
            ));
        }

        #[test]
        fn dangling_git_target_is_unresolved() {
            let tmp = tempfile::tempdir().unwrap();
            fs::create_dir_all(tmp.path().join(".jj/repo/store")).unwrap();
            fs::write(
                tmp.path().join(".jj/repo/store/git_target"),
                "../../../no-such-dir/.git",
            )
            .unwrap();

            assert!(matches!(
                resolve_git_dir(tmp.path()),
                GitDirResolution::Unresolved(_)
            ));
        }

        fn debug_name(r: &GitDirResolution) -> &'static str {
            match r {
                GitDirResolution::NotNeeded => "NotNeeded",
                GitDirResolution::Resolved(_) => "Resolved",
                GitDirResolution::Unresolved(_) => "Unresolved",
            }
        }

        /// 実 jj で colocated repo + secondary workspace を組み、実レイアウトとの
        /// 齟齬 (jj バージョン更新による layout 変更) を検出する統合テスト。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn real_jj_secondary_workspace_resolves_to_main_git() {
            use std::process::Command as StdCommand;

            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            fs::create_dir_all(&main).unwrap();

            let init_ok = StdCommand::new("jj")
                .args(["git", "init", "--colocate"])
                .current_dir(&main)
                .status()
                .expect("jj git init 実行失敗")
                .success();
            assert!(init_ok, "jj git init --colocate が失敗");

            let ws = tmp.path().join("ws");
            let add_ok = StdCommand::new("jj")
                .args(["workspace", "add", ws.to_string_lossy().as_ref()])
                .current_dir(&main)
                .status()
                .expect("jj workspace add 実行失敗")
                .success();
            assert!(add_ok, "jj workspace add が失敗");

            assert!(
                !ws.join(".git").exists(),
                "secondary workspace は .git を持たない前提 (持つなら本機能は不要になる)"
            );

            match resolve_git_dir(&ws) {
                GitDirResolution::Resolved(p) => {
                    let expected = main.join(".git").canonicalize().unwrap();
                    assert_eq!(p.canonicalize().unwrap(), expected);
                }
                other => panic!("Resolved を期待: {}", debug_name(&other)),
            }
        }
    }
}
