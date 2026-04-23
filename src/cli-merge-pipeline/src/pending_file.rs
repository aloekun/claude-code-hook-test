//! post-merge-feedback pending file の読み書きと入力検証 (ADR-029)
//!
//! pending file は `.claude/post-merge-feedback-pending.json` に配置され、
//! cli-merge-pipeline が post-merge ステップ (`type = "ai"`) で書き込み、
//! hooks-stop-feedback-dispatch が Stop 時に検出して Claude に skill 起動を指示する。
//!
//! 書き込みは常に「tmp file → `fs::rename`」の 2 段階で行う。
//!
//! atomic 保証の前提 (ADR-029):
//!   - Windows 10 1607+ / NTFS or ReFS: `FileRenameInfoEx` 経路で atomic overwrite
//!     (本プロジェクトのターゲット環境)
//!   - POSIX: `rename(2)` により atomic overwrite
//!   - 旧 Windows / 非対応 FS: non-atomic fallback (他プロセスが中間状態を観測可能)
//!
//! 非 atomic 環境では POST_MERGE_FEEDBACK_TRIGGER が 1 回発火失敗する可能性があるが、
//! 次のマージで復帰可能なので本 module は fallback を許容する。`fs::rename` の Err は
//! 戻り値として呼び出し側へ伝播させ、呼び出し側が log を出す (silent fail させない)。
//! 必要になれば `FileRenameInfoEx` の直接呼び出しを検討する (現段階では YAGNI)。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// プロセス内で一意な tmp ファイル名を生成するためのカウンタ。
/// 複数の writer が同時に `write_atomic` を呼んでも tmp パスが衝突しない。
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// pending file のスキーマバージョン。
///
/// 非互換変更時に bump する。hooks-stop-feedback-dispatch (task 1-C) は
/// これと一致しない pending を「破損」として削除する。
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// ファイル名 (`.claude/` 配下に配置)
pub(crate) const FILE_NAME: &str = "post-merge-feedback-pending.json";

/// ADR-029 で定義された status 値
pub(crate) const STATUS_PENDING: &str = "pending";
pub(crate) const STATUS_DISPATCHED: &str = "dispatched";
pub(crate) const STATUS_CONSUMED: &str = "consumed";

/// pending file の JSON スキーマ (ADR-029 §Pending file JSON スキーマ v1)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PendingFile {
    pub(crate) schema_version: u32,
    pub(crate) pr_number: u64,
    pub(crate) owner_repo: String,
    pub(crate) prompt: String,
    pub(crate) status: String,
    pub(crate) created_at: String,
    pub(crate) dispatched_at: Option<String>,
    pub(crate) consumed_at: Option<String>,
}

/// 既存 pending file の読み取り結果。
///
/// status 未知値は Corrupt 扱いとする (ADR-029 は `pending`/`dispatched`/`consumed`
/// のみを schema_version=1 で enumerate しているため、それ以外は schema drift とみなす)。
#[derive(Debug)]
pub(crate) enum ExistingPending {
    /// ファイル不在。通常経路。
    None,
    /// size 0 / parse 失敗 / schema_version 不一致 / 未知 status。削除して再書き込みする。
    Corrupt(String),
    /// status = "consumed"。上書き OK。
    Consumed,
    /// status = "pending" または "dispatched"。書き込みをスキップする (WARN)。
    Active(String),
}

/// 既存 pending file の状態を判定する。
pub(crate) fn read_existing(path: &Path) -> ExistingPending {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return ExistingPending::None,
    };
    if meta.len() == 0 {
        return ExistingPending::Corrupt("size=0".to_string());
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return ExistingPending::Corrupt(format!("read error: {}", e)),
    };
    let pending: PendingFile = match serde_json::from_str(&content) {
        Ok(p) => p,
        Err(e) => return ExistingPending::Corrupt(format!("parse error: {}", e)),
    };
    if pending.schema_version != SCHEMA_VERSION {
        return ExistingPending::Corrupt(format!(
            "schema_version mismatch (got {}, want {})",
            pending.schema_version, SCHEMA_VERSION
        ));
    }
    match pending.status.as_str() {
        STATUS_CONSUMED => ExistingPending::Consumed,
        STATUS_PENDING | STATUS_DISPATCHED => ExistingPending::Active(pending.status),
        other => ExistingPending::Corrupt(format!("unknown status '{}'", other)),
    }
}

/// pending file を atomic に書き込む (tmp → rename)。
///
/// 失敗時は Err を返し、呼び出し側が log を出す。tmp ファイルは
/// 成功時は rename によって消えるが、失敗時は残骸が残らないよう best-effort で削除する。
pub(crate) fn write_atomic(path: &Path, pending: &PendingFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(pending)
        .map_err(|e| format!("pending file のシリアライズ失敗: {}", e))?;
    // 複数 writer が同時実行しても tmp ファイルが上書き競合しない。
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pending".to_string());
    let tmp_name = format!("{}.tmp.{}.{}", file_name, std::process::id(), counter);
    let tmp_path = path.with_file_name(tmp_name);
    if let Err(e) = std::fs::write(&tmp_path, &json) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "pending tmp file への書き込み失敗: {} ({})",
            tmp_path.display(),
            e
        ));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "pending file の rename 失敗 ({} → {}): {}",
            tmp_path.display(),
            path.display(),
            e
        ));
    }
    Ok(())
}

/// `{owner}/{repo}` 形式の文字列を検証する (ADR-029 todo 1-B の security-review 反映)。
///
/// 許容文字: ASCII 英数字 + `_` `.` `-`。スラッシュはちょうど 1 つ、owner/repo とも非空。
/// newline / 制御文字は弾く (pending file / additionalContext への注入防御)。
pub(crate) fn is_valid_owner_repo(s: &str) -> bool {
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

/// pending file のデフォルト配置先 (exe と同じディレクトリ = `.claude/`)。
///
/// 本プロジェクトは `pnpm deploy:hooks` で exe を `.claude/` に配置するため、
/// exe の親ディレクトリが pending file の正しい置き場になる。派生プロジェクトも同様。
pub(crate) fn default_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

// ─── UTC ISO 8601 helper ───
//
// cli-pr-monitor/src/util.rs にも同等の pub(crate) 関数が存在する。
// 1-C (hooks-stop-feedback-dispatch) も同じ helper を必要とするため、
// 3 callers になった時点で lib へ切り出すか判断する (現段階では duplicate でよい)。

/// 現在時刻を ISO 8601 UTC 文字列に変換する (std のみ, chrono 不要)。
pub(crate) fn utc_now_iso8601() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_secs_to_iso8601(now.as_secs())
}

// Constants for Hatcher's proleptic Gregorian civil-date algorithm.
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

fn epoch_secs_to_iso8601(epoch: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pending(status: &str) -> PendingFile {
        PendingFile {
            schema_version: SCHEMA_VERSION,
            pr_number: 123,
            owner_repo: "aloekun/claude-code-hook-test".to_string(),
            prompt: "post-merge-feedback".to_string(),
            status: status.to_string(),
            created_at: "2026-04-23T10:00:00Z".to_string(),
            dispatched_at: None,
            consumed_at: None,
        }
    }

    fn unique_tmp(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pending-{}-{}-{}.json",
            label,
            std::process::id(),
            // nanosecond で同一 pid 内の衝突も回避
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
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
        assert!(!is_valid_owner_repo("a/b/c")); // 複数スラッシュ
        assert!(!is_valid_owner_repo("has space/repo"));
        assert!(!is_valid_owner_repo("owner/repo\nfoo")); // newline injection
        assert!(!is_valid_owner_repo("owner/repo\r"));
        assert!(!is_valid_owner_repo("owner/repo\t"));
        assert!(!is_valid_owner_repo("owner!/repo"));
    }

    #[test]
    fn write_atomic_creates_file_without_tmp_residue() {
        let path = unique_tmp("write-atomic");
        let pending = sample_pending(STATUS_PENDING);

        write_atomic(&path, &pending).unwrap();

        // 正しく書き込まれている
        let loaded: PendingFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, pending);

        // 新形式 "{basename}.tmp.{pid}.{counter}" は path.with_extension で特定できないため、
        // 同ディレクトリ内で ".tmp." を含む残骸ファイルの有無をスキャンして確認する。
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        let basename = path.file_name().unwrap().to_string_lossy().into_owned();
        let has_residue = std::fs::read_dir(dir).unwrap().flatten().any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(&basename) && name.contains(".tmp.")
        });
        assert!(
            !has_residue,
            "tmp residue left behind under {}",
            dir.display()
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_atomic_is_safe_under_concurrent_writers() {
        use std::sync::Arc;

        let path = Arc::new(unique_tmp("concurrent"));
        let pending = Arc::new(sample_pending(STATUS_PENDING));

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let p = Arc::clone(&path);
                let pf = Arc::clone(&pending);
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        let _ = write_atomic(&p, &pf);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // 最終ファイルが有効な JSON であること
        let content = std::fs::read_to_string(path.as_ref()).unwrap();
        let loaded: PendingFile = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded, *pending);

        let _ = std::fs::remove_file(path.as_ref());
    }

    #[test]
    fn read_existing_returns_none_when_absent() {
        let path = unique_tmp("absent");
        assert!(matches!(read_existing(&path), ExistingPending::None));
    }

    #[test]
    fn read_existing_returns_corrupt_for_empty_file() {
        let path = unique_tmp("empty");
        std::fs::write(&path, "").unwrap();

        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => assert!(reason.contains("size=0")),
            other => panic!("expected Corrupt, got {:?}", other),
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_corrupt_for_invalid_json() {
        let path = unique_tmp("bad-json");
        std::fs::write(&path, "not a json").unwrap();

        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => assert!(reason.contains("parse")),
            other => panic!("expected Corrupt, got {:?}", other),
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_corrupt_for_schema_mismatch() {
        let path = unique_tmp("bad-schema");
        let mut pending = sample_pending(STATUS_PENDING);
        pending.schema_version = 99;
        write_atomic(&path, &pending).unwrap();

        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => {
                assert!(reason.contains("schema_version"));
            }
            other => panic!("expected Corrupt, got {:?}", other),
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_corrupt_for_unknown_status() {
        let path = unique_tmp("bad-status");
        let pending = sample_pending("garbage");
        write_atomic(&path, &pending).unwrap();

        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => assert!(reason.contains("unknown status")),
            other => panic!("expected Corrupt, got {:?}", other),
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_active_for_pending_and_dispatched() {
        let path_p = unique_tmp("active-pending");
        write_atomic(&path_p, &sample_pending(STATUS_PENDING)).unwrap();
        match read_existing(&path_p) {
            ExistingPending::Active(s) => assert_eq!(s, STATUS_PENDING),
            other => panic!("expected Active(pending), got {:?}", other),
        }
        let _ = std::fs::remove_file(&path_p);

        let path_d = unique_tmp("active-dispatched");
        write_atomic(&path_d, &sample_pending(STATUS_DISPATCHED)).unwrap();
        match read_existing(&path_d) {
            ExistingPending::Active(s) => assert_eq!(s, STATUS_DISPATCHED),
            other => panic!("expected Active(dispatched), got {:?}", other),
        }
        let _ = std::fs::remove_file(&path_d);
    }

    #[test]
    fn read_existing_returns_consumed_for_consumed_status() {
        let path = unique_tmp("consumed");
        write_atomic(&path, &sample_pending(STATUS_CONSUMED)).unwrap();
        assert!(matches!(read_existing(&path), ExistingPending::Consumed));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn utc_now_iso8601_matches_expected_format() {
        let s = utc_now_iso8601();
        // YYYY-MM-DDTHH:MM:SSZ = 20 chars
        assert_eq!(s.len(), 20, "unexpected length: {}", s);
        assert!(s.ends_with('Z'));
        assert_eq!(s.chars().nth(4), Some('-'));
        assert_eq!(s.chars().nth(7), Some('-'));
        assert_eq!(s.chars().nth(10), Some('T'));
        assert_eq!(s.chars().nth(13), Some(':'));
        assert_eq!(s.chars().nth(16), Some(':'));
    }

    #[test]
    fn epoch_secs_to_iso8601_epoch_zero_is_unix_epoch() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }
}
