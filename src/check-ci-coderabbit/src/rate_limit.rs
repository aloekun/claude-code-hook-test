//! CodeRabbit rate-limit parsing + ISO 8601 timestamp arithmetic。

use crate::markers::{is_rate_limit_comment, rate_limit_event_time};
use crate::models::{GhComment, RateLimitInfo};

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
    let (minutes, seconds) = extract_wait_time(body)?;
    let comment_unix = parse_iso8601_to_unix(event_time)?;

    let until_unix_secs = comment_unix + (minutes as i64) * 60 + (seconds as i64) + 60;

    Some(RateLimitInfo {
        until_unix_secs,
        comment_event_time: event_time.to_string(),
        wait_minutes: minutes,
        wait_seconds: seconds,
    })
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

/// 旧 / 新どちらかの format に一致すれば `(minutes, seconds)` を返す。旧 → 新の順で試行。
pub(crate) fn extract_wait_time(body: &str) -> Option<(u64, u64)> {
    extract_old_format_wait_time(body).or_else(|| extract_new_format_wait_time(body))
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

    #[test]
    fn rate_limit_no_match_when_no_wait_time() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded but format is unusual",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        assert!(parse_rate_limit(json, "2026-04-29T00:00:00Z").is_none());
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
