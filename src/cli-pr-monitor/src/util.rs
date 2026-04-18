use std::process::Command;

use crate::log::log_info;
use crate::runner::run_gh_quiet;

pub(crate) struct PrInfo {
    pub(crate) pr_number: Option<u64>,
    pub(crate) repo: Option<String>,
    pub(crate) push_time: Option<String>,
}

/// PR 情報を取得する（多段フォールバック）
///
/// Strategy A: gh pr view (標準 git ブランチ環境)
/// Strategy B: jj bookmark -> gh pr list --head (jj 環境)
pub(crate) fn get_pr_info() -> PrInfo {
    let repo = run_gh_quiet(&[
        "repo",
        "view",
        "--json",
        "nameWithOwner",
        "-q",
        ".nameWithOwner",
    ]);

    // Strategy A: gh pr view (git ブランチが使える場合)
    let pr_number = run_gh_quiet(&["pr", "view", "--json", "number", "-q", ".number"])
        .and_then(|s| s.parse::<u64>().ok());

    if pr_number.is_some() {
        return PrInfo {
            pr_number,
            repo,
            push_time: None,
        };
    }

    // Strategy B: jj bookmark -> gh pr list --head (全ブックマークを順に試す)
    let bookmarks = get_jj_bookmarks();
    for bookmark in &bookmarks {
        log_info(&format!("jj bookmark '{}' を使用して PR を検索", bookmark));
        let pr_number = run_gh_quiet(&[
            "pr",
            "list",
            "--head",
            bookmark,
            "--json",
            "number",
            "-q",
            ".[0].number",
        ])
        .and_then(|s| s.parse::<u64>().ok());

        if pr_number.is_some() {
            return PrInfo {
                pr_number,
                repo,
                push_time: None,
            };
        }
    }

    PrInfo {
        pr_number: None,
        repo,
        push_time: None,
    }
}

/// PR URL (https://github.com/.../pull/123) から PR 番号を抽出する
pub(crate) fn parse_pr_number_from_url(output: &str) -> Option<u64> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.rfind("/pull/") {
            let num_str = &trimmed[pos + 6..];
            let num_part: String = num_str.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_part.parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

/// Bookmark 検索に使用する revset のリスト (近い順 = 優先順)。
///
/// `select_from_revsets` は先頭から順に試し、最初に (trunk 除外後の) bookmark が
/// 見つかった時点で後続の revset を検索しない ("@" で見つかれば "@--" は触らない)。
///
/// - `@`: 標準 `git` ブランチ運用、または bookmark が現在のコミット上にある場合
/// - `@-`: `jj new` で空 `@` を作った直後 (PR #53 / #54 で実測)
/// - `@--`: 連続 `jj new` や中間空コミット運用向けのフォールバック
///
/// cli-merge-pipeline/src/main.rs と同じ設計 (PR #54 で確定)。
const BOOKMARK_SEARCH_REVSETS: &[&str] = &["@", "@-", "@--"];

/// PR 検出から除外する trunk 系 bookmark。
/// cli-push-runner/push_jj_bookmark.rs と同じリストを採用。
const TRUNK_BOOKMARKS: &[&str] = &["main", "master", "trunk", "develop"];

fn is_trunk_bookmark(name: &str) -> bool {
    TRUNK_BOOKMARKS.contains(&name)
}

/// 現在の jj change に紐づく全ブックマーク名を取得する。
///
/// `BOOKMARK_SEARCH_REVSETS` の順で検索し、最初に非空の結果が得られた revset の
/// bookmark を返す。すべての revset で空なら空 Vec。
pub(crate) fn get_jj_bookmarks() -> Vec<String> {
    select_from_revsets(BOOKMARK_SEARCH_REVSETS, query_bookmarks_at)
}

/// 指定 revset を優先順に試し、最初に非空の bookmark リストを得た revset の結果を返す。
/// テスト用に `query` をクロージャで注入できる。
fn select_from_revsets<F>(revsets: &[&str], query: F) -> Vec<String>
where
    F: Fn(&str) -> Vec<String>,
{
    for (i, revset) in revsets.iter().enumerate() {
        let bookmarks = query(revset);
        if !bookmarks.is_empty() {
            if i > 0 {
                log_info(&format!(
                    "revset '{}' で bookmark を検出: {:?}",
                    revset, bookmarks
                ));
            }
            return bookmarks;
        }
    }
    Vec::new()
}

/// 指定 revset の bookmark 名を `jj log` で取得する (I/O)。
///
/// stderr は意図的に抑止している (`Stdio::null`)。revset 不正や jj テンプレート
/// 非互換等の失敗時は空 Vec を返し、呼び出し側 (`select_from_revsets`) が次候補
/// revset へ fall through する動作。CI ログに jj の警告を大量に残さないためのトレードオフ。
///
/// 将来 jj テンプレート DSL の変更等で原因特定が必要になった場合は、一時的に
/// `.stderr(Stdio::inherit())` に差し替えるか、`!o.status.success()` 分岐で
/// `log_info` を出力するよう切り替えて調査する。
fn query_bookmarks_at(revset: &str) -> Vec<String> {
    let output = match Command::new("jj")
        .args([
            "log",
            "-r",
            revset,
            "--no-graph",
            "-T",
            "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    parse_bookmark_list_output(&String::from_utf8_lossy(&output.stdout))
}

/// `jj log` テンプレート出力 (カンマ区切り × 行) からユニークな bookmark 名を抽出する。
/// trunk 系 bookmark (master/main/trunk/develop) は PR 検索対象から除外する。
fn parse_bookmark_list_output(raw: &str) -> Vec<String> {
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

/// epoch seconds を ISO 8601 UTC 文字列に変換する (std のみ, chrono 不要)
pub(crate) fn epoch_secs_to_iso8601(epoch: u64) -> String {
    let secs_per_day: u64 = 86400;
    let day_count = (epoch / secs_per_day) as i64;
    let time_of_day = epoch % secs_per_day;

    let z = day_count + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, min, sec
    )
}

pub(crate) fn utc_now_iso8601() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_secs_to_iso8601(now.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_zero() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn epoch_known_date() {
        assert_eq!(epoch_secs_to_iso8601(1775044800), "2026-04-01T12:00:00Z");
    }

    #[test]
    fn epoch_leap_year() {
        assert_eq!(epoch_secs_to_iso8601(1709164800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn epoch_end_of_day() {
        assert_eq!(epoch_secs_to_iso8601(1775087999), "2026-04-01T23:59:59Z");
    }

    #[test]
    fn parse_pr_url_standard() {
        let output = "https://github.com/aloekun/claude-code-hook-test/pull/14";
        assert_eq!(parse_pr_number_from_url(output), Some(14));
    }

    #[test]
    fn parse_pr_url_with_prefix_lines() {
        let output = "some warning\nhttps://github.com/owner/repo/pull/42\n";
        assert_eq!(parse_pr_number_from_url(output), Some(42));
    }

    #[test]
    fn parse_pr_url_no_match() {
        let output = "no url here";
        assert_eq!(parse_pr_number_from_url(output), None);
    }

    #[test]
    fn parse_pr_url_empty() {
        assert_eq!(parse_pr_number_from_url(""), None);
    }

    // ─── bookmark 検出ロジック (cli-merge-pipeline と同仕様) ───

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
    fn is_trunk_bookmark_known_names_rejected() {
        assert!(is_trunk_bookmark("main"));
        assert!(is_trunk_bookmark("master"));
        assert!(is_trunk_bookmark("trunk"));
        assert!(is_trunk_bookmark("develop"));
        assert!(!is_trunk_bookmark("feat/x"));
        assert!(!is_trunk_bookmark("main-feature"));
    }

    #[test]
    fn select_from_revsets_returns_empty_when_all_revsets_empty() {
        let result = select_from_revsets(&["@", "@-"], |_| Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn select_from_revsets_prefers_current_over_parent() {
        let result = select_from_revsets(&["@", "@-"], |r| match r {
            "@" => vec!["feat/current".to_string()],
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        });
        assert_eq!(result, vec!["feat/current"]);
    }

    #[test]
    fn select_from_revsets_falls_back_to_parent_when_current_empty() {
        // create_pr.rs の --head 自動補完ケース: @ 空 / @- に feature bookmark
        let result = select_from_revsets(&["@", "@-"], |r| match r {
            "@" => Vec::new(),
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        });
        assert_eq!(result, vec!["feat/parent"]);
    }

    #[test]
    fn select_from_revsets_stops_at_first_hit() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::<String>::new());
        let result = select_from_revsets(&["@", "@-", "@--"], |r| {
            calls.borrow_mut().push(r.to_string());
            if r == "@-" {
                vec!["feat/hit".to_string()]
            } else {
                Vec::new()
            }
        });
        assert_eq!(result, vec!["feat/hit"]);
        assert_eq!(*calls.borrow(), vec!["@".to_string(), "@-".to_string()]);
    }
}
