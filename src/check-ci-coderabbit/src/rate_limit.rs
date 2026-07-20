//! CodeRabbit rate-limit parsing + ISO 8601 timestamp arithmetic。

use crate::markers::{is_rate_limit_comment, rate_limit_event_time};
use crate::models::{GhComment, RateLimitInfo};

/// 待ち時間を読み取れなかった rate-limit comment に充てる既定の待ち分数。
///
/// CR が書式を変えても「レート制限は起きている」という事実は marker で判っている。
/// ここで `None` を返して**制限なし扱いにするのが最悪の失敗**なので (実観測: 監視が
/// 「CodeRabbit 指摘なし」と誤報告して success 判定した)、既定値で park させる。
///
/// 短すぎても実害は小さい: wakeup 後に再 poll し、まだ制限中なら再度 park する
/// (self-correcting)。過去の実観測値は 5 / 15 / 21 / 30 / 36 分。
const FALLBACK_WAIT_MINUTES: u64 = 15;

/// push 以降に CR が投稿した rate-limit comment のうち**最も新しいもの**を返す。
///
/// 新しい順で選ぶのは、CR が同一 comment を編集して待ち時間を更新するため
/// (`rate_limit_event_time` が `updated_at` を優先する理由と対)。
fn latest_rate_limit_comment<'a>(
    comments: &'a [GhComment],
    push_time: &str,
) -> Option<&'a GhComment> {
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
    candidates.first().copied()
}

/// CodeRabbit rate-limit comment を検出し、reset 時刻 (unix epoch) を返す。
///
/// **marker が一致したら必ず `Some` を返す**。待ち時間のパースに失敗しても
/// `FALLBACK_WAIT_MINUTES` で代替し `wait_time_parsed: false` を立てる。
/// 「rate-limit と判っているのに待ち時間が読めないから制限なしとして扱う」のは
/// fail-open であり、CR の書式変更のたびに silent regression を生む (ADR-034)。
pub(crate) fn parse_rate_limit(json: &str, push_time: &str) -> Option<RateLimitInfo> {
    let comments: Vec<GhComment> = serde_json::from_str(json).ok()?;
    let latest = latest_rate_limit_comment(&comments, push_time)?;

    let body = latest.body.as_deref()?;
    let event_time = rate_limit_event_time(latest)?;
    let comment_unix = parse_iso8601_to_unix(event_time)?;

    let (minutes, seconds, wait_time_parsed) = match extract_wait_time(body) {
        Some((m, s)) => (m, s, true),
        None => {
            eprintln!(
                "[check-ci] Warning: rate-limit comment を検出しましたが待ち時間の書式が未知です。\
                 既定値 {}分で park します。CR が書式を変更した可能性があるため \
                 extract_wait_time に新書式の追加が必要です (ADR-034)。",
                FALLBACK_WAIT_MINUTES
            );
            (FALLBACK_WAIT_MINUTES, 0, false)
        }
    };

    let until_unix_secs = comment_unix + (minutes as i64) * 60 + (seconds as i64) + 60;

    Some(RateLimitInfo {
        until_unix_secs,
        comment_event_time: event_time.to_string(),
        wait_minutes: minutes,
        wait_seconds: seconds,
        wait_time_parsed,
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

/// 現行 format (`**Next review available in:** **N minutes**`) を抽出。
/// PR #307 (2026-07-20) で実観測した CR の 3 番目の書式。
///
/// markdown の強調 (`**`) が `in:` の直後と数値の直前の両方に入るため、
/// `\**` と `\s*` で任意個の `*` と空白を吸収する。
pub(crate) fn extract_current_format_wait_time(body: &str) -> Option<(u64, u64)> {
    let re_full = regex::Regex::new(
        r"Next review available in:\**\s*\**(\d+) minutes? and \**(\d+) seconds?",
    )
    .ok()?;
    if let Some(caps) = re_full.captures(body) {
        let m: u64 = caps.get(1)?.as_str().parse().ok()?;
        let s: u64 = caps.get(2)?.as_str().parse().ok()?;
        return Some((m, s));
    }
    let re_min = regex::Regex::new(r"Next review available in:\**\s*\**(\d+) minutes?").ok()?;
    let caps = re_min.captures(body)?;
    let m: u64 = caps.get(1)?.as_str().parse().ok()?;
    Some((m, 0))
}

/// 既知の 3 書式のいずれかに一致すれば `(minutes, seconds)` を返す。旧 → 新 → 現行の順で試行。
///
/// **書式を足すときは必ず実観測した本文で test を書くこと**。ここに落ちると
/// `parse_rate_limit` が既定値へフォールバックし、wakeup 時刻が実際の reset と
/// ズレる (制限なし扱いにはならないが、無駄な poll が増える)。
pub(crate) fn extract_wait_time(body: &str) -> Option<(u64, u64)> {
    extract_old_format_wait_time(body)
        .or_else(|| extract_new_format_wait_time(body))
        .or_else(|| extract_current_format_wait_time(body))
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

    /// **契約変更 (PR #307 incident、2026-07-20)**: 旧実装はこのケースで `None`
    /// (= rate-limit なし) を返しており、本 test はその挙動を固定していた。
    /// しかしそれこそが「CR が書式を変えると監視が誤って success を返す」原因だった
    /// ため、marker 一致時は既定値付きで必ず `Some` を返す契約へ改めた。
    #[test]
    fn rate_limit_reported_with_fallback_when_wait_time_format_is_unknown() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "Rate limit exceeded but format is unusual",
            "created_at": "2026-04-30T00:00:00Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-04-29T00:00:00Z")
            .expect("marker 一致なら待ち時間が読めなくても rate-limit として報告する");
        assert!(!result.wait_time_parsed);
        assert_eq!(result.wait_minutes, FALLBACK_WAIT_MINUTES);
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

    /// PR #307 (2026-07-20) の incident 再現 (bad): CR が 3 番目の書式に変えたため
    /// `extract_wait_time` が None になり、`parse_rate_limit` が **rate-limit なし**を
    /// 返していた。結果、監視が「CodeRabbit 指摘なし」= success と誤報告した。
    ///
    /// body は実際に PR #307 へ投稿された comment から採取したもの (ADR-049 の流儀:
    /// incident 由来の fixture は実データを使う)。
    #[test]
    fn rate_limit_detected_from_current_format_next_review_available_in() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->\n\n> [!WARNING]\n> ## Review limit reached\n> \n> `@aloekun`, you've reached your PR review limit, so we couldn't start this review.\n> \n> **Next review available in:** **15 minutes**\n",
            "created_at": "2026-07-20T10:06:43Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-07-20T10:06:34Z")
            .expect("現行書式の rate-limit comment を検出できること (PR #307 incident)");
        assert_eq!(result.wait_minutes, 15);
        assert_eq!(result.wait_seconds, 0);
        assert!(
            result.wait_time_parsed,
            "書式を追加したので実測値として読めていること",
        );
    }

    /// 構造的な回帰防止 (good): marker は一致するが待ち時間の書式が**未知**でも
    /// `None` (= rate-limit なし) を返さないこと。
    ///
    /// これが本 incident の本質。書式追加だけでは 4 回目の変更で同じ silent regression が
    /// 再発するため、「marker 一致 → 必ず rate-limit として扱う」を契約として固定する。
    #[test]
    fn unknown_wait_time_format_still_reports_rate_limit_with_fallback() {
        let json = r#"[{
            "user": {"login": "coderabbitai[bot]"},
            "body": "<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->\n\nReviews resume at some point in the future.",
            "created_at": "2026-07-20T10:06:43Z"
        }]"#;
        let result = parse_rate_limit(json, "2026-07-20T10:00:00Z").expect(
            "待ち時間が読めなくても rate-limit として報告すること (None は制限なし扱い = 誤 success の原因)",
        );
        assert!(
            !result.wait_time_parsed,
            "既定値で代替したことを下流へ伝えること",
        );
        assert_eq!(result.wait_minutes, FALLBACK_WAIT_MINUTES);
        let base = parse_iso8601_to_unix("2026-07-20T10:06:43Z").unwrap();
        assert_eq!(
            result.until_unix_secs,
            base + (FALLBACK_WAIT_MINUTES as i64) * 60 + 60,
        );
    }

    /// 現行書式の分・秒併記 variant (CR が秒を付け足した場合に備える)。
    #[test]
    fn current_format_with_minutes_and_seconds() {
        let body = "**Next review available in:** **3 minutes and 20 seconds**";
        assert_eq!(extract_wait_time(body), Some((3, 20)));
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
