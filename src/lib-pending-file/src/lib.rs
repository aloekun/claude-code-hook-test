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

    // ─── is_valid_owner_repo ───

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
