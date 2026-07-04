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

    let Some(pr) = read_state_from(&state_file_path()).and_then(|s| s.pr) else {
        log_info(
            "[review_trigger] PR 番号が state から解決できず @coderabbitai review をスキップ (必要なら手動投稿してください)",
        );
        return;
    };
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
}
