use lib_jj_helpers::{get_jj_bookmarks as lib_get_jj_bookmarks, StderrMode};
pub(crate) use lib_pending_file::utc_now_iso8601;

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

/// 現在の jj change に紐づく全ブックマーク名を取得する。
///
/// `lib_jj_helpers::BOOKMARK_SEARCH_REVSETS` の順で検索し、最初に非空の結果が
/// 得られた revset の bookmark を返す。trunk 系 bookmark は除外される。
///
/// stderr は `Silent` (CI ログ汚染回避)。fallback revset で hit した場合のみ
/// `log_info` に通知。
pub(crate) fn get_jj_bookmarks() -> Vec<String> {
    lib_get_jj_bookmarks(StderrMode::Silent, Some(log_info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib_pending_file::epoch_secs_to_iso8601;

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

    // ─── bookmark 検出ロジック (lib-jj-helpers に集約済) ───
    //
    // TRUNK_BOOKMARKS / BOOKMARK_SEARCH_REVSETS / parse_bookmark_list_output /
    // select_from_revsets / query_bookmarks_at / get_jj_bookmarks の unit test は
    // lib-jj-helpers/src/lib.rs#tests に集約 (ADR-024 本採用、PR-C で移設)。
    // cli-pr-monitor 側からは lib_jj_helpers の公開 API 経由でのみ使用する。
}
