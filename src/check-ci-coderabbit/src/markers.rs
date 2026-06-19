//! CodeRabbit walkthrough / rate-limit marker 定数 + 判定 helper。

use crate::models::GhComment;

/// CodeRabbit rate-limit body markers。complete list of known formats.
///
/// CR は format を時間経過で変更するため multi-variant 配列で対応する
/// (PR #182/#184 で silent regression を実体観測)。詳細は ADR-034 § CR rate-limit
/// format evolution 参照。
pub(crate) const RATE_LIMIT_MARKERS: &[&str] =
    &["Rate limit exceeded", "rate limited by coderabbit.ai"];

/// 順位 208: CR walkthrough comment が clean 判定を示すときに body に含まれる marker。
pub(crate) const WALKTHROUGH_CLEAN_MARKER: &str =
    "No actionable comments were generated in the recent review.";

/// 順位 208: CR walkthrough comment body の auto-generated header marker。
pub(crate) const WALKTHROUGH_HEADER_MARKER: &str =
    "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->";

/// `c.body` に [`RATE_LIMIT_MARKERS`] のいずれかが含まれていれば rate-limit comment と判定する。
pub(crate) fn is_rate_limit_comment(c: &GhComment) -> bool {
    c.body
        .as_deref()
        .map(|b| RATE_LIMIT_MARKERS.iter().any(|m| b.contains(m)))
        .unwrap_or(false)
}

/// rate-limit comment の reset 計算に使うタイムスタンプを返す。
///
/// `updated_at` (CR が wait 時間を更新した編集時刻) を優先し、未設定なら `created_at`。
/// `created_at` のみで計算すると premature retrigger を引き起こす
/// (PR #97 round 3 Finding 1、2026-04-30 実観測)。
pub(crate) fn rate_limit_event_time(c: &GhComment) -> Option<&str> {
    c.updated_at.as_deref().or(c.created_at.as_deref())
}

/// 順位 208: 単一 comment が CR walkthrough の clean marker を持つか判定する pure helper。
pub(crate) fn is_clean_walkthrough_comment(c: &GhComment, push_time: &str) -> bool {
    let is_coderabbit = c
        .user
        .as_ref()
        .and_then(|u| u.login.as_deref())
        .map(|l| l == "coderabbitai[bot]")
        .unwrap_or(false);
    if !is_coderabbit {
        return false;
    }
    let after_push_time = rate_limit_event_time(c)
        .map(|t| t >= push_time)
        .unwrap_or(false);
    if !after_push_time {
        return false;
    }
    if is_rate_limit_comment(c) {
        return false;
    }
    let body = c.body.as_deref().unwrap_or("");
    body.contains(WALKTHROUGH_HEADER_MARKER) && body.contains(WALKTHROUGH_CLEAN_MARKER)
}
