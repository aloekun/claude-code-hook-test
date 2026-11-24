//! PR 検出 / owner-repo 検出 / リモートブランチ削除 (gh + jj 連携)。

use crate::pipeline::log_info;
use lib_jj_helpers::{get_jj_bookmarks_with_remote_fallback, BookmarkSearch, StderrMode};
use lib_subprocess::{combine_output, CmdCapture};
use serde::Deserialize;

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

/// `gh` 呼び出しの timeout。`--paginate` で複数ページを取る呼び出しも同じ値で賄うため、
/// 1 リクエスト分より余裕を持たせる。
pub(crate) const GH_CMD_TIMEOUT_SECS: u64 = 120;

/// gh を timeout 付きで実行する。**本 crate の gh 呼び出しはすべてここを通す** (順位 238)。
///
/// 旧実装は gh を timeout なしの `.output()` で呼んでおり、ネットワーク停止や
/// gh 側の停止で merge pipeline が無期限にハングし得た (PR #230 CodeRabbit Major、ADR-016)。
pub(crate) fn run_gh(args: &[&str]) -> CmdCapture {
    lib_subprocess::run_cmd_direct_capture("gh", args, GH_CMD_TIMEOUT_SECS)
}

/// 失敗した gh 呼び出しを 1 行の診断にする (I/O なし)。
///
/// timeout は stderr が空でも原因が分かるよう明記する。起動失敗は
/// `run_cmd_direct_capture` が stderr に `"Failed to execute ..."` を入れている。
pub(crate) fn gh_failure_detail(cap: &CmdCapture) -> String {
    let stderr = cap.stderr.trim();
    match (cap.timed_out, stderr.is_empty()) {
        (true, true) => format!("timeout after {}s", GH_CMD_TIMEOUT_SECS),
        (true, false) => format!("timeout after {}s: {}", GH_CMD_TIMEOUT_SECS, stderr),
        (false, _) => stderr.to_string(),
    }
}

/// gh コマンドを実行し、失敗時は stderr をログ出力する
pub(crate) fn run_gh_logged(args: &[&str]) -> Option<String> {
    let cap = run_gh(args);
    if !cap.ok {
        let detail = gh_failure_detail(&cap);
        if !detail.is_empty() {
            log_info(&format!("gh {:?} 失敗: {}", args, detail));
        }
        return None;
    }
    let s = cap.stdout.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
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
fn pr_number_for_bookmark<G>(bookmark: &str, gh: &mut G) -> Option<u64>
where
    G: FnMut(&[&str]) -> Option<String>,
{
    log_info(&format!("jj bookmark '{}' を使用して PR を検索", bookmark));

    let open = gh(&["pr", "list", "--head", bookmark, "--json", PR_LIST_FIELDS])
        .and_then(|json| first_same_repo_pr(&json));
    if open.is_some() {
        return open;
    }

    gh(&["pr", "list", "--head", bookmark, "--state", "all", "--json", PR_LIST_FIELDS])
        .and_then(|json| first_same_repo_pr(&json))
}

/// `gh pr list` に取らせる項目。fork の判定に `isCrossRepository` を使う。
const PR_LIST_FIELDS: &str = "number,isCrossRepository";

/// `gh pr list --json number,isCrossRepository` の出力から、fork でない最初の PR 番号を取る (I/O なし)。
///
/// `--head` は branch 名だけで絞り、head のリポジトリを見ない。他人の fork に同名の branch で
/// PR が作られると、それが先頭に来てマージ対象になりうる (PR #665 の CodeRabbit 指摘)。
/// `isCrossRepository` が `false` と読めた PR だけを採る。項目が欠けた PR は採らない (ADR-043)。
fn first_same_repo_pr(json: &str) -> Option<u64> {
    let prs: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    prs.iter()
        .find(|pr| pr["isCrossRepository"].as_bool() == Some(false))
        .and_then(|pr| pr["number"].as_u64())
}

/// 現在のブックマークから PR 番号を検出する。
///
/// ローカル bookmark で見つからない場合はリモート追跡 bookmark へフォールバックする。
/// bot が remote に作った PR (ADR-072 の夜間ループ) は `claude/nightly-163@origin` の
/// ようなリモート専用 bookmark しか持たず、ローカル探索だけでは検出できない (順位 397)。
///
/// **引数なしの `gh pr view` は使わない** (順位 626)。`gh pr view` は共有 store の git HEAD
/// から PR を決めるが、並列 jj workspace (ADR-045) では git HEAD が全 workspace で 1 つなので、
/// 別 workspace で checkout した branch の PR が返り、それを黙ってマージしうる。jj の colocated
/// リポジトリでは git HEAD は通常 detached なので、`gh pr view` が成功するのは誰かが
/// branch を checkout したときだけであり、正しく働く場面が無い。bookmark が無い・PR が
/// 無いときは git HEAD へ落ちずに失敗する。
///
/// jj を使わない git だけのリポジトリは対象外とする。本 crate はマージ後の同期を
/// `jj git fetch` / `jj new` で行うので、そもそも jj 前提であり、`gh pr view` の経路を
/// git だけの環境のために残す理由が無い。
pub(crate) fn detect_pr_number() -> Result<u64, PrLookupFailure> {
    detect_pr_number_with(
        || get_jj_bookmarks_with_remote_fallback(StderrMode::Piped(log_info), Some(log_info)),
        run_gh_logged,
    )
}

/// [`detect_pr_number`] の本体。bookmark の探索と gh の呼び出しを注入できる。
fn detect_pr_number_with<S, G>(search_bookmarks: S, mut gh: G) -> Result<u64, PrLookupFailure>
where
    S: FnOnce() -> BookmarkSearch,
    G: FnMut(&[&str]) -> Option<String>,
{
    let search = search_bookmarks();
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
        if let Some(pr_number) = pr_number_for_bookmark(bookmark, &mut gh) {
            return Ok(pr_number);
        }
    }

    Err(PrLookupFailure { search })
}

pub(crate) fn delete_remote_branch(branch_name: &str) {
    let encoded_branch = percent_encode_path_segment(branch_name);
    let ref_path = format!("repos/{{owner}}/{{repo}}/git/refs/heads/{}", encoded_branch);
    let cap = run_gh(&["api", &ref_path, "-X", "DELETE"]);
    let del_ok = cap.ok;
    let del_out = if cap.timed_out {
        gh_failure_detail(&cap)
    } else {
        combine_output(cap.stdout.trim(), cap.stderr.trim())
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

    fn failed_capture(stderr: &str, timed_out: bool) -> CmdCapture {
        CmdCapture {
            ok: false,
            stdout: String::new(),
            stderr: stderr.to_string(),
            timed_out,
        }
    }

    /// timeout は stderr が空でも原因が読めること (順位 238)。
    #[test]
    fn gh_failure_detail_names_the_timeout_even_without_stderr() {
        let detail = gh_failure_detail(&failed_capture("", true));
        assert_eq!(detail, format!("timeout after {}s", GH_CMD_TIMEOUT_SECS));
    }

    #[test]
    fn gh_failure_detail_keeps_stderr_alongside_the_timeout() {
        let detail = gh_failure_detail(&failed_capture("partial\n", true));
        assert_eq!(
            detail,
            format!("timeout after {}s: partial", GH_CMD_TIMEOUT_SECS)
        );
    }

    /// gh の偽物。`gh pr view` (引数なし) は git HEAD の branch の PR を返し、
    /// `gh pr list --head <name>` は `heads` に登録した PR を同じリポジトリの PR として
    /// JSON で返す。`raw_heads` は JSON をそのまま返す (fork を混ぜるとき用)。呼ばれた引数を記録する。
    struct FakeGh {
        git_head_pr: Option<&'static str>,
        heads: Vec<(&'static str, &'static str)>,
        raw_heads: Vec<(&'static str, &'static str)>,
        calls: Vec<Vec<String>>,
    }

    impl FakeGh {
        fn new(git_head_pr: Option<&'static str>, heads: &[(&'static str, &'static str)]) -> Self {
            Self {
                git_head_pr,
                heads: heads.to_vec(),
                raw_heads: Vec::new(),
                calls: Vec::new(),
            }
        }

        fn with_raw(raw_heads: &[(&'static str, &'static str)]) -> Self {
            Self {
                raw_heads: raw_heads.to_vec(),
                ..Self::new(None, &[])
            }
        }

        fn call(&mut self, args: &[&str]) -> Option<String> {
            self.calls.push(args.iter().map(|s| s.to_string()).collect());
            match args {
                ["pr", "view", ..] => self.git_head_pr.map(str::to_string),
                ["pr", "list", "--head", head, ..] => {
                    if let Some((_, raw)) = self.raw_heads.iter().find(|(h, _)| h == head) {
                        return Some(raw.to_string());
                    }
                    self.heads
                        .iter()
                        .find(|(h, _)| h == head)
                        .map(|(_, n)| format!(r#"[{{"number":{n},"isCrossRepository":false}}]"#))
                }
                _ => None,
            }
        }

        fn called_pr_view(&self) -> bool {
            self.calls.iter().any(|c| c.starts_with(&["pr".into(), "view".into()]))
        }
    }

    /// git HEAD が別 workspace の feature branch (PR #999) を指していても、現在の jj
    /// bookmark の PR を選ぶ (順位 626)。git HEAD は全 workspace で 1 つなので、
    /// `gh pr view` を先に見ると別 workspace の PR を黙ってマージしうる。
    #[test]
    fn detect_pr_number_prefers_the_jj_bookmark_over_the_git_head_branch() {
        let mut gh = FakeGh::new(Some("999"), &[("feat-mine", "42")]);
        let got = detect_pr_number_with(
            || BookmarkSearch::Local(vec!["feat-mine".into()]),
            |args: &[&str]| gh.call(args),
        );
        assert_eq!(got.ok(), Some(42));
        assert!(!gh.called_pr_view(), "git HEAD 由来の gh pr view を呼んだ: {:?}", gh.calls);
    }

    /// bookmark はあるが PR が無いとき、git HEAD の PR へ落ちずに失敗する。
    /// 落ちると、bookmark を作っただけの workspace から別の PR をマージしうる。
    #[test]
    fn detect_pr_number_fails_without_falling_back_to_the_git_head_branch() {
        let mut gh = FakeGh::new(Some("999"), &[]);
        let got = detect_pr_number_with(
            || BookmarkSearch::Local(vec!["feat-no-pr".into()]),
            |args: &[&str]| gh.call(args),
        );
        let failure = got.expect_err("PR は見つからないはず");
        assert!(matches!(failure.search, BookmarkSearch::Local(ref n) if n == &["feat-no-pr".to_string()]));
        assert!(!gh.called_pr_view(), "git HEAD 由来の gh pr view を呼んだ: {:?}", gh.calls);
    }

    /// bookmark が 1 つも無いときも、git HEAD の PR を採用しない。
    #[test]
    fn detect_pr_number_fails_when_no_bookmark_even_if_git_head_has_a_pr() {
        let mut gh = FakeGh::new(Some("999"), &[]);
        let got = detect_pr_number_with(|| BookmarkSearch::NotFound, |args: &[&str]| gh.call(args));
        assert!(matches!(got.err().map(|f| f.search), Some(BookmarkSearch::NotFound)));
        assert!(!gh.called_pr_view(), "git HEAD 由来の gh pr view を呼んだ: {:?}", gh.calls);
    }

    /// リモート専用 bookmark でも PR を引ける (順位 397 の経路を保つ)。
    #[test]
    fn detect_pr_number_uses_remote_only_bookmarks() {
        let mut gh = FakeGh::new(None, &[("claude/nightly-163", "163")]);
        let got = detect_pr_number_with(
            || BookmarkSearch::RemoteOnly(vec!["claude/nightly-163".into()]),
            |args: &[&str]| gh.call(args),
        );
        assert_eq!(got.ok(), Some(163));
    }

    /// 他人の fork に同名 branch の新しい PR (#999) があっても、同じリポジトリの PR (#42) を選ぶ
    /// (PR #665 の CodeRabbit 指摘)。`gh pr list` は新しい順なので fork の PR が先頭に来る。
    #[test]
    fn detect_pr_number_skips_a_newer_fork_pr_with_the_same_branch_name() {
        let mut gh = FakeGh::with_raw(&[(
            "feat-x",
            r#"[{"number":999,"isCrossRepository":true},{"number":42,"isCrossRepository":false}]"#,
        )]);
        let got = detect_pr_number_with(
            || BookmarkSearch::Local(vec!["feat-x".into()]),
            |args: &[&str]| gh.call(args),
        );
        assert_eq!(got.ok(), Some(42));
    }

    /// 同名 branch の PR が fork にしか無いときは、それを採らずに失敗する。
    #[test]
    fn detect_pr_number_fails_when_only_a_fork_pr_has_the_branch_name() {
        let mut gh = FakeGh::with_raw(&[("feat-x", r#"[{"number":999,"isCrossRepository":true}]"#)]);
        let got = detect_pr_number_with(
            || BookmarkSearch::Local(vec!["feat-x".into()]),
            |args: &[&str]| gh.call(args),
        );
        assert!(got.is_err(), "fork の PR を採った: {:?}", got.ok());
    }

    #[test]
    fn first_same_repo_pr_takes_the_first_pr_that_is_not_from_a_fork() {
        let json = r#"[{"number":9,"isCrossRepository":true},{"number":7,"isCrossRepository":false},{"number":5,"isCrossRepository":false}]"#;
        assert_eq!(first_same_repo_pr(json), Some(7));
        assert_eq!(first_same_repo_pr("[]"), None);
    }

    /// fork かどうかを読めない PR は採らない (ADR-043)。
    #[test]
    fn first_same_repo_pr_rejects_prs_whose_fork_flag_is_unreadable() {
        assert_eq!(first_same_repo_pr(r#"[{"number":7}]"#), None);
        assert_eq!(first_same_repo_pr(r#"[{"number":7,"isCrossRepository":"false"}]"#), None);
        assert_eq!(first_same_repo_pr("not json"), None);
    }

    #[test]
    fn gh_failure_detail_is_the_trimmed_stderr_for_ordinary_failures() {
        let detail = gh_failure_detail(&failed_capture("  HTTP 404: Not Found\n", false));
        assert_eq!(detail, "HTTP 404: Not Found");
    }
}
