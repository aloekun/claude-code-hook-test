//! epoch 秒と暦日 (proleptic Gregorian) の相互変換ヘルパー (pure-std, chrono 不使用)。
//!
//! `lib-telemetry` / `cli-takt-timings` と同じ Howard Hinnant のアルゴリズムを用いる。
//! 本 exe は firing `ts` (`YYYY-MM-DDTHH:MM:SSZ`) から月キー (`YYYY-MM`) を取り出し、
//! partition ファイル名 (`firings-YYYY-MM-DD-<pid>.jsonl`) の日付から retention 経過日数を
//! 決定論的に計算する。時刻取得の副作用は main が担い、本 module は純粋変換のみを提供する。
//! Reference: <https://howardhinnant.github.io/date_algorithms.html>

/// 1 日の秒数。
const SECS_PER_DAY: u64 = 86_400;
/// proleptic Gregorian epoch (0000-03-01) から Unix epoch (1970-01-01) までの日数。
const CIVIL_EPOCH_OFFSET: i64 = 719_468;
/// 400 年 Gregorian era の日数。
const DAYS_PER_ERA: i64 = 146_097;

/// epoch 秒 → `YYYY-MM-DD` (UTC)。レポートファイル名・当日判定に使う。
pub fn epoch_secs_to_date(epoch: u64) -> String {
    let (y, m, d) = civil_from_days((epoch / SECS_PER_DAY) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// epoch 秒 → `YYYY-MM` (UTC)。月次 rollup / 当月判定のキー。
pub fn epoch_secs_to_month(epoch: u64) -> String {
    let (y, m, _d) = civil_from_days((epoch / SECS_PER_DAY) as i64);
    format!("{y:04}-{m:02}")
}

/// epoch 秒 → `YYYY-MM-DDTHH:MM:SSZ` (UTC)。rollup / レポートの生成時刻記録用。
pub fn epoch_secs_to_iso8601(epoch: u64) -> String {
    let (y, m, d) = civil_from_days((epoch / SECS_PER_DAY) as i64);
    let tod = epoch % SECS_PER_DAY;
    let (hh, mm, ss) = (tod / 3_600, (tod % 3_600) / 60, tod % 60);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// epoch 秒 → Unix epoch からの日数 (retention の経過日数計算用)。
pub fn epoch_secs_to_day(epoch: u64) -> i64 {
    (epoch / SECS_PER_DAY) as i64
}

/// `ts` 文字列 (`YYYY-MM-DDTHH:MM:SS...`) から月キー `YYYY-MM` を取り出す。
///
/// 先頭 7 文字が `YYYY-MM` (4-2 の桁と `-`) の体裁を満たさなければ `None` を返し、
/// 呼び出し側が当該行を skip する (壊れ行耐性)。
pub fn month_key_of_ts(ts: &str) -> Option<String> {
    let head = ts.get(..7)?;
    let bytes = head.as_bytes();
    let shaped = bytes.len() == 7
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit();
    if shaped {
        Some(head.to_string())
    } else {
        None
    }
}

/// `YYYY-MM-DD` 日付文字列 → Unix epoch からの日数。retention 判定で partition ファイル名の
/// 日付を経過日数に変換するために使う。桁が揃わない / 数値でない入力は `None`。
pub fn date_str_to_day(date: &str) -> Option<i64> {
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
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

/// Unix epoch からの日数 → 暦日 (y, m, d)。Howard Hinnant のアルゴリズム。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + CIVIL_EPOCH_OFFSET;
    let era = (if z >= 0 { z } else { z - 146_096 }) / DAYS_PER_ERA;
    let doe = z - era * DAYS_PER_ERA;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-07-20T14:56:23Z の epoch 秒。
    const T_2026_07_20: u64 = 1_784_559_383;

    #[test]
    fn epoch_to_date_and_month_and_iso() {
        assert_eq!(epoch_secs_to_date(T_2026_07_20), "2026-07-20");
        assert_eq!(epoch_secs_to_month(T_2026_07_20), "2026-07");
        assert_eq!(epoch_secs_to_iso8601(T_2026_07_20), "2026-07-20T14:56:23Z");
    }

    #[test]
    fn unix_epoch_roundtrips() {
        assert_eq!(epoch_secs_to_date(0), "1970-01-01");
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn month_key_of_ts_extracts_and_validates() {
        assert_eq!(month_key_of_ts("2026-07-20T14:56:23Z").as_deref(), Some("2026-07"));
        assert_eq!(month_key_of_ts("2026-12-01").as_deref(), Some("2026-12"));
        assert!(month_key_of_ts("").is_none());
        assert!(month_key_of_ts("2026/07").is_none());
        assert!(month_key_of_ts("abcd-ef").is_none());
    }

    #[test]
    fn date_str_to_day_matches_known_offsets() {
        assert_eq!(date_str_to_day("1970-01-01"), Some(0));
        assert_eq!(date_str_to_day("1970-01-02"), Some(1));
        assert_eq!(
            date_str_to_day("2026-07-21").unwrap() - date_str_to_day("2026-07-20").unwrap(),
            1
        );
        assert!(date_str_to_day("2026-13-01").is_none());
        assert!(date_str_to_day("not-a-date").is_none());
    }

    #[test]
    fn retention_days_are_exact() {
        let old = date_str_to_day("2026-04-01").unwrap();
        let now = epoch_secs_to_day(T_2026_07_20);
        assert_eq!(now - old, 110);
    }

}
