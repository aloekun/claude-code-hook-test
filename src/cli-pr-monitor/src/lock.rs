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
//! `--mark-notified` は one-shot mutation のため guard 対象外。
//! single-iteration check + takt を回す `start_monitoring` のみ guard する。

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

/// `create_new` 成功から内容書き込み完了までの窓を吸収する猶予 (WP-15)。
///
/// `create_new` は atomic だが、その直後にファイルは**空**で存在する。この窓で
/// 別プロセスが読むと TOML parse に失敗し、素朴に「壊れている = stale」と扱うと
/// 全員が takeover して**同時取得**が起きる (Linux で 6/6 スレッドが取得する
/// 実測不具合。Windows はスケジューリングの差で顕在化していなかっただけ)。
///
/// 実際の書き込みはミリ秒で完了するため数秒あれば十分。短く保つことで、
/// 「create 直後に crash して空ファイルが残った」場合の巻き添えも数秒で解ける。
const LOCK_WRITE_WINDOW_SECS: i64 = 5;

/// parse 不能な lock の pid は不明。表示用に「不明」を表す番兵。
const UNKNOWN_HOLDER_PID: u32 = 0;

/// 2 つの時刻から経過秒を求める。**mtime が未来なら 0 (作成直後) とみなす**。
///
/// `duration_since` は mtime が未来だと `Err` を返す。これを「齢が不明」として
/// 扱うと呼び出し側が stale 判定に倒れ、`create_new` 直後の空 lock が takeover
/// されて WP-15 で塞いだ同時取得レースがクロックスキュー経由で再発する。
/// クラウド / コンテナでスキューは珍しくないため、未来 mtime は「たった今作られた」
/// と解釈するのが安全側 (CodeRabbit PR #307 指摘)。
fn age_secs_between(modified: std::time::SystemTime, now: std::time::SystemTime) -> i64 {
    match now.duration_since(modified) {
        Ok(elapsed) => i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// ファイル自身の mtime から経過秒を求める (内容が読めない lock の齢判定用)。
///
/// `None` は metadata / mtime の取得自体に失敗した場合のみ。
fn file_age_secs(path: &PathBuf) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(age_secs_between(modified, std::time::SystemTime::now()))
}

/// parse 不能な lock を「保持者が書き込み中」とみなせるか判定する。
///
/// 2 つの状況を内容で区別する:
/// - **空**: `create_new` は成功したが `write_all` がまだ = 保持者が書き込み中。
///   busy (`Some`) を返して同時取得を防ぐ。
/// - **非空だが不正**: 本当に壊れた lock。従来どおり `None` を返して takeover させる。
///
/// 空の側にも `LOCK_WRITE_WINDOW_SECS` の齢制限を掛ける。create 直後に保持者が
/// crash すると空ファイルが残るが、この制限が無いと以降の取得が永久に阻まれるため。
///
/// pid は内容が読めない以上 unknown。
fn holder_still_writing(
    path: &PathBuf,
    content: &str,
    parse_error: &toml::de::Error,
) -> Option<(LockFile, i64)> {
    if !content.trim().is_empty() {
        log_info(&format!(
            "[lock] 既存 lock の parse 失敗 (内容あり = 破損、stale 扱い): {}",
            parse_error
        ));
        return None;
    }

    let age_secs = file_age_secs(path)?;
    if age_secs >= LOCK_WRITE_WINDOW_SECS {
        log_info(&format!(
            "[lock] 空の lock が {}s 以上残存 (create 直後の crash とみなし stale 扱い)",
            LOCK_WRITE_WINDOW_SECS
        ));
        return None;
    }

    Some((
        LockFile {
            pid: UNKNOWN_HOLDER_PID,
            start_time: String::new(),
            mode: String::new(),
        },
        age_secs,
    ))
}

/// 既存 lock が fresh なら `Some((LockFile, age_secs))` を返す。
/// stale (超過 / 古くて壊れている) の場合は `None` (= 取得可)。
fn read_fresh_lock(path: &PathBuf, stale_threshold_secs: i64) -> Option<(LockFile, i64)> {
    let content = std::fs::read_to_string(path).ok()?;
    let lock: LockFile = match toml::from_str(&content) {
        Ok(l) => l,
        Err(e) => return holder_still_writing(path, &content, &e),
    };
    let past_time = PastTime::from_iso8601_now(&lock.start_time)?;
    let age_secs = past_time.age_secs();
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

/// 過去性が型レベルで保証された timestamp。
///
/// 「parse 成功 + (then <= now) を確認」の 2 ステップを `from_iso8601_now` /
/// `from_parts` に閉じ込めることで、`age_secs()` の戻り値が常に非負である
/// invariant を構造的に保証する。
///
/// この型導入の動機は `saturating_sub` 系の silent semantic mismatch を排除
/// すること。過去の bug class:
///   - `parse_age_secs` が future timestamp に対し `saturating_sub` で 0 を返し、
///     破損 future-dated lock が永続 fresh 扱いとなり crash recovery が機能しなかった。
///
/// PastTime は construction 時に future timestamp を `None` で reject するため、
/// 同型の silent fresh bug を型層で再発不能化する (Bundle W / PR #96 follow-up)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PastTime {
    epoch_secs: i64,
    captured_now: i64,
}

impl PastTime {
    /// ISO 8601 文字列を parse し、system clock の現在と比較して past-ness を検証する。
    /// parse 失敗 / future timestamp / system clock 取得失敗のいずれでも `None`。
    fn from_iso8601_now(iso8601: &str) -> Option<Self> {
        let then = parse_iso8601(iso8601)?;
        let now = current_unix_secs()?;
        Self::from_parts(then, now)
    }

    /// テスト注入 / proptest 用: `now` を引数で受ける variant。
    /// `then > now` (future) の場合 `None`。それ以外は invariant を満たす PastTime を返す。
    fn from_parts(then_epoch_secs: i64, now_epoch_secs: i64) -> Option<Self> {
        if then_epoch_secs > now_epoch_secs {
            return None;
        }
        Some(Self {
            epoch_secs: then_epoch_secs,
            captured_now: now_epoch_secs,
        })
    }

    /// 経過秒数 (construction 時点の `captured_now - epoch_secs`)。
    /// invariant により常に非負。
    fn age_secs(&self) -> i64 {
        debug_assert!(self.captured_now >= self.epoch_secs);
        self.captured_now - self.epoch_secs
    }
}

fn current_unix_secs() -> Option<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs()
        .try_into()
        .ok()
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
        match acquire_at(path.clone(), "test", DEFAULT_STALE_THRESHOLD_SECS) {
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
            let _lock = match acquire_at(path.clone(), "test", DEFAULT_STALE_THRESHOLD_SECS) {
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
        let _first = match acquire_at(path.clone(), "first", DEFAULT_STALE_THRESHOLD_SECS) {
            LockResult::Acquired(l) => l,
            LockResult::Busy { .. } => panic!("expected Acquired for first"),
            LockResult::Unavailable { reason } => {
                panic!("expected Acquired for first, got Unavailable: {}", reason)
            }
        };

        match acquire_at(path.clone(), "second", DEFAULT_STALE_THRESHOLD_SECS) {
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
        match acquire_at(path.clone(), "takeover", DEFAULT_STALE_THRESHOLD_SECS) {
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
        //
        // 2 barrier 構成の意図: `start` で全 thread 同時に acquire_at に突入させ、
        // `finish` で全 thread が判定を終えるまで Acquired guard を保持する。
        // 1 barrier だと先行 thread の guard が判定後に即 drop され、後続 thread が
        // 逐次 Acquired する flaky window が生じる (CR finding E)。
        use std::sync::{Arc, Barrier};
        use std::thread;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        let start = Arc::new(Barrier::new(8));
        let finish = Arc::new(Barrier::new(8));
        let mut handles = vec![];
        for _ in 0..8 {
            let p = path.clone();
            let start_b = start.clone();
            let finish_b = finish.clone();
            handles.push(thread::spawn(move || {
                start_b.wait();
                let result = acquire_at(p, "concurrent", DEFAULT_STALE_THRESHOLD_SECS);
                let acquired = matches!(result, LockResult::Acquired(_));
                // 全 thread の判定が終わるまで result (Acquired なら guard) を保持
                finish_b.wait();
                acquired
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

    /// クロックスキュー対策 (CodeRabbit PR #307): mtime が**未来**でも齢 0 とみなすこと。
    ///
    /// 旧実装は `duration_since` の `Err` を `None` に潰しており、呼び出し側が
    /// 「齢不明 = stale」に倒れて空 lock を takeover していた。つまり WP-15 で
    /// 塞いだ同時取得レースが、クロックスキューという別経路から再発しうる。
    #[test]
    fn future_mtime_is_treated_as_just_created() {
        let now = std::time::SystemTime::now();
        let future = now + std::time::Duration::from_secs(3600);
        assert_eq!(
            age_secs_between(future, now),
            0,
            "未来 mtime は「たった今作られた」と解釈すること (stale 誤判定を防ぐ)",
        );
    }

    /// 通常経路 (good): 過去の mtime は経過秒をそのまま返すこと。
    #[test]
    fn past_mtime_yields_elapsed_seconds() {
        let now = std::time::SystemTime::now();
        let past = now - std::time::Duration::from_secs(42);
        assert_eq!(age_secs_between(past, now), 42);
    }

    /// WP-15 incident 再現 (bad): `create_new` 直後の**空** lock を stale と誤判定
    /// しないこと。
    ///
    /// 由来: 2026-07-20 の Linux 実測 (WSL Ubuntu 24.04)。`create_new` は atomic
    /// だが直後のファイルは空で、内容書き込みまでの窓に別スレッドが読むと TOML
    /// parse に失敗する。これを「壊れている = stale」と扱っていたため全員が
    /// takeover し、8 スレッド中 6 つが同時に Acquired になった。Windows では
    /// スケジューリングの差で顕在化していなかっただけで、設計上の欠陥は同じ。
    ///
    /// 修正の核心は「parse 不能でも十分新しければ書き込み中 = busy とみなす」。
    #[test]
    fn empty_lock_file_is_treated_as_busy_not_stale() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        std::fs::write(&path, "").unwrap();

        let result = acquire_at(path, "probe", DEFAULT_STALE_THRESHOLD_SECS);

        assert!(
            matches!(result, LockResult::Busy { .. }),
            "空 lock は「保持者が書き込み中」= Busy とすること。Acquired だと\
             create_new の排他が無意味になり同時取得が起きる (WP-15 の不具合)",
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
    fn future_timestamp_lock_is_taken_over() {
        // 時計巻き戻し / 破損 future timestamp の lock が永続 fresh で塩漬けに
        // ならず、stale 扱いで takeover されることを確認 (CR finding D)。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        // 9999 年は確実に未来 (parse_iso8601 上限内)
        let future = LockFile {
            pid: 999_999,
            start_time: "9999-01-01T00:00:00Z".into(),
            mode: "future-test".into(),
        };
        std::fs::write(&path, toml::to_string(&future).unwrap()).unwrap();

        match acquire_at(path.clone(), "takeover", DEFAULT_STALE_THRESHOLD_SECS) {
            LockResult::Acquired(_lock) => {
                let content = std::fs::read_to_string(&path).unwrap();
                assert!(
                    content.contains(&format!("pid = {}", std::process::id())),
                    "lock should be overwritten with current PID"
                );
            }
            LockResult::Busy {
                holder_age_secs, ..
            } => panic!(
                "future timestamp should be treated as stale, got Busy with age={}s",
                holder_age_secs
            ),
            LockResult::Unavailable { reason } => {
                panic!(
                    "future-stale takeover should succeed, got Unavailable: {}",
                    reason
                )
            }
        }
    }

    #[test]
    fn corrupt_lock_is_taken_over() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        // parse 不能な内容を仕込む
        std::fs::write(&path, "this is not valid toml :::").unwrap();

        match acquire_at(path.clone(), "takeover", DEFAULT_STALE_THRESHOLD_SECS) {
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

    /// 親に「ディレクトリではなく通常ファイル」を仕込むと、`create_dir_all` は
    /// silent fail、後続の `open()` が AlreadyExists 以外の I/O error を返し
    /// `Unavailable` 経路に入る、というシナリオを構築する。
    #[test]
    fn io_error_returns_unavailable() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file_as_dir = tmp.path().join("notadir");
        std::fs::write(&file_as_dir, "content").unwrap();
        let path = file_as_dir.join("pr-monitor.lock");
        match acquire_at(path, "test", DEFAULT_STALE_THRESHOLD_SECS) {
            LockResult::Unavailable { .. } => {}
            LockResult::Acquired(_) => panic!("expected Unavailable on I/O error, got Acquired"),
            LockResult::Busy { .. } => panic!("expected Unavailable on I/O error, got Busy"),
        }
    }

    #[test]
    fn past_time_from_parts_accepts_past() {
        let pt = PastTime::from_parts(100, 200).expect("then < now should succeed");
        assert_eq!(pt.age_secs(), 100);
    }

    #[test]
    fn past_time_from_parts_accepts_equal() {
        let pt = PastTime::from_parts(100, 100).expect("then == now should succeed");
        assert_eq!(pt.age_secs(), 0);
    }

    #[test]
    fn past_time_from_parts_rejects_future() {
        assert_eq!(
            PastTime::from_parts(200, 100),
            None,
            "then > now must be rejected (silent fresh bug 防止)"
        );
    }

    #[test]
    fn past_time_from_iso8601_now_rejects_far_future_year_9999() {
        assert_eq!(PastTime::from_iso8601_now("9999-01-01T00:00:00Z"), None);
    }

    #[test]
    fn past_time_from_iso8601_now_accepts_unix_epoch_origin() {
        let pt = PastTime::from_iso8601_now("1970-01-01T00:00:00Z").expect("epoch is past");
        assert!(pt.age_secs() >= 0);
    }
}

#[cfg(test)]
mod proptests {
    //! Bundle W (順位 34): proptest properties for `parse_iso8601` / `PastTime::from_parts`.
    //!
    //! 本 module は spec 層で AI が flaky 実装を書ける窓を塞ぐ regression net。
    //! 主要 property:
    //!   - P1: from_parts(then, now) で then <= now → age_secs == now - then
    //!   - P2: from_parts(then, now) で then > now → None (silent fresh 防止 / Finding D)
    //!   - P3: parse_iso8601 は任意 string 入力で panic しない
    //!   - P4: parse_iso8601 は pre-epoch year を必ず reject
    //!   - P5: parse_iso8601 は有効範囲内の date を必ず accept
    //!
    //! proptest case 数は default 256。実行時間は数百 ms 程度 (pre-push pipeline
    //! 完了基準 +1 秒以内に収まる)。

    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// P1: from_parts(then, now) で then <= now のとき age_secs == now - then が成立。
        /// `saturating_sub` 系の silent semantic mismatch (CR finding D) が混入したら
        /// このプロパティが落ちる regression net。
        #[test]
        fn past_time_age_is_correct_when_in_past(
            then in -1_000_000_000_000_i64..=1_000_000_000_000_i64,
            offset in 0_i64..=1_000_000_000_i64,
        ) {
            let now = then + offset;
            let pt = PastTime::from_parts(then, now).expect("then <= now");
            prop_assert_eq!(pt.age_secs(), offset);
        }

        /// P2: from_parts(then, now) で then > now のとき必ず None。
        /// Finding D を直接 encode: future timestamp が fresh 値を生むことは構造的に不可能。
        #[test]
        fn past_time_rejects_future(
            now in -1_000_000_000_i64..=1_000_000_000_i64,
            future_offset in 1_i64..=1_000_000_i64,
        ) {
            let then = now + future_offset;
            prop_assert_eq!(PastTime::from_parts(then, now), None);
        }

        /// P3: parse_iso8601 は任意 string で panic しない (corrupt input は None)。
        /// 過去に `days_from_epoch` の index out-of-bounds panic が発生した
        /// regression: range check が抜けると proptest がこれを再検出する。
        #[test]
        fn parse_iso8601_never_panics(s in ".*") {
            let _ = parse_iso8601(&s);
        }

        /// P4: pre-epoch year (< 1970) は必ず reject。
        #[test]
        fn parse_iso8601_rejects_pre_epoch_year(
            year in 0_u32..1970,
            month in 1_u32..=12,
            day in 1_u32..=28,
        ) {
            let s = format!("{:04}-{:02}-{:02}T00:00:00Z", year, month, day);
            prop_assert_eq!(parse_iso8601(&s), None);
        }

        /// P5: 有効範囲内の正規 ISO 8601 は必ず accept (round-trip 基本性質)。
        /// day を 1..=28 に絞ることで全月で有効な日付に限定 (うるう年判定を回避)。
        #[test]
        fn parse_iso8601_accepts_well_formed(
            year in 1970_u32..=9999,
            month in 1_u32..=12,
            day in 1_u32..=28,
            hour in 0_u32..=23,
            minute in 0_u32..=59,
            second in 0_u32..=59,
        ) {
            let s = format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                year, month, day, hour, minute, second
            );
            prop_assert!(parse_iso8601(&s).is_some(), "should accept: {}", s);
        }
    }
}
