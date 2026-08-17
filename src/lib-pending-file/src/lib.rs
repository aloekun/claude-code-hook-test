//! post-merge-feedback pending file の共有スキーマと UTC ヘルパー (ADR-029)
//!
//! 3 crate 間で重複していた `PendingFile` 構造体・status 定数・ISO 8601 ヘルパーを
//! 一か所に集約したライブラリクレート。
//!
//! 消費側:
//!   - `cli-merge-pipeline`       — pending file の新規排他作成・上書き
//!   - `hooks-stop-feedback-dispatch` — pending file の読み取りと dispatched 遷移
//!   - `cli-pr-monitor`           — ISO 8601 UTC ヘルパーのみ利用

use serde::{Deserialize, Serialize};

// ─── Schema constants ───

/// pending file のスキーマバージョン。非互換変更時に bump する。
pub const SCHEMA_VERSION: u32 = 1;

/// ファイル名 (`.claude/` 配下に配置)
pub const FILE_NAME: &str = "post-merge-feedback-pending.json";

/// ADR-029 で定義された status 値
pub const STATUS_PENDING: &str = "pending";
pub const STATUS_DISPATCHED: &str = "dispatched";
pub const STATUS_CONSUMED: &str = "consumed";

// ─── Shared struct ───

/// pending file の JSON スキーマ (ADR-029 §Pending file JSON スキーマ v1)
///
/// `producer` は schema v1 互換の optional フィールド。取りこぼし発生時に
/// 「誰が書いた pending が消えたか」を破損残骸からも追跡可能にするための観測性補助。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingFile {
    pub schema_version: u32,
    pub pr_number: u64,
    pub owner_repo: String,
    pub prompt: String,
    pub status: String,
    pub created_at: String,
    pub dispatched_at: Option<String>,
    pub consumed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
}

// ─── Input validation ───

/// `{owner}/{repo}` 形式の文字列を検証する (ADR-029 §競合ポリシー の security-review 反映)。
///
/// 許容文字: ASCII 英数字 + `_` `.` `-`。スラッシュはちょうど 1 つ、owner/repo とも非空。
/// newline / 制御文字は弾く (pending file / additionalContext への注入防御)。
pub fn is_valid_owner_repo(s: &str) -> bool {
    let Some((owner, repo)) = s.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !repo.is_empty()
        && !repo.contains('/')
        && owner.chars().all(is_repo_ident_char)
        && repo.chars().all(is_repo_ident_char)
}

fn is_repo_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}

// ─── UTC ISO 8601 helpers ───
//
// Hatcher's proleptic Gregorian civil-date algorithm (pure std, no chrono).
// Reference: https://howardhinnant.github.io/date_algorithms.html

/// Days from the proleptic Gregorian epoch (0000-03-01) to the Unix epoch (1970-01-01).
const CIVIL_EPOCH_OFFSET: i64 = 719_468;
/// Days in a 400-year Gregorian era.
const DAYS_PER_ERA: i64 = 146_097;
/// DAYS_PER_ERA - 1; used for the era-floor sign correction.
const DAYS_PER_ERA_M1: i64 = 146_096;
/// Days in a 4-year cycle (excluding century boundaries).
const DAYS_PER_4Y: u64 = 1_460;
/// Days in a 100-year cycle.
const DAYS_PER_100Y: u64 = 36_524;
/// Days in an ordinary year.
const DAYS_PER_YEAR: u64 = 365;
/// Years per 400-year Gregorian era.
const YEARS_PER_ERA: i64 = 400;
/// Multiplier for the month-to-day-of-year encoding: (5*mp + 2) / 153.
const MONTH_ENCODE_MUL: u64 = 5;
/// Divisor for the month-to-day-of-year encoding.
const MONTH_ENCODE_DIV: u64 = 153;
/// Seconds per hour.
const SECS_PER_HOUR: u64 = 3_600;
/// Seconds per minute.
const SECS_PER_MIN: u64 = 60;
/// Seconds per day.
const SECS_PER_DAY: u64 = 86_400;

/// epoch 秒 → ISO 8601 UTC 文字列 (`YYYY-MM-DDTHH:MM:SSZ`)。
pub fn epoch_secs_to_iso8601(epoch: u64) -> String {
    let day_count = (epoch / SECS_PER_DAY) as i64;
    let time_of_day = epoch % SECS_PER_DAY;

    let z = day_count + CIVIL_EPOCH_OFFSET;
    let era = (if z >= 0 { z } else { z - DAYS_PER_ERA_M1 }) / DAYS_PER_ERA;
    let doe = (z - era * DAYS_PER_ERA) as u64;
    let yoe = (doe - doe / DAYS_PER_4Y + doe / DAYS_PER_100Y - doe / (DAYS_PER_ERA_M1 as u64))
        / DAYS_PER_YEAR;
    let y = yoe as i64 + era * YEARS_PER_ERA;
    let doy = doe - (DAYS_PER_YEAR * yoe + yoe / 4 - yoe / 100);
    let mp = (MONTH_ENCODE_MUL * doy + 2) / MONTH_ENCODE_DIV;
    let d = doy - (MONTH_ENCODE_DIV * mp + 2) / MONTH_ENCODE_MUL + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let hour = time_of_day / SECS_PER_HOUR;
    let min = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;
    let sec = time_of_day % SECS_PER_MIN;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, min, sec
    )
}

/// 現在時刻を ISO 8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`) で返す。
pub fn utc_now_iso8601() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_secs_to_iso8601(now.as_secs())
}

/// ISO 8601 UTC 文字列 (`YYYY-MM-DDTHH:MM:SSZ`) → epoch 秒。[`epoch_secs_to_iso8601`] の逆変換。
///
/// `2026-08-10T17:32:12.584Z` のような小数秒は truncate する (takt の `meta.json` が
/// 小数秒付きで書くため。整数秒精度で十分)。末尾 `Z` は必須 (UTC 以外は扱わない)。
///
/// 値域外 (year < 1970 / month 13 / 平年の 2/29 等) は `None`。これは
/// `days_from_civil` の index 計算が破綻する入力を弾くためであり、呼び手は `None` を
/// 「時刻が確定できない」として明示的に扱う (silent な epoch 0 fallback を作らない)。
pub fn iso8601_to_epoch_secs(s: &str) -> Option<i64> {
    let without_frac = strip_fractional_secs(s.strip_suffix('Z')?)?;
    let (date, time) = without_frac.split_once('T')?;
    let [year, month, day] = split_exact_decimals::<3>(date, '-')?;
    let [hour, minute, second] = split_exact_decimals::<3>(time, ':')?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(
        days * SECS_PER_DAY as i64
            + hour * SECS_PER_HOUR as i64
            + minute * SECS_PER_MIN as i64
            + second,
    )
}

/// 小数秒を落とす。`.` が無ければそのまま返す。
///
/// `.` がある場合は **1 桁以上の ASCII 数字のみ**を小数部として許す。ここを検証しないと
/// `2026-01-01T00:00:00.invalidZ` のような壊れた値が「小数秒付き」として通り、
/// 呼び手が「時刻を確定できた」と誤認する。
fn strip_fractional_secs(s: &str) -> Option<&str> {
    let Some((head, frac)) = s.split_once('.') else {
        return Some(s);
    };
    if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(head)
}

/// `sep` で**ちょうど `N` 個**に分割し、各要素を 10 進数として読む。
///
/// 要素数が `N` に満たない / 超える場合と、ASCII 数字以外 (符号・空白・英字) を含む
/// 場合は `None`。個数を検証しないと `2026-01-01-extra` の余剰要素が黙って捨てられ、
/// 構造が壊れた文字列を受理してしまう。
fn split_exact_decimals<const N: usize>(s: &str, sep: char) -> Option<[i64; N]> {
    let mut out = [0_i64; N];
    let mut parts = s.split(sep);
    for slot in out.iter_mut() {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *slot = part.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(out)
}

/// Hatcher's `days_from_civil`: 暦日 → Unix epoch からの日数。
/// [`epoch_secs_to_iso8601`] が使う `civil_from_days` の逆写像 (同じ定数を共有する)。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - (YEARS_PER_ERA - 1) }) / YEARS_PER_ERA;
    let yoe = y - era * YEARS_PER_ERA;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (MONTH_ENCODE_DIV as i64 * mp + 2) / MONTH_ENCODE_MUL as i64 + day - 1;
    let doe = yoe * DAYS_PER_YEAR as i64 + yoe / 4 - yoe / 100 + doy;
    era * DAYS_PER_ERA + doe - CIVIL_EPOCH_OFFSET
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    const MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let base = MONTH_DAYS[(month - 1) as usize];
    if month == 2 && is_leap_year(year) {
        base + 1
    } else {
        base
    }
}

/// 現在の epoch 秒を返す (stale 判定用)。
pub fn utc_now_epoch_secs() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── epoch_secs_to_iso8601 ───

    #[test]
    fn epoch_zero_is_unix_epoch() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn epoch_day_boundary() {
        assert_eq!(epoch_secs_to_iso8601(86400), "1970-01-02T00:00:00Z");
    }

    #[test]
    fn epoch_known_date() {
        assert_eq!(epoch_secs_to_iso8601(1_775_044_800), "2026-04-01T12:00:00Z");
    }

    #[test]
    fn epoch_leap_year() {
        assert_eq!(epoch_secs_to_iso8601(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn epoch_end_of_day() {
        assert_eq!(epoch_secs_to_iso8601(1_775_087_999), "2026-04-01T23:59:59Z");
    }

    #[test]
    fn utc_now_iso8601_format() {
        let s = utc_now_iso8601();
        assert_eq!(s.len(), "1970-01-01T00:00:00Z".len());
        assert!(s.ends_with('Z'));
        assert_eq!(s.chars().nth(4), Some('-'));
        assert_eq!(s.chars().nth(7), Some('-'));
        assert_eq!(s.chars().nth(10), Some('T'));
        assert_eq!(s.chars().nth(13), Some(':'));
        assert_eq!(s.chars().nth(16), Some(':'));
    }

    #[test]
    fn parse_unix_epoch() {
        assert_eq!(iso8601_to_epoch_secs("1970-01-01T00:00:00Z"), Some(0));
    }

    /// `epoch_secs_to_iso8601` との round-trip。片方だけ壊れたら落ちる。
    ///
    /// fixture はうるう日 (2024-02-29) / 日境界 / 日終端 / 非うるう世紀年 (2100) を含む。
    #[test]
    fn parse_round_trips_with_the_formatter() {
        const ROUND_TRIP_EPOCHS: [u64; 6] = [
            0,
            86_400,
            1_709_164_800,
            1_775_044_800,
            1_775_087_999,
            4_102_444_800,
        ];
        for epoch in ROUND_TRIP_EPOCHS {
            let iso = epoch_secs_to_iso8601(epoch);
            assert_eq!(
                iso8601_to_epoch_secs(&iso),
                Some(epoch as i64),
                "round-trip failed for {iso}"
            );
        }
    }

    /// takt の `meta.json` は `"startTime": "2026-08-10T17:32:12.584Z"` と小数秒付きで書く。
    #[test]
    fn parse_truncates_fractional_seconds() {
        assert_eq!(
            iso8601_to_epoch_secs("2026-08-10T17:32:12.584Z"),
            iso8601_to_epoch_secs("2026-08-10T17:32:12Z"),
            "小数秒は reject ではなく truncate すること"
        );
    }

    #[test]
    fn parse_rejects_out_of_range_and_malformed() {
        const REJECTED: [(&str, &str); 16] = [
            ("1969-12-31T23:59:59Z", "pre-epoch"),
            ("2026-13-01T00:00:00Z", "month > 12"),
            ("2026-00-01T00:00:00Z", "month = 0"),
            ("2026-02-29T00:00:00Z", "平年の 2/29"),
            ("2026-01-01T24:00:00Z", "hour = 24"),
            ("2026-01-01T00:00:00", "末尾 Z 無し"),
            ("2026-01-01 00:00:00Z", "T 区切り無し"),
            ("", "空文字"),
            ("garbage", "非 ISO 8601"),
            ("2026-01-01-extraT00:00:00Z", "date に 4 個目の要素"),
            ("2026-01-01T00:00:00:extraZ", "time に 4 個目の要素"),
            ("2026-01T00:00:00Z", "date が 2 要素しかない"),
            ("2026-01-01T00:00Z", "time が 2 要素しかない"),
            ("2026-01-01T00:00:00.invalidZ", "小数部が数字でない"),
            ("2026-01-01T00:00:00.Z", "小数部が空"),
            ("2026-01-01T00:00:+0Z", "符号付き要素"),
        ];
        for (input, why) in REJECTED {
            assert_eq!(
                iso8601_to_epoch_secs(input),
                None,
                "{why} は None にすること: {input:?}"
            );
        }
    }

    #[test]
    fn parse_accepts_leap_day_in_leap_year() {
        assert_eq!(
            iso8601_to_epoch_secs("2024-02-29T00:00:00Z"),
            Some(1_709_164_800)
        );
    }

    #[test]
    fn valid_owner_repo_accepts_typical_slugs() {
        assert!(is_valid_owner_repo("aloekun/claude-code-hook-test"));
        assert!(is_valid_owner_repo("octo-org/my.repo"));
        assert!(is_valid_owner_repo("a/b"));
        assert!(is_valid_owner_repo("Ab_12/X.y-z"));
    }

    #[test]
    fn valid_owner_repo_rejects_malformed() {
        assert!(!is_valid_owner_repo(""));
        assert!(!is_valid_owner_repo("noslash"));
        assert!(!is_valid_owner_repo("/missing-owner"));
        assert!(!is_valid_owner_repo("missing-repo/"));
        assert!(!is_valid_owner_repo("a/b/c"));
        assert!(!is_valid_owner_repo("has space/repo"));
        assert!(!is_valid_owner_repo("owner/repo\nfoo")); // newline injection
        assert!(!is_valid_owner_repo("owner/repo\r"));
        assert!(!is_valid_owner_repo("owner/repo\t"));
        assert!(!is_valid_owner_repo("owner!/repo"));
    }

    // ─── PendingFile serde ───

    #[test]
    fn pending_file_without_producer_deserializes() {
        let json = r#"{
            "schema_version": 1,
            "pr_number": 42,
            "owner_repo": "o/r",
            "prompt": "post-merge-feedback",
            "status": "pending",
            "created_at": "2026-04-23T10:00:00Z",
            "dispatched_at": null,
            "consumed_at": null
        }"#;
        let p: PendingFile = serde_json::from_str(json).unwrap();
        assert_eq!(p.producer, None);
        assert_eq!(p.pr_number, 42);
    }
}
