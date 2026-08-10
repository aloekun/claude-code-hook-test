//! PR 検出 / owner-repo 検出 / リモートブランチ削除 (gh + jj 連携)。

use crate::pipeline::log_info;
use lib_jj_helpers::{get_jj_bookmarks_with_remote_fallback, BookmarkSearch, StderrMode};
use lib_subprocess::combine_output;
use serde::Deserialize;
use std::process::Command;

/// `gh pr view --json headRefName,isCrossRepository` のレスポンス
///
/// fork PR では `is_cross_repository == true` となり、upstream repo の
/// 同名ブランチを誤削除しないようにリモートブランチ削除をスキップする。
#[derive(Deserialize)]
pub(crate) struct PrHeadInfo {
    #[serde(rename = "headRefName")]
    pub(crate) head_ref_name: String,
    #[serde(rename = "isCrossRepository")]
    pub(crate) is_cross_repository: bool,
}

/// fork PR かどうかを判定し、リモートブランチ削除をスキップすべきか返す。
///
/// fork PR では `isCrossRepository == true` になるため、upstream repo の
/// 同名 ref への DELETE を防ぐ。
pub(crate) fn should_skip_branch_delete(info: &PrHeadInfo) -> bool {
    info.is_cross_repository
}

/// RFC 3986 の unreserved characters (`A-Z a-z 0-9 - _ . ~`) 以外を percent-encode する。
///
/// `gh api` の URL path segment に branch 名等を埋め込む際の安全弁。
/// `replace('/', "%2F")` だけでは `?` `#` `+` 等の特殊文字が素通りするため、
/// CodeRabbit PR #70 指摘 (Major) を受けて全特殊文字を encode する実装に置換した。
/// 実運用では git branch 命名規則によりほとんどの特殊文字は出現しないが、
/// defense-in-depth として汎用 helper を提供する。
pub(crate) fn percent_encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// gh コマンドを実行し、失敗時は stderr をログ出力する
pub(crate) fn run_gh_logged(args: &[&str]) -> Option<String> {
    let output = match Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            log_info(&format!("gh コマンド実行失敗: {} (args: {:?})", e, args));
            return None;
        }
    };

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            log_info(&format!("gh {:?} 失敗: {}", args, stderr));
        }
        None
    }
}

/// 現在のリポジトリの `{owner}/{repo}` を検出する (ADR-029)
pub(crate) fn detect_owner_repo() -> Option<String> {
    run_gh_logged(&[
        "repo",
        "view",
        "--json",
        "nameWithOwner",
        "-q",
        ".nameWithOwner",
    ])
}

/// PR 検出に失敗したときの診断情報 (実行可能な復旧手順を組み立てるために使う)。
pub(crate) struct PrLookupFailure {
    /// 失敗時点で見えていた bookmark。空でなければ「bookmark はあるが PR が無い」を意味する。
    pub(crate) search: BookmarkSearch,
}

/// bookmark 名から PR 番号を引く (open → 全 state の順)。
fn pr_number_for_bookmark(bookmark: &str) -> Option<u64> {
    log_info(&format!("jj bookmark '{}' を使用して PR を検索", bookmark));

    let open = run_gh_logged(&[
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
    if open.is_some() {
        return open;
    }

    run_gh_logged(&[
        "pr",
        "list",
        "--head",
        bookmark,
        "--state",
        "all",
        "--json",
        "number",
        "-q",
        ".[0].number",
    ])
    .and_then(|s| s.parse::<u64>().ok())
}

/// 現在のブックマークから PR 番号を検出する。
///
/// ローカル bookmark で見つからない場合はリモート追跡 bookmark へフォールバックする。
/// bot が remote に作った PR (ADR-072 の夜間ループ) は `claude/nightly-163@origin` の
/// ようなリモート専用 bookmark しか持たず、ローカル探索だけでは検出できない (順位 397)。
pub(crate) fn detect_pr_number() -> Result<u64, PrLookupFailure> {
    if let Some(pr_number) = run_gh_logged(&["pr", "view", "--json", "number", "-q", ".number"])
        .and_then(|s| s.parse::<u64>().ok())
    {
        return Ok(pr_number);
    }

    let search = get_jj_bookmarks_with_remote_fallback(StderrMode::Piped(log_info), Some(log_info));
    let bookmarks = match &search {
        BookmarkSearch::Local(names) => names.clone(),
        BookmarkSearch::RemoteOnly(names) => {
            log_info(&format!(
                "ローカル bookmark が無いため、リモート追跡 bookmark で PR を検索します: {:?} (jj bookmark track は不要)",
                names
            ));
            names.clone()
        }
        BookmarkSearch::NotFound => Vec::new(),
    };

    for bookmark in &bookmarks {
        if let Some(pr_number) = pr_number_for_bookmark(bookmark) {
            return Ok(pr_number);
        }
    }

    Err(PrLookupFailure { search })
}

pub(crate) fn delete_remote_branch(branch_name: &str) {
    let encoded_branch = percent_encode_path_segment(branch_name);
    let ref_path = format!("repos/{{owner}}/{{repo}}/git/refs/heads/{}", encoded_branch);
    let gh_output = Command::new("gh")
        .args(["api", &ref_path, "-X", "DELETE"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    let (del_ok, del_out) = match gh_output {
        Ok(o) => {
            let combined = combine_output(
                String::from_utf8_lossy(&o.stdout).trim(),
                String::from_utf8_lossy(&o.stderr).trim(),
            );
            (o.status.success(), combined)
        }
        Err(e) => (false, format!("gh コマンド実行失敗: {}", e)),
    };
    if del_ok {
        log_info(&format!(
            "リモートブランチ '{}' を削除しました",
            branch_name
        ));
    } else if del_out.contains("Reference does not exist") {
        log_info(&format!(
            "リモートブランチ '{}' は既に削除済みです（GitHub による自動削除）",
            branch_name
        ));
    } else {
        let msg = if del_out.is_empty() {
            "不明なエラー".to_string()
        } else {
            del_out
        };
        log_info(&format!(
            "リモートブランチ '{}' の削除失敗: {}",
            branch_name, msg
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_skip_branch_delete_true_for_fork_pr() {
        let info = PrHeadInfo {
            head_ref_name: "feature-branch".to_string(),
            is_cross_repository: true,
        };
        assert!(should_skip_branch_delete(&info));
    }

    #[test]
    fn should_skip_branch_delete_false_for_same_repo_pr() {
        let info = PrHeadInfo {
            head_ref_name: "feature-branch".to_string(),
            is_cross_repository: false,
        };
        assert!(!should_skip_branch_delete(&info));
    }

    #[test]
    fn percent_encode_passes_unreserved_chars() {
        assert_eq!(
            percent_encode_path_segment("abcXYZ-_0123.~"),
            "abcXYZ-_0123.~"
        );
    }

    #[test]
    fn percent_encode_slash_and_special_chars() {
        assert_eq!(percent_encode_path_segment("feat/foo"), "feat%2Ffoo");
        assert_eq!(percent_encode_path_segment("a?b#c"), "a%3Fb%23c");
        assert_eq!(percent_encode_path_segment("x+y&z=w"), "x%2By%26z%3Dw");
        assert_eq!(percent_encode_path_segment("has space"), "has%20space");
    }

    #[test]
    fn percent_encode_multibyte_utf8() {
        assert_eq!(percent_encode_path_segment("日"), "%E6%97%A5");
    }

    #[test]
    fn percent_encode_empty_string() {
        assert_eq!(percent_encode_path_segment(""), "");
    }

    #[test]
    fn skip_delete_when_cross_repository() {
        let info = PrHeadInfo {
            head_ref_name: "feat-x".into(),
            is_cross_repository: true,
        };
        assert!(should_skip_branch_delete(&info));
    }

    #[test]
    fn delete_allowed_when_same_repository() {
        let info = PrHeadInfo {
            head_ref_name: "feat-x".into(),
            is_cross_repository: false,
        };
        assert!(!should_skip_branch_delete(&info));
    }
}
