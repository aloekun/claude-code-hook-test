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

/// 現 HEAD が既に CodeRabbit にレビュー済みか判定する (WP-05 follow-up、再トリガー抑止)。
///
/// `Some(true)` = レビュー済み / `Some(false)` = 未レビュー / `None` = 判定不能。
/// CodeRabbit の PR review は submit された commit を `commit_id` に持つため、現 HEAD が
/// いずれかの CodeRabbit review の `commit_id` と一致すれば「その HEAD はレビュー済み」。
/// gh 照会失敗時は `None` を返し、呼び出し側は fail-open (投稿) にする。確証がある
/// `Some(true)` のときだけ skip し、再レビュー欠落を招かない設計。
fn head_already_reviewed(pr: u64, repo: &str) -> Option<bool> {
    let pr_str = pr.to_string();
    let head = crate::runner::run_gh_quiet(&[
        "pr", "view", &pr_str, "--json", "headRefOid", "--jq", ".headRefOid",
    ])?;
    let head = head.trim();
    if head.is_empty() {
        return None;
    }
    let reviewed = crate::runner::run_gh_quiet(&[
        "api",
        "--paginate",
        &format!("repos/{}/pulls/{}/reviews", repo, pr),
        "--jq",
        r#".[] | select(.user.login=="coderabbitai[bot]") | .commit_id"#,
    ])?;
    Some(is_head_in_reviewed(head, &reviewed))
}

/// CodeRabbit がレビューした commit SHA 一覧 (改行区切り) に現 HEAD が含まれるかの純粋判定。
fn is_head_in_reviewed(head: &str, reviewed_commit_ids: &str) -> bool {
    reviewed_commit_ids
        .lines()
        .any(|line| line.trim() == head)
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
}
