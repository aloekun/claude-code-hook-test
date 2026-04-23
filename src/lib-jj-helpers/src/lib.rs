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
}
