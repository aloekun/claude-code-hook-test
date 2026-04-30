//! 重複起動防止 file lock
//!
//! `start_monitoring` の polling + takt 並走を防ぐため、`.claude/pr-monitor.lock` に
//! PID + start_time を記録する。同時に複数の cli-pr-monitor が polling を回すと
//! Claude Code Max のレートリミットを浪費するため (PR #88 dogfood で実測)、
//! 1 リポジトリ 1 アクティブ監視 にゲートする。
//!
//! 仕様:
//!   - acquire(): lock file を atomic create。既存 lock が "fresh" (start_time が
//!     `stale_threshold_secs` 以内) なら None (= 別インスタンスが走行中、skip)。
//!     stale (timeout 超過) なら overwrite して取得。
//!   - Drop: lock file を削除。プロセス crash 時は file が残るが、stale 判定で
//!     `stale_threshold_secs` 経過後に次インスタンスが takeover できる。
//!
//! `--observe` / `--mark-notified` は read-only / one-shot mutation のため guard 対象外。
//! polling + takt を回す `start_monitoring` のみ guard する。

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::log::log_info;

const LOCK_FILENAME: &str = ".claude/pr-monitor.lock";
/// stale 判定 threshold。max_duration_secs (600s = 10min) の 3x で安全マージン。
const DEFAULT_STALE_THRESHOLD_SECS: i64 = 1800;

#[derive(Serialize, Deserialize)]
struct LockFile {
    pid: u32,
    start_time: String,
    mode: String,
}

/// Lock 取得成功時に保持する RAII guard。Drop で lock file を削除する。
pub(crate) struct MonitorLock {
    path: PathBuf,
}

impl Drop for MonitorLock {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            // already removed (race) なら無視。それ以外は warn。
            if e.kind() != std::io::ErrorKind::NotFound {
                log_info(&format!("[lock] cleanup 失敗: {}", e));
            }
        }
    }
}

/// Lock 取得結果。
pub(crate) enum LockResult {
    /// 取得成功。guard が drop されるまで保持される。
    Acquired(MonitorLock),
    /// 別インスタンスが fresh な lock を保持中 → skip 推奨。
    Busy {
        holder_pid: u32,
        holder_age_secs: i64,
    },
    /// lock ファイルの作成に失敗 (権限不足等)。lock 機能なしで継続可能。
    Unavailable { reason: String },
}

/// `start_monitoring` 用 lock を取得する。`mode` は debug 用の人間可読ラベル。
pub(crate) fn acquire(mode: &str) -> LockResult {
    acquire_at(lock_path(), mode, DEFAULT_STALE_THRESHOLD_SECS)
}

/// テスト用: lock path / stale threshold を引数化。
///
/// レース対策: `OpenOptions::create_new` で atomic create を試み、AlreadyExists の
/// 場合のみ既存 lock の stale 判定にフォールバックする。read-then-write の TOCTOU
/// race を排除する設計。stale 判定後の overwrite は仕様上 race を許容 (stale =
/// 監視者なし、複数 takeover が同時に成功しても無害)。
pub(crate) fn acquire_at(path: PathBuf, mode: &str, stale_threshold_secs: i64) -> LockResult {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let content = match build_lock_content(mode) {
        Some(c) => c,
        None => {
            return LockResult::Unavailable {
                reason: "lock content serialize 失敗".to_string(),
            }
        }
    };

    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                log_info(&format!("[lock] 新規 lock 書き込み失敗 (継続): {}", e));
            }
            LockResult::Acquired(MonitorLock { path })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // fresh な lock が存在する間は後続セッションを skip してレートリミット浪費を防ぐ
            if let Some((holder, age_secs)) = read_fresh_lock(&path, stale_threshold_secs) {
                return LockResult::Busy {
                    holder_pid: holder.pid,
                    holder_age_secs: age_secs,
                };
            }
            // stale takeover: overwrite。複数の takeover が並走しても last write wins で
            // どれか 1 つの guard が cleanup する (不変条件: lock file は最終的に消える)。
            if let Err(e) = std::fs::write(&path, content) {
                log_info(&format!("[lock] takeover 書き込み失敗 (継続): {}", e));
            }
            LockResult::Acquired(MonitorLock { path })
        }
        Err(e) => {
            // I/O エラー (権限不足等): lock なしで監視は継続可能。
            log_info(&format!("[lock] create_new 失敗 (lock なしで継続): {}", e));
            LockResult::Unavailable {
                reason: e.to_string(),
            }
        }
    }
}

fn build_lock_content(mode: &str) -> Option<String> {
    let lock = LockFile {
        pid: std::process::id(),
        start_time: crate::util::utc_now_iso8601(),
        mode: mode.to_string(),
    };
    match toml::to_string(&lock) {
        Ok(c) => Some(c),
        Err(e) => {
            log_info(&format!("[lock] serialize 失敗 (lock なしで継続): {}", e));
            None
        }
    }
}

/// 既存 lock が fresh なら `Some((LockFile, age_secs))` を返す。
/// stale (parse 失敗 / 超過) の場合は `None` (= 取得可)。
fn read_fresh_lock(path: &PathBuf, stale_threshold_secs: i64) -> Option<(LockFile, i64)> {
    let content = std::fs::read_to_string(path).ok()?;
    let lock: LockFile = match toml::from_str(&content) {
        Ok(l) => l,
        Err(e) => {
            log_info(&format!(
                "[lock] 既存 lock の parse 失敗 (stale 扱い): {}",
                e
            ));
            return None;
        }
    };
    let age_secs = parse_age_secs(&lock.start_time)?;
    if age_secs < stale_threshold_secs {
        Some((lock, age_secs))
    } else {
        log_info(&format!(
            "[lock] 既存 lock は stale (pid={}, age={}s > {}s threshold)、takeover",
            lock.pid, age_secs, stale_threshold_secs
        ));
        None
    }
}

/// ISO 8601 文字列から「現在からの経過秒数」を返す。parse 失敗時は None (stale 扱い)。
fn parse_age_secs(iso8601: &str) -> Option<i64> {
    let then = parse_iso8601(iso8601)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some(now.saturating_sub(then))
}

/// ISO 8601 (`2026-04-30T05:00:00Z` 形式) を Unix epoch secs にパース。
/// chrono を依存させずに済むよう手書き parse。
/// フィールドの値域を検証し、範囲外なら None を返す (corrupt lock → stale 扱い)。
fn parse_iso8601(s: &str) -> Option<i64> {
    let s = s.trim_end_matches('Z');
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

    // Range checks: out-of-bounds values cause index-out-of-bounds panic in
    // days_from_epoch. Returning None lets read_fresh_lock treat the lock as stale.
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }

    Some(unix_timestamp(year, month, day, hour, minute, second))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let base = month_days[(month - 1) as usize];
    if month == 2 && is_leap(year) {
        base + 1
    } else {
        base
    }
}

/// 単純な Unix epoch 計算 (UTC 前提、うるう秒は無視)。
fn unix_timestamp(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    let days = days_from_epoch(year, month, day);
    days * 86400 + hour * 3600 + minute * 60 + second
}

fn days_from_epoch(year: i64, month: i64, day: i64) -> i64 {
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        let idx = (m - 1) as usize;
        days += month_days[idx];
        if m == 2 && is_leap(year) {
            days += 1;
        }
    }
    days + day - 1
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn lock_path() -> PathBuf {
    PathBuf::from(LOCK_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn acquire_in_clean_dir_succeeds() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        match acquire_at(path.clone(), "test", 1800) {
            LockResult::Acquired(_lock) => {
                assert!(path.exists(), "lock file should be created");
            }
            LockResult::Busy { .. } => panic!("expected Acquired in clean dir"),
            LockResult::Unavailable { reason } => {
                panic!(
                    "expected Acquired in clean dir, got Unavailable: {}",
                    reason
                )
            }
        }
    }

    #[test]
    fn drop_removes_lock_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        {
            let _lock = match acquire_at(path.clone(), "test", 1800) {
                LockResult::Acquired(l) => l,
                LockResult::Busy { .. } => panic!("expected Acquired"),
                LockResult::Unavailable { reason } => {
                    panic!("expected Acquired, got Unavailable: {}", reason)
                }
            };
            assert!(path.exists());
        }
        assert!(!path.exists(), "Drop should remove the lock file");
    }

    #[test]
    fn fresh_lock_blocks_second_acquire() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        let _first = match acquire_at(path.clone(), "first", 1800) {
            LockResult::Acquired(l) => l,
            LockResult::Busy { .. } => panic!("expected Acquired for first"),
            LockResult::Unavailable { reason } => {
                panic!("expected Acquired for first, got Unavailable: {}", reason)
            }
        };

        match acquire_at(path.clone(), "second", 1800) {
            LockResult::Busy { holder_pid, .. } => {
                assert_eq!(holder_pid, std::process::id());
            }
            LockResult::Acquired(_) => panic!("second should be Busy while first holds"),
            LockResult::Unavailable { reason } => {
                panic!("second should be Busy, got Unavailable: {}", reason)
            }
        }
    }

    #[test]
    fn stale_lock_is_taken_over() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        // 古い start_time を持つ lock を仕込む (1980-01-01 = epoch+10年、確実に stale)
        let stale = LockFile {
            pid: 999_999,
            start_time: "1980-01-01T00:00:00Z".into(),
            mode: "stale-test".into(),
        };
        std::fs::write(&path, toml::to_string(&stale).unwrap()).unwrap();

        // threshold=1800s でも 1980 は stale 判定 → takeover 成功
        match acquire_at(path.clone(), "takeover", 1800) {
            LockResult::Acquired(_lock) => {
                let content = std::fs::read_to_string(&path).unwrap();
                assert!(content.contains(&format!("pid = {}", std::process::id())));
            }
            LockResult::Busy { .. } => panic!("stale lock should allow takeover"),
            LockResult::Unavailable { reason } => {
                panic!(
                    "stale lock should allow takeover, got Unavailable: {}",
                    reason
                )
            }
        }
    }

    #[test]
    fn concurrent_acquire_only_one_wins() {
        // 真の concurrency test: 8 thread が同一 path に同時 acquire を試み、
        // 1 つだけが Acquired (lock 保持) で残りは Busy になることを確認。
        // create_new による atomic create が機能していない場合、複数が
        // Acquired になり test 失敗する。
        use std::sync::{Arc, Barrier};
        use std::thread;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = vec![];
        for _ in 0..8 {
            let p = path.clone();
            let b = barrier.clone();
            handles.push(thread::spawn(move || {
                b.wait();
                matches!(acquire_at(p, "concurrent", 1800), LockResult::Acquired(_))
            }));
        }
        let acquired_count = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|&v| v)
            .count();
        // race-safe な実装なら 1 thread のみが Acquired。
        // (Drop は thread 終了で走るため、その後の状態は不定 — 取得回数だけを検証)
        assert_eq!(
            acquired_count, 1,
            "exactly one thread should acquire the lock under concurrency"
        );
    }

    #[test]
    fn lock_format_matches_util_iso8601() {
        // util::utc_now_iso8601() の出力 format と本 module の parse_iso8601 が
        // round-trip することを確認 (advisor 指摘の "format alignment" check)。
        let now = crate::util::utc_now_iso8601();
        let parsed = parse_iso8601(&now);
        assert!(
            parsed.is_some(),
            "util's iso8601 format must parse: {}",
            now
        );
    }

    #[test]
    fn corrupt_lock_is_taken_over() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        // parse 不能な内容を仕込む
        std::fs::write(&path, "this is not valid toml :::").unwrap();

        match acquire_at(path.clone(), "takeover", 1800) {
            LockResult::Acquired(_lock) => {}
            LockResult::Busy { .. } => panic!("corrupt lock should be treated as stale"),
            LockResult::Unavailable { reason } => {
                panic!(
                    "corrupt lock should allow takeover, got Unavailable: {}",
                    reason
                )
            }
        }
    }

    #[test]
    fn parse_iso8601_round_trip() {
        // 2026-04-30T00:00:00Z = 56 yr from epoch with leap year handling
        // 単純にパースが動くことを確認
        let ts = parse_iso8601("2026-04-30T00:00:00Z").unwrap();
        // 2026-04-30 should be > 2025-01-01 (1735689600 sec) and < 2027-01-01
        assert!(ts > 1_735_689_600);
        assert!(ts < 1_798_761_600);
    }

    #[test]
    fn is_leap_correctness() {
        assert!(is_leap(2024));
        assert!(!is_leap(2025));
        assert!(!is_leap(1900)); // century non-leap
        assert!(is_leap(2000)); // 400-year leap
    }

    #[test]
    fn parse_iso8601_rejects_out_of_range_month() {
        // month=99 would cause index-out-of-bounds in days_from_epoch without bounds check
        assert_eq!(parse_iso8601("2026-99-30T00:00:00Z"), None);
    }

    #[test]
    fn parse_iso8601_rejects_out_of_range_fields() {
        assert_eq!(parse_iso8601("1969-01-01T00:00:00Z"), None); // year < 1970
        assert_eq!(parse_iso8601("2026-00-01T00:00:00Z"), None); // month = 0
        assert_eq!(parse_iso8601("2026-13-01T00:00:00Z"), None); // month > 12
        assert_eq!(parse_iso8601("2026-01-00T00:00:00Z"), None); // day = 0
        assert_eq!(parse_iso8601("2026-01-32T00:00:00Z"), None); // day > 31
        assert_eq!(parse_iso8601("2026-01-01T24:00:00Z"), None); // hour = 24
        assert_eq!(parse_iso8601("2026-01-01T00:60:00Z"), None); // minute = 60
        assert_eq!(parse_iso8601("2026-01-01T00:00:60Z"), None); // second = 60
        assert_eq!(parse_iso8601("2026-02-29T00:00:00Z"), None); // day 29 in non-leap year
    }

    #[test]
    fn io_error_returns_unavailable() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Create a regular file at the path that will be used as a parent directory.
        // create_dir_all on a file path silently fails, so open() gets a non-AlreadyExists error.
        let file_as_dir = tmp.path().join("notadir");
        std::fs::write(&file_as_dir, "content").unwrap();
        let path = file_as_dir.join("pr-monitor.lock");
        match acquire_at(path, "test", 1800) {
            LockResult::Unavailable { .. } => {}
            LockResult::Acquired(_) => panic!("expected Unavailable on I/O error, got Acquired"),
            LockResult::Busy { .. } => panic!("expected Unavailable on I/O error, got Busy"),
        }
    }
}
