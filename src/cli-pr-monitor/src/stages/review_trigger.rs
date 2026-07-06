//! auto-push 後の CodeRabbit 再レビュー明示トリガー (WP-03 / ADR-019 amendment)。
//!
//! `.coderabbit.yaml` の `reviews.auto_review.auto_incremental_review = false` と結合する。
//! 増分レビューを抑止すると fix push だけでは CodeRabbit が再レビューしないため、
//! 監視側 (`repush.rs` の auto-push 成功後) から `@coderabbitai review` を明示投稿して
//! 再レビューを 1 回だけ発火する。レート消費を「fix 1 束ねあたり 1 レビュー」に抑える。

use crate::log::log_info;

/// auto-push 後に `@coderabbitai review` を明示投稿すべきかの純粋判定。
///
/// push が成功し、かつ `trigger_review_after_push` が有効なときのみ true。
/// push 失敗時はレビュー対象の変更が remote に反映されていないため投稿しない。
/// flag が false のときは `.coderabbit.yaml` の auto_incremental_review が有効な前提で、
/// 明示投稿すると二重レビューになるため投稿しない。
pub(crate) fn should_trigger_review_after_push(push_ok: bool, flag: bool) -> bool {
    push_ok && flag
}

/// `@coderabbitai review` を投稿して CodeRabbit の再レビューを明示発火する。
///
/// PR 番号は state から解決する。state 不在 / PR 番号未確定 / gh 投稿失敗は
/// いずれも log を残して続行する (再レビュー起動は助言層 = fail-open。
/// ADR-043 の fail-closed はゲート層にのみ適用され、本経路は該当しない)。
pub(crate) fn trigger_coderabbit_review() {
    use crate::state::{read_state_from, state_file_path};

    let Some(state) = read_state_from(&state_file_path()) else {
        log_info(
            "[review_trigger] state を読み込めず @coderabbitai review をスキップ (必要なら手動投稿してください)",
        );
        return;
    };
    let Some(pr) = state.pr else {
        log_info(
            "[review_trigger] PR 番号が state から解決できず @coderabbitai review をスキップ (必要なら手動投稿してください)",
        );
        return;
    };

    if let Some(repo) = state.repo.as_deref() {
        if head_already_reviewed(pr, repo) == Some(true) {
            log_info(&format!(
                "[review_trigger] PR #{} の現 HEAD は既に CodeRabbit レビュー済みのため @coderabbitai review をスキップ (再トリガー抑止、レート消費回避)",
                pr
            ));
            return;
        }
    }

    let pr_str = pr.to_string();
    if crate::runner::run_gh_quiet(&["pr", "comment", &pr_str, "--body", "@coderabbitai review"])
        .is_none()
    {
        log_info(&format!(
            "[review_trigger] @coderabbitai review 投稿失敗 (PR #{})。必要なら手動投稿してください",
            pr
        ));
        return;
    }
    log_info(&format!(
        "[review_trigger] @coderabbitai review を投稿 (PR #{}, auto_incremental_review=false 経路)",
        pr
    ));
}

/// 現 HEAD が既に CodeRabbit にレビュー済みか判定する (WP-05 follow-up / 順位258、再トリガー抑止)。
///
/// `Some(true)` = レビュー済み (skip) / `Some(false)` = 未レビュー確定 (投稿) /
/// `None` = 判定不能 (fail-open で投稿)。
///
/// 判定ソースは 2 系統 (順位258 で commit status を追加):
/// 1. **reviews API**: CodeRabbit の PR review は submit した commit を `commit_id` に持つため、
///    現 HEAD がいずれかの CodeRabbit review の `commit_id` と一致すればレビュー済み。
/// 2. **commit status**: CodeRabbit は「指摘ゼロ」で完了した (再) レビューでは formal review
///    object を提出せず、commit status (context `CodeRabbit` / state `success`) のみで完了を
///    通知する (2026-07-05 実測)。reviews API 単独では指摘ゼロ完了を検知できず (順位258 の動機)、
///    HEAD の combined status に CodeRabbit success があればレビュー済みとみなす。
///
/// 2 系統は [`combine_reviewed`] で fail-open 合成する: いずれかが確証 (`Some(true)`) なら skip、
/// 両方 `None` (判定不能) なら fail-open で投稿。gh 照会失敗・JSON parse 不能はいずれも `None` に
/// 倒し、確証がある `Some(true)` のときだけ skip して再レビュー欠落を招かない設計。
fn head_already_reviewed(pr: u64, repo: &str) -> Option<bool> {
    let pr_str = pr.to_string();
    let head = crate::runner::run_gh_quiet(&[
        "pr", "view", &pr_str, "--json", "headRefOid", "--jq", ".headRefOid",
    ])?;
    let head = head.trim();
    if head.is_empty() {
        return None;
    }
    let via_reviews = reviewed_via_reviews_api(pr, repo, head);
    let via_status = reviewed_via_commit_status(repo, head);
    combine_reviewed(via_reviews, via_status)
}

/// reviews API 経由の「HEAD レビュー済み」判定。gh 照会失敗は `None` (判定不能)。
fn reviewed_via_reviews_api(pr: u64, repo: &str, head: &str) -> Option<bool> {
    let reviewed = crate::runner::run_gh_quiet(&[
        "api",
        "--paginate",
        &format!("repos/{}/pulls/{}/reviews", repo, pr),
        "--jq",
        r#".[] | select(.user.login=="coderabbitai[bot]") | .commit_id"#,
    ])?;
    Some(is_head_in_reviewed(head, &reviewed))
}

/// commit status 経由の「HEAD レビュー済み」判定。gh 照会失敗・JSON parse 不能は `None`。
fn reviewed_via_commit_status(repo: &str, head: &str) -> Option<bool> {
    let status_json =
        crate::runner::run_gh_quiet(&["api", &format!("repos/{}/commits/{}/status", repo, head)])?;
    parse_commit_status_reviewed(&status_json)
}

/// CodeRabbit がレビューした commit SHA 一覧 (改行区切り) に現 HEAD が含まれるかの純粋判定。
fn is_head_in_reviewed(head: &str, reviewed_commit_ids: &str) -> bool {
    reviewed_commit_ids
        .lines()
        .any(|line| line.trim() == head)
}

/// GitHub combined status API (`repos/{repo}/commits/{sha}/status`) の JSON から
/// CodeRabbit がその commit をレビュー完了済みか判定する純粋関数。
///
/// `statuses[]` に context が `CodeRabbit` (ASCII 大文字小文字無視) かつ state が `success` の
/// エントリがあれば `Some(true)` (実測 description は `Review completed`)。エントリはあるが
/// 該当なしは `Some(false)`。JSON parse 不能 / `statuses` 欠落は `None` (fail-open)。
fn parse_commit_status_reviewed(status_json: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(status_json).ok()?;
    let statuses = value.get("statuses")?.as_array()?;
    Some(statuses.iter().any(|entry| {
        let context = entry
            .get("context")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let state = entry
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        context.eq_ignore_ascii_case("CodeRabbit") && state == "success"
    }))
}

/// reviews API と commit status の 2 系統の「レビュー済み」判定を fail-open で合成する純粋関数。
///
/// - いずれかが `Some(true)` (確証) → `Some(true)` (skip)。
/// - 両方 `None` (判定不能) → `None` (fail-open で投稿)。
/// - それ以外 (true なし、少なくとも一方が `Some(false)`) → `Some(false)` (未レビュー確定 → 投稿)。
///
/// caller は `== Some(true)` のときだけ skip するため `Some(false)` と `None` は同じ「投稿」に
/// 落ちるが、判定不能 (`None`) と未レビュー確定 (`Some(false)`) を型で区別し、ログ・テストで
/// fail-open 経路 (`None` → 投稿) を反転検査できるようにする (順位258 / 順位162 と同型)。
fn combine_reviewed(via_reviews: Option<bool>, via_status: Option<bool>) -> Option<bool> {
    match (via_reviews, via_status) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (None, None) => None,
        _ => Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_trigger_review_only_when_push_ok_and_flag_enabled() {
        assert!(should_trigger_review_after_push(true, true));
        assert!(
            !should_trigger_review_after_push(false, true),
            "push 失敗時は変更が remote に無いので再レビューを発火しない"
        );
        assert!(
            !should_trigger_review_after_push(true, false),
            "flag off なら auto_incremental_review 有効 (二重レビュー防止)"
        );
        assert!(!should_trigger_review_after_push(false, false));
    }

    #[test]
    fn is_head_in_reviewed_detects_already_reviewed_head() {
        let reviewed = "abc1230000\ndef4560000\n789aaa0000";
        assert!(
            is_head_in_reviewed("def4560000", reviewed),
            "現 HEAD が CodeRabbit review 済み SHA 集合に含まれる → 再トリガー抑止"
        );
        assert!(
            is_head_in_reviewed("789aaa0000", "  789aaa0000  \nabc1230000"),
            "前後空白を trim して一致判定する"
        );
        assert!(
            !is_head_in_reviewed("999zzz0000", reviewed),
            "未レビューの新 HEAD は含まれない → 投稿する"
        );
        assert!(
            !is_head_in_reviewed("abc1230000", ""),
            "CodeRabbit review が無ければ false (投稿する)"
        );
    }

    #[test]
    fn parse_commit_status_detects_coderabbit_success() {
        let json = r#"{
            "state": "success",
            "statuses": [
                {"context": "ci/build", "state": "success"},
                {"context": "CodeRabbit", "state": "success", "description": "Review completed"}
            ]
        }"#;
        assert_eq!(
            parse_commit_status_reviewed(json),
            Some(true),
            "commit status に CodeRabbit success があれば指摘ゼロ完了でもレビュー済み"
        );
    }

    #[test]
    fn parse_commit_status_is_case_insensitive_for_context() {
        let json = r#"{"statuses": [{"context": "coderabbit", "state": "success"}]}"#;
        assert_eq!(
            parse_commit_status_reviewed(json),
            Some(true),
            "context は ASCII 大文字小文字を無視して一致"
        );
    }

    #[test]
    fn parse_commit_status_not_reviewed_when_no_coderabbit_success() {
        let pending = r#"{"statuses": [{"context": "CodeRabbit", "state": "pending"}]}"#;
        assert_eq!(
            parse_commit_status_reviewed(pending),
            Some(false),
            "CodeRabbit があっても state が success でなければ未レビュー確定"
        );
        let other = r#"{"statuses": [{"context": "ci/build", "state": "success"}]}"#;
        assert_eq!(
            parse_commit_status_reviewed(other),
            Some(false),
            "CodeRabbit context が無ければ未レビュー確定"
        );
        let empty = r#"{"statuses": []}"#;
        assert_eq!(
            parse_commit_status_reviewed(empty),
            Some(false),
            "statuses が空なら未レビュー確定"
        );
    }

    #[test]
    fn parse_commit_status_none_on_unparseable_or_missing_statuses() {
        assert_eq!(
            parse_commit_status_reviewed("not json"),
            None,
            "JSON parse 不能は None (fail-open で投稿)"
        );
        assert_eq!(
            parse_commit_status_reviewed(r#"{"state": "success"}"#),
            None,
            "statuses key 欠落は None (fail-open で投稿)"
        );
    }

    #[test]
    fn combine_reviewed_skips_when_either_source_confirms() {
        assert_eq!(
            combine_reviewed(Some(true), Some(false)),
            Some(true),
            "reviews API が確証 → skip"
        );
        assert_eq!(
            combine_reviewed(None, Some(true)),
            Some(true),
            "commit status が確証 (reviews API 判定不能でも) → skip"
        );
        assert_eq!(
            combine_reviewed(Some(true), None),
            Some(true),
            "reviews API 確証 (commit status 判定不能でも) → skip"
        );
    }

    /// fail-open 反転テスト (順位258 / 順位162 と同型): 両ソース判定不能 → None → 投稿。
    /// caller は `Some(true)` のときだけ skip するため、`None` が誤って `Some(true)` に反転すると
    /// 再レビューが恒常的に欠落する。この経路を明示的に固定する。
    #[test]
    fn combine_reviewed_fails_open_to_none_when_both_indeterminate() {
        assert_eq!(
            combine_reviewed(None, None),
            None,
            "両ソース判定不能 (gh 照会失敗 / parse 不能) は None を返し fail-open で投稿する"
        );
    }

    #[test]
    fn combine_reviewed_posts_when_confirmed_not_reviewed() {
        assert_eq!(
            combine_reviewed(Some(false), Some(false)),
            Some(false),
            "両ソースが未レビュー確定 → Some(false) で投稿"
        );
        assert_eq!(
            combine_reviewed(Some(false), None),
            Some(false),
            "一方が未レビュー確定・他方判定不能 → Some(false) で投稿"
        );
        assert_eq!(
            combine_reviewed(None, Some(false)),
            Some(false),
            "一方判定不能・他方が未レビュー確定 → Some(false) で投稿"
        );
    }
}
