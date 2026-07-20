//! CodeRabbit rate-limit parsing + ISO 8601 timestamp arithmetic。

use crate::markers::{is_rate_limit_comment, rate_limit_event_time};
use crate::models::{GhComment, RateLimitInfo};

/// marker は一致したが待機時間を既知書式で読めなかったときに使う既定待機時間 (分)。
///
/// CR は rate-limit comment の書式を過去 2 回変更しており (ADR-034)、書式追随は
/// 常に後追いになる。待機時間が読めないことを「rate-limit ではない」と扱うと
/// silent success に戻るため、marker 一致を制限の根拠として採用し、待機時間だけを
/// 保守的な既定値で埋める。値が実際の reset より短ければ wakeup 後に再検出されて
/// 再度 park されるだけ (retry は `max_retries` で有界) なので安全側に倒れる。
pub(crate) const UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES: u64 = 30;

/// CodeRabbit rate-limit comment を検出し、reset 時刻 (unix epoch) を返す。
pub(crate) fn parse_rate_limit(json: &str, push_time: &str) -> Option<RateLimitInfo> {
    let comments: Vec<GhComment> = serde_json::from_str(json).ok()?;

    let mut candidates: Vec<&GhComment> = comments
        .iter()
        .filter(|c| {
            let is_coderabbit = c
                .user
                .as_ref()
                .and_then(|u| u.login.as_deref())
                .map(|l| l == "coderabbitai[bot]")
                .unwrap_or(false);
            let has_rate_limit = is_rate_limit_comment(c);
            let after_push_time = rate_limit_event_time(c)
                .map(|t| t >= push_time)
                .unwrap_or(false);
            is_coderabbit && has_rate_limit && after_push_time
        })
        .collect();
    candidates.sort_by(|a, b| {
        rate_limit_event_time(b)
            .unwrap_or("")
            .cmp(rate_limit_event_time(a).unwrap_or(""))
    });
    let latest = candidates.first()?;

    let body = latest.body.as_deref()?;
    let event_time = rate_limit_event_time(latest)?;
    let (minutes, seconds, wait_time_parsed) = resolve_wait_time(body);
    if !wait_time_parsed {
        warn_unknown_wait_time_format();
    }
    let comment_unix = parse_iso8601_to_unix(event_time)?;

    let until_unix_secs = comment_unix + (minutes as i64) * 60 + (seconds as i64) + 60;

    Some(RateLimitInfo {
        until_unix_secs,
        comment_event_time: event_time.to_string(),
        wait_minutes: minutes,
        wait_seconds: seconds,
        wait_time_parsed,
    })
}

/// 未知書式で既定値を適用したことを stderr に警告する。
///
/// cli-pr-monitor は checker の stderr をログ転送するため、既定値で埋めた事実が
/// 運用ログに残る (park signal の「30 分待機」を CR の申告値と誤読させないため)。
/// 同時に「CR が書式を再変更した」検知シグナルとしても機能する。
fn warn_unknown_wait_time_format() {
    eprintln!(
        "[check-ci-coderabbit] rate-limit marker は一致したが待機時間を既知書式で読めず、既定 {} 分を適用 (CR 書式が再変更された可能性)",
        UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES
    );
}

/// marker 一致済み body から待機時間を解決する。
///
/// 戻り値の 3 番目は「既知書式で待機時間を読めたか」。既知 3 世代のいずれにも
/// 一致しなければ [`UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES`] を返し `false` を立てる。
/// 呼び出し側はこのフラグで「実測値」と「既定値」を区別して報告できる。
pub(crate) fn resolve_wait_time(body: &str) -> (u64, u64, bool) {
    match extract_wait_time(body) {
        Some((minutes, seconds)) => (minutes, seconds, true),
        None => (UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES, 0, false),
    }
}

/// 旧 format (`Please wait **N minutes? and M seconds?**`) を抽出。
pub(crate) fn extract_old_format_wait_time(body: &str) -> Option<(u64, u64)> {
    let re_full = regex::Regex::new(r"Please wait \*?\*?(\d+) minutes? and (\d+) seconds?").ok()?;
    if let Some(caps) = re_full.captures(body) {
        let m: u64 = caps.get(1)?.as_str().parse().ok()?;
        let s: u64 = caps.get(2)?.as_str().parse().ok()?;
        return Some((m, s));
    }
    let re_min = regex::Regex::new(r"Please wait \*?\*?(\d+) minutes?").ok()?;
    let caps = re_min.captures(body)?;
    let m: u64 = caps.get(1)?.as_str().parse().ok()?;
    Some((m, 0))
}

/// 新 format (`More reviews will be available in N minutes? and M seconds?`) を抽出。
/// PR #182/#184 で実観測した CR 新フォーマット。
pub(crate) fn extract_new_format_wait_time(body: &str) -> Option<(u64, u64)> {
    let re_full =
        regex::Regex::new(r"More reviews will be available in (\d+) minutes? and (\d+) seconds?")
            .ok()?;
    if let Some(caps) = re_full.captures(body) {
        let m: u64 = caps.get(1)?.as_str().parse().ok()?;
        let s: u64 = caps.get(2)?.as_str().parse().ok()?;
        return Some((m, s));
    }
    let re_min = regex::Regex::new(r"More reviews will be available in (\d+) minutes?").ok()?;
    let caps = re_min.captures(body)?;
    let m: u64 = caps.get(1)?.as_str().parse().ok()?;
    Some((m, 0))
}

/// 第 3 世代 format (`**Next review available in:** **N minutes**`) を抽出。
///
/// PR #309 (2026-07-20) で実観測。ラベルと数値の間に markdown の `**` と `:` が
/// 挟まるため、区切りは `[:*\s]*` で吸収する (CR が強調記法を変えても壊れにくい)。
pub(crate) fn extract_next_review_format_wait_time(body: &str) -> Option<(u64, u64)> {
    let re_full = regex::Regex::new(
        r"Next review available in[:*\s]*(\d+) minutes?[*\s]*and[:*\s]*(\d+) seconds?",
    )
    .ok()?;
    if let Some(caps) = re_full.captures(body) {
        let m: u64 = caps.get(1)?.as_str().parse().ok()?;
        let s: u64 = caps.get(2)?.as_str().parse().ok()?;
        return Some((m, s));
    }
    let re_min = regex::Regex::new(r"Next review available in[:*\s]*(\d+) minutes?").ok()?;
    let caps = re_min.captures(body)?;
    let m: u64 = caps.get(1)?.as_str().parse().ok()?;
    Some((m, 0))
}

/// 既知 3 世代のいずれかに一致すれば `(minutes, seconds)` を返す。古い世代から順に試行。
pub(crate) fn extract_wait_time(body: &str) -> Option<(u64, u64)> {
    extract_old_format_wait_time(body)
        .or_else(|| extract_new_format_wait_time(body))
        .or_else(|| extract_next_review_format_wait_time(body))
}

/// ISO 8601 (`YYYY-MM-DDTHH:MM:SSZ` 形式) を unix epoch 秒に変換する。
pub(crate) fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let mut parts = s.split('T');
    let date = parts.next()?;
    let time = parts.next()?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;

    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month_check(year, month)).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }

    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        let idx = (m - 1) as usize;
        days += month_days[idx];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }
    days += day - 1;

    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

pub(crate) fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_in_month_check(year: i64, month: i64) -> i64 {
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let base = month_days[(month - 1) as usize];
    if month == 2 && is_leap_year(year) {
        base + 1
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PR #309 の実 rate-limit comment body (2026-07-20T12:10:47Z 投稿) の忠実な抜粋。
    ///
    /// 出典: `gh api repos/aloekun/claude-code-hook-test/issues/309/comments`。
    /// この incident が「CR 書式変更で rate-limit 検知が沈黙する」2 度目の regression
    /// (PR #182/#184 に次ぐ) を起こした実入力そのもの。ADR-049 に従い合成データでは
    /// なく実データを fixture 化する。
    ///
    /// 構造上の要点は 2 つ:
    /// - walkthrough header marker (`summarize by coderabbit.ai`) を **同一 comment 内に**
    ///   併せ持つ。`is_clean_walkthrough_comment` が rate-limit comment を除外していなければ
    ///   clean walkthrough と誤認され得る配置。
    /// - 待機時間が第 3 世代書式 (`**Next review available in:** **57 minutes**`)。
    const PR309_RATE_LIMIT_BODY: &str = r#"<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

[![Review Change Stack](https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg)](https://app.coderabbit.ai/change-stack/aloekun/claude-code-hook-test/pull/309)

<!-- review_stack_entry_end -->
<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->

> [!WARNING]
> ## Review limit reached
>
> `@aloekun`, you've reached your PR review limit, so we couldn't start this review.
>
> **Next review available in:** **57 minutes**
>
> Enable **usage-based reviews** in Billing to review now."#;

    /// PR #309 の実 comment を GH API の comments JSON 形として組み立てる。
    fn pr309_comments_json() -> String {
        serde_json::json!([{
            "user": {"login": "coderabbitai[bot]"},
            "body": PR309_RATE_LIMIT_BODY,
            "created_at": "2026-07-20T12:10:47Z"
        }])
        .to_string()
    }

    #[test]
    fn iso8601_epoch_zero() {
        assert_eq!(parse_iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn iso8601_known_date() {
        let ts = parse_iso8601_to_unix("2026-04-30T00:00:00Z").unwrap();
        assert!(ts > 1_735_689_600);
        assert!(ts < 1_798_761_600);
    }

    #[test]
    fn iso8601_rejects_invalid_month() {
        assert!(parse_iso8601_to_unix("2026-99-01T00:00:00Z").is_none());
    }

    #[test]
    fn iso8601_rejects_invalid_day() {
        assert!(parse_iso8601_to_unix("2026-02-30T00:00:00Z").is_none());
    }

    #[test]
    fn iso8601_handles_leap_year() {
        assert!(parse_iso8601_to_unix("2024-02-29T00:00:00Z").is_some());
        assert!(parse_iso8601_to_unix("2025-02-29T00:00:00Z").is_none());
    }

    #[test]
    fn wait_time_full_format() {
        let body = "Please wait **5 minutes and 13 seconds** before requesting another review.";
        assert_eq!(extract_wait_time(body), Some((5, 13)));
    }

    #[test]
    fn wait_time_singular_units() {
        let body = "Please wait 1 minute and 7 seconds before requesting another review.";
        assert_eq!(extract_wait_time(body), Some((1, 7)));
    }

    #[test]
    fn wait_time_minutes_only() {
        let body = "Please wait **30 minutes** before requesting another review.";
        assert_eq!(extract_wait_time(body), Some((30, 0)));
    }

    #[test]
    fn wait_time_no_match_returns_none() {
        assert_eq!(extract_wait_time("just a normal comment"), None);
    }

    #[test]
    fn rate_limit_detected_from_coderabbit_comment() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\n\nPlease wait **5 minutes and 13 seconds** before requesting another review.",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-04-29T00:00:00Z").unwrap();
        assert_eq!(result.wait_minutes, 5);
        assert_eq!(result.wait_seconds, 13);
        let base = parse_iso8601_to_unix("2026-04-30T00:00:00Z").unwrap();
        assert_eq!(result.until_unix_secs, base + 5 * 60 + 13 + 60);
    }

    #[test]
    fn rate_limit_picks_latest_when_multiple() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Rate limit exceeded\nPlease wait 5 minutes and 0 seconds", "created_at": "2026-04-29T00:00:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "Rate limit exceeded\nPlease wait 1 minute and 30 seconds", "created_at": "2026-04-30T00:00:00Z"}
        ]"#;
        let result = parse_rate_limit(json, "2026-04-29T00:00:00Z").unwrap();
        assert_eq!(result.wait_minutes, 1);
        assert_eq!(result.wait_seconds, 30);
    }

    #[test]
    fn rate_limit_ignores_non_coderabbit() {
        let json = r#"[{
            "user": {"login": "someuser"},
            "body": "Rate limit exceeded\nPlease wait 5 minutes and 0 seconds",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        assert!(parse_rate_limit(json, "2026-04-29T00:00:00Z").is_none());
    }

    #[test]
    fn rate_limit_no_match_when_unrelated_comment() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Review completed.",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        assert!(parse_rate_limit(json, "2026-04-29T00:00:00Z").is_none());
    }

    /// R2: marker は一致するが待機時間が既知 3 世代のどれにも一致しない未知書式でも、
    /// rate-limit として検出し続ける (旧実装は `None` を返し監視が silent success に倒れた)。
    /// 待機時間は既定値で埋め、`wait_time_parsed = false` で「読めなかった」ことを明示する。
    #[test]
    fn rate_limit_falls_back_to_default_wait_when_format_unknown() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded but format is unusual",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-04-29T00:00:00Z")
            .expect("marker 一致時は未知書式でも rate-limit として検出すべき");
        assert_eq!(result.wait_minutes, UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES);
        assert_eq!(result.wait_seconds, 0);
        assert!(
            !result.wait_time_parsed,
            "未知書式では既定値であることを wait_time_parsed=false で申告する"
        );
        let base = parse_iso8601_to_unix("2026-04-30T00:00:00Z").unwrap();
        assert_eq!(
            result.until_unix_secs,
            base + (UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES as i64) * 60 + 60
        );
    }

    /// R2 の fallback が「marker 無しの comment まで rate-limit 扱いする」方向に
    /// 効きすぎていないことを固定する (fallback の適用範囲は marker 一致後のみ)。
    #[test]
    fn rate_limit_fallback_does_not_fire_without_marker() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Next review available in: 57 minutes",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        assert!(
            parse_rate_limit(json, "2026-04-29T00:00:00Z").is_none(),
            "marker 非一致なら待機時間書式に一致しても rate-limit ではない"
        );
    }

    #[test]
    fn rate_limit_empty_json_returns_none() {
        assert!(parse_rate_limit("[]", "2026-04-29T00:00:00Z").is_none());
    }

    #[test]
    fn rate_limit_filters_out_past_session_comments() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\nPlease wait 5 minutes and 13 seconds",
            "created_at": "2026-04-29T00:00:00Z"
        }]"#;
        assert!(parse_rate_limit(json, "2026-04-30T00:00:00Z").is_none());
    }

    #[test]
    fn rate_limit_includes_comment_at_exact_push_time() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\nPlease wait 5 minutes and 13 seconds",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        assert!(parse_rate_limit(json, "2026-04-30T00:00:00Z").is_some());
    }

    #[test]
    fn rate_limit_uses_updated_at_when_present() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\nPlease wait 21 minutes before requesting another review.",
            "created_at": "2026-04-30T11:11:51Z",
            "updated_at": "2026-04-30T14:38:32Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-04-30T11:00:00Z").unwrap();
        let updated_unix = parse_iso8601_to_unix("2026-04-30T14:38:32Z").unwrap();
        assert_eq!(result.until_unix_secs, updated_unix + 21 * 60 + 60);
        assert_eq!(result.comment_event_time, "2026-04-30T14:38:32Z");
    }

    #[test]
    fn rate_limit_falls_back_to_created_at_when_updated_at_missing() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\nPlease wait 5 minutes and 0 seconds",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-04-29T00:00:00Z").unwrap();
        let base = parse_iso8601_to_unix("2026-04-30T00:00:00Z").unwrap();
        assert_eq!(result.until_unix_secs, base + 5 * 60 + 60);
        assert_eq!(result.comment_event_time, "2026-04-30T00:00:00Z");
    }

    #[test]
    fn rate_limit_edited_comment_yields_new_dedup_key() {
        let json_before_edit = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\nPlease wait 5 minutes and 0 seconds",
            "created_at": "2026-04-30T11:00:00Z",
            "updated_at": "2026-04-30T11:00:00Z"
        }]"#;
        let json_after_edit = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded\nPlease wait 21 minutes before requesting another review.",
            "created_at": "2026-04-30T11:00:00Z",
            "updated_at": "2026-04-30T14:38:32Z"
        }]"#;
        let before = parse_rate_limit(json_before_edit, "2026-04-30T10:00:00Z").unwrap();
        let after = parse_rate_limit(json_after_edit, "2026-04-30T10:00:00Z").unwrap();
        assert_ne!(
            before.comment_event_time, after.comment_event_time,
            "編集前後で dedup key が異なるべき"
        );
    }

    #[test]
    fn rate_limit_detected_from_new_format_with_html_marker_and_full_wait_time() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->\n\n> [!WARNING]\n> ## Review limit reached\n> \n> More reviews will be available in 36 minutes and 52 seconds. [Learn more](https://docs.coderabbit.ai/management/plans).",
            "created_at": "2026-05-29T08:16:12Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-05-29T00:00:00Z")
            .expect("new format with HTML marker + full wait time must be detected");
        assert_eq!(result.wait_minutes, 36);
        assert_eq!(result.wait_seconds, 52);
        let base = parse_iso8601_to_unix("2026-05-29T08:16:12Z").unwrap();
        assert_eq!(result.until_unix_secs, base + 36 * 60 + 52 + 60);
    }

    #[test]
    fn rate_limit_detected_from_new_format_with_minutes_only() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->\n\nMore reviews will be available in 30 minutes.",
            "created_at": "2026-05-29T08:00:00Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-05-29T00:00:00Z")
            .expect("new format minutes-only variant must be detected");
        assert_eq!(result.wait_minutes, 30);
        assert_eq!(result.wait_seconds, 0);
    }

    /// R2 / incident 再現: PR #309 の実 comment から第 3 世代書式の待機時間を読む。
    /// 旧実装ではここが `None` に倒れ、`decide()` に rate_limit が届かず silent success
    /// になっていた (本 incident の起点)。
    #[test]
    fn rate_limit_detected_from_pr309_incident_body() {
        let result = parse_rate_limit(&pr309_comments_json(), "2026-07-20T12:00:00Z")
            .expect("PR #309 の実 rate-limit comment は検出されなければならない");
        assert_eq!(result.wait_minutes, 57);
        assert_eq!(result.wait_seconds, 0);
        assert!(
            result.wait_time_parsed,
            "第 3 世代書式は既知書式として読めるので fallback ではない"
        );
        assert_eq!(result.comment_event_time, "2026-07-20T12:10:47Z");
        let base = parse_iso8601_to_unix("2026-07-20T12:10:47Z").unwrap();
        assert_eq!(result.until_unix_secs, base + 57 * 60 + 60);
    }

    /// PR #309 の実 comment は walkthrough header marker を併せ持つが、rate-limit
    /// comment である限り clean walkthrough と判定してはならない。
    /// (この排他が崩れると `decide()` が walkthrough_clean で早期 success に倒れる)
    #[test]
    fn pr309_incident_body_is_not_treated_as_clean_walkthrough() {
        let clean = crate::parsers::parse_walkthrough_clean_marker(
            &pr309_comments_json(),
            "2026-07-20T12:00:00Z",
        );
        assert!(
            !clean,
            "rate-limit comment は walkthrough header を含んでも clean 扱いしない"
        );
    }

    #[test]
    fn wait_time_next_review_format_minutes_only() {
        let body = "**Next review available in:** **57 minutes**";
        assert_eq!(extract_wait_time(body), Some((57, 0)));
    }

    #[test]
    fn wait_time_next_review_format_with_seconds() {
        let body = "**Next review available in:** **3 minutes and 20 seconds**";
        assert_eq!(extract_wait_time(body), Some((3, 20)));
    }

    /// 強調記法が付かない素の書式でも読めること (CR が markdown を変えても壊れない)。
    #[test]
    fn wait_time_next_review_format_without_markdown_emphasis() {
        let body = "Next review available in 12 minutes.";
        assert_eq!(extract_wait_time(body), Some((12, 0)));
    }

    #[test]
    fn rate_limit_picks_latest_when_mixed_old_and_new_formats() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Rate limit exceeded\nPlease wait 5 minutes and 0 seconds", "created_at": "2026-05-28T00:00:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "<!-- rate limited by coderabbit.ai -->\nMore reviews will be available in 15 minutes and 30 seconds.", "created_at": "2026-05-29T00:00:00Z"}
        ]"#;
        let result = parse_rate_limit(json, "2026-05-28T00:00:00Z")
            .expect("mixed old/new formats must resolve to newest comment");
        assert_eq!(result.wait_minutes, 15);
        assert_eq!(result.wait_seconds, 30);
    }
}
