//! ISO 8601 タイムスタンプ → Unix epoch ミリ秒 (整数) の pure-std パーサ。
//!
//! takt の meta.json `startTime` と log の `timestamp` は `YYYY-MM-DDTHH:MM:SS.sssZ`
//! 形式。窓判定 (`--since`/`--until`) は日付のみ (`YYYY-MM-DD`) も受け付ける。
//! chrono を使わず Howard Hinnant の proleptic Gregorian civil-date アルゴリズムで
//! epoch ミリ秒へ変換する (lib-pending-file の `epoch_secs_to_iso8601` の逆変換に相当)。
//!
//! **整数ミリ秒**で保持するのは、旧 ps1 の `[datetime]` (100ns tick の厳密 10 進) と
//! duration を一致させるため。f64 秒で累積すると個別 phase 所要が丸め境界付近で ±0.1s
//! ずれることがある (実データで 1 件観測)。ミリ秒精度の入力に対し整数演算は厳密。
//!
//! PowerShell 版が `[datetime]::Parse(..., AssumeUniversal)` で行っていた「UTC 前提の
//! 正規化」を再現する: タイムゾーン指定が無い / `Z` の場合は UTC、`±HH:MM` オフセット
//! 付きは UTC に換算する。パース不能な入力は `None` を返し、呼び出し側で当該 run/phase
//! を skip する (旧 ps1 の try/catch → continue と同じ流儀)。

const MILLIS_PER_DAY: i64 = 86_400_000;
const MILLIS_PER_HOUR: i64 = 3_600_000;
const MILLIS_PER_MIN: i64 = 60_000;
const MILLIS_PER_SEC: i64 = 1_000;
const CIVIL_EPOCH_OFFSET: i64 = 719_468;
const DAYS_PER_ERA: i64 = 146_097;

/// `YYYY-MM-DD[THH:MM:SS[.fff]][Z|±HH:MM]` を UTC epoch ミリ秒 (整数) に変換する。
pub fn parse_iso8601_to_epoch_millis(input: &str) -> Option<i64> {
    let s = input.trim();
    let (date_part, time_part) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };

    let (y, m, d) = parse_date(date_part)?;
    let days = days_from_civil(y, m, d);

    let (time_millis, offset_millis) = match time_part {
        Some(t) => parse_time_with_offset(t)?,
        None => (0, 0),
    };

    Some(days * MILLIS_PER_DAY + time_millis - offset_millis)
}

fn parse_date(s: &str) -> Option<(i64, i64, i64)> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// 時刻部 (`HH:MM:SS[.fff][Z|±HH:MM]`) を (ミリ秒, オフセットミリ秒) に分解する。
fn parse_time_with_offset(t: &str) -> Option<(i64, i64)> {
    let (core, offset_millis) = split_offset(t)?;
    let time_millis = parse_time(core)?;
    Some((time_millis, offset_millis))
}

/// 末尾のタイムゾーン (`Z` / `±HH:MM`) を切り出し、(時刻本体, オフセットミリ秒) を返す。
fn split_offset(t: &str) -> Option<(&str, i64)> {
    if let Some(core) = t.strip_suffix('Z') {
        return Some((core, 0));
    }
    let sign_pos = t.char_indices().skip(1).find(|&(_, c)| c == '+' || c == '-');
    let Some((idx, sign)) = sign_pos else {
        return Some((t, 0));
    };
    let core = &t[..idx];
    let off = &t[idx + 1..];
    let mut parts = off.split(':');
    let oh: i64 = parts.next()?.parse().ok()?;
    let om: i64 = parts.next().unwrap_or("0").parse().ok()?;
    let magnitude = oh * MILLIS_PER_HOUR + om * MILLIS_PER_MIN;
    Some((core, if sign == '-' { -magnitude } else { magnitude }))
}

fn parse_time(core: &str) -> Option<i64> {
    let (hms, frac) = match core.split_once('.') {
        Some((h, f)) => (h, Some(f)),
        None => (core, None),
    };
    let mut parts = hms.split(':');
    let hh: i64 = parts.next()?.parse().ok()?;
    let mm: i64 = parts.next()?.parse().ok()?;
    let ss: i64 = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let millis = match frac {
        Some(f) => parse_fraction_millis(f)?,
        None => 0,
    };
    Some(hh * MILLIS_PER_HOUR + mm * MILLIS_PER_MIN + ss * MILLIS_PER_SEC + millis)
}

/// 小数秒文字列をミリ秒に変換する (先頭 3 桁を採用し 3 桁へゼロ詰め)。
fn parse_fraction_millis(frac: &str) -> Option<i64> {
    if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut ms = 0i64;
    for i in 0..3 {
        ms = ms * 10 + frac.as_bytes().get(i).map_or(0, |b| (b - b'0') as i64);
    }
    Some(ms)
}

/// 暦日 (y, m, d) → Unix epoch からの日数 (負値可)。Howard Hinnant のアルゴリズム。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * DAYS_PER_ERA + doe - CIVIL_EPOCH_OFFSET
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(s: &str) -> i64 {
        parse_iso8601_to_epoch_millis(s).expect("parse")
    }

    #[test]
    fn unix_epoch_is_zero() {
        assert_eq!(ms("1970-01-01T00:00:00Z"), 0);
        assert_eq!(ms("1970-01-01"), 0);
    }

    #[test]
    fn known_instant_matches_expected_epoch() {
        assert_eq!(ms("2000-01-01T00:00:00Z"), 946_684_800_000);
    }

    #[test]
    fn milliseconds_are_included_exactly() {
        assert_eq!(ms("2026-07-19T19:20:16.148Z") - ms("2026-07-19T19:20:16Z"), 148);
    }

    #[test]
    fn fractional_padding_short_and_long() {
        assert_eq!(ms("1970-01-01T00:00:00.1Z"), 100);
        assert_eq!(ms("1970-01-01T00:00:00.12Z"), 120);
        assert_eq!(ms("1970-01-01T00:00:00.123Z"), 123);
        assert_eq!(ms("1970-01-01T00:00:00.123999Z"), 123, "3 桁超は切り捨て");
    }

    #[test]
    fn duration_between_two_timestamps_is_exact() {
        let start = ms("2026-07-19T19:20:16.246Z");
        let end = ms("2026-07-19T19:20:49.328Z");
        assert_eq!(end - start, 33_082);
    }

    #[test]
    fn date_only_is_midnight_utc() {
        assert_eq!(ms("2026-07-17"), ms("2026-07-17T00:00:00Z"));
    }

    #[test]
    fn positive_offset_converts_to_utc() {
        assert_eq!(ms("2026-07-19T09:00:00+09:00"), ms("2026-07-19T00:00:00Z"));
    }

    #[test]
    fn negative_offset_converts_to_utc() {
        assert_eq!(ms("2026-07-19T00:00:00-05:00"), ms("2026-07-19T05:00:00Z"));
    }

    #[test]
    fn missing_timezone_assumed_utc() {
        assert_eq!(ms("2026-07-19T12:00:00"), ms("2026-07-19T12:00:00Z"));
    }

    #[test]
    fn far_future_sentinel_parses() {
        assert!(parse_iso8601_to_epoch_millis("9999-12-31").is_some());
    }

    #[test]
    fn ordering_is_preserved() {
        assert!(ms("2026-07-17") < ms("2026-07-18T13:00:00Z"));
        assert!(ms("2026-07-18T13:00:00Z") < ms("2026-07-19"));
    }

    #[test]
    fn malformed_inputs_return_none() {
        assert!(parse_iso8601_to_epoch_millis("").is_none());
        assert!(parse_iso8601_to_epoch_millis("not-a-date").is_none());
        assert!(parse_iso8601_to_epoch_millis("2026-13-01").is_none());
        assert!(parse_iso8601_to_epoch_millis("2026-07-19T25:xx").is_none());
        assert!(parse_iso8601_to_epoch_millis("2026-07-19T10:00:00.abcZ").is_none());
    }
}
