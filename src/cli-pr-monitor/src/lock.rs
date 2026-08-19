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
//!   - stale takeover: **実行権を 1 プロセスに絞ってから atomic に置換する**
//!     (順位 292)。素朴な上書きだと、同じ stale lock を読んだ全プロセスが取得に
//!     成功する (8 スレッドで 8/8 取得を実測)。
//!   - Drop: **自分が書いた lock だけ**を削除する (token 一致確認、順位 292)。
//!     プロセス crash 時は file が残るが、stale 判定で `stale_threshold_secs`
//!     経過後に次インスタンスが takeover できる。
//!
//! ## pid の生存確認を入れない理由 (順位 385、2026-08-20 判断)
//!
//! stale 判定は経過時間のみで、lock に記録された pid が生きているかは見ない。
//! したがって holder が crash / kill されると、次のインスタンスが takeover
//! できるまで最大 `stale_threshold_secs` (30 分) かかる。2026-08-08 の #364 監視で
//! 実際に踏んでいる (終了済み pid の lock が残り、以降の監視呼び出しが skip された)。
//!
//! **それでも liveness check は入れない。** 判断の根拠:
//!
//! 1. **影響範囲が狭い** — 遅延するのは interactive セッション中の監視だけ。
//!    無人経路 (GitHub Actions の pr-monitor workflow) は別プロセス・別マシンで
//!    動くため、この lock の影響を受けない。
//! 2. **入れた場合の失敗が現状より悪い** — pid 生存確認は pid 再利用で誤判定する。
//!    誤って「死んでいる」と判定すれば **fresh な lock を takeover** し、監視が
//!    同時に 2 本走って Claude Code Max のレートリミットを浪費する
//!    (本 lock が防ぐべき当のもの)。回避には pid だけでなくプロセス起動時刻の照合が
//!    要り、その取得は Windows / Linux で実装が分かれる。**復帰窓 30 分**という
//!    上限つきの遅延と引き換えにするには釣り合わない。
//! 3. **本 lock は助言層** — 取得できなければ監視を skip するだけで、ゲートではない
//!    (ADR-043 の「助言層は fail-open が正しい」)。復帰が遅れても状態は壊れない。
//! 4. 順位 301 が「`cli-pr-monitor/lock.rs` は設計判断済みのため scope 除外」と
//!    している既存判断とも整合する。
//!
//! **再検討の条件**: 無人経路がこの lock に依存するようになった場合、または
//! 復帰待ちが実運用で 30 分では収まらない形 (例: threshold の延長) に変わった場合。
//!
//! `--mark-notified` は one-shot mutation のため guard 対象外。
//! single-iteration check + takt を回す `start_monitoring` のみ guard する。

use lib_jj_helpers::pipeline_lock::{acquire_takeover_gate, replace_file_atomically, TakeoverGate};
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
    /// 取得インスタンスを一意識別するランダムトークン (順位 292)。
    ///
    /// `serde(default)` は **本フィールド導入前に書かれた lock ファイル**のため。
    /// 無いと parse に失敗し、旧 lock が「破損 = stale」扱いで即 takeover されて
    /// しまう (fresh な旧 lock を踏み越えて同時監視が起きる)。既定の空文字は
    /// どの新規 token とも一致しないので、Drop の所有権確認は安全側に働く。
    #[serde(default)]
    token: String,
    pid: u32,
    start_time: String,
    mode: String,
}

/// Lock 取得成功時に保持する RAII guard。Drop で **自分が書いた** lock file のみ削除する。
pub(crate) struct MonitorLock {
    path: PathBuf,
    /// 取得インスタンスを一意識別するトークン。Drop の所有権確認に使う。
    token: String,
}

impl Drop for MonitorLock {
    /// **所有権確認付き削除** (順位 292): lock ファイルの token が自分のものと
    /// 一致した場合のみ削除する。
    ///
    /// 無条件削除だと、stale takeover 後 (別プロセス B が同じパスへ B の lock を
    /// 書いた後) に旧プロセス A の Drop が **B の lock を消す**。B は自分が lock を
    /// 持っていると思ったまま走り続け、その間に C が取得して同時監視になる。
    /// `lib_jj_helpers::pipeline_lock` が PR #271 で塞いだのと同型のバグで、
    /// 同じ token 方式に揃える。
    ///
    /// pid / start_time ではなく token を照合するのは pid 再利用による誤一致を
    /// 避けるため。残余 TOCTOU (read → remove 間の takeover) は fresh な lock が
    /// takeover されない (stale threshold 到達が takeover の必要条件) ことから
    /// 実用上起きない — pipeline_lock の Drop と同じ論拠。
    fn drop(&mut self) {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                if !self.owns(&content) {
                    log_info(
                        "[lock] cleanup skip: lock は既に別インスタンスへ takeover 済み",
                    );
                    return;
                }
                if let Err(e) = std::fs::remove_file(&self.path) {
                    // already removed (race) なら無視。それ以外は warn。
                    if e.kind() != std::io::ErrorKind::NotFound {
                        log_info(&format!("[lock] cleanup 失敗: {}", e));
                    }
                }
            }
            // 既に消えている (race) なら何もしない。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log_info(&format!("[lock] cleanup 時の read 失敗: {}", e)),
        }
    }
}

impl MonitorLock {
    /// lock ファイルの内容が自分のものか (token 一致) を判定する。
    ///
    /// parse できない内容は「自分のものではない」に倒す。書き込み途中の空ファイルや
    /// 破損 lock を自分のものとみなして消すと、無条件削除に戻ってしまう。
    fn owns(&self, content: &str) -> bool {
        match toml::from_str::<LockFile>(content) {
            Ok(lock) => !lock.token.is_empty() && lock.token == self.token,
            Err(_) => false,
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

    let token = lib_jj_helpers::pipeline_lock::generate_lock_token();
    let content = match build_lock_content(&token, mode) {
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
            LockResult::Acquired(MonitorLock { path, token })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // fresh な lock が存在する間は後続セッションを skip してレートリミット浪費を防ぐ
            if let Some((holder, age_secs)) = read_fresh_lock(&path, stale_threshold_secs) {
                return LockResult::Busy {
                    holder_pid: holder.pid,
                    holder_age_secs: age_secs,
                };
            }
            takeover_stale_lock(path, token, content, stale_threshold_secs)
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

/// stale と判定した lock を **1 プロセスだけが** 置き換える。
///
/// **素朴な上書きでは排他にならない。** 旧実装は stale 判定後に `std::fs::write` で
/// 上書きし、コメントは「複数 takeover が同時に成功しても無害」としていたが、本 lock の
/// 目的は「1 リポジトリ 1 アクティブ監視」なので同時取得はレートリミット浪費に直結する。
/// 実測では **8 スレッド中 8 つが `Acquired`** になった (順位 292、CodeRabbit #430 指摘)。
///
/// 実行権の選出は `lib_jj_helpers::pipeline_lock::acquire_takeover_gate` に委譲する。
/// sentinel + 孤立時の reclaim gate まで含む選出ロジックは、8 スレッド高競合の実測で
/// 2 `Acquired` を潰しながら組み上げたもの (PR #342)。**同じ問題を解く実装を 2 箇所に
/// 持たない** — 片方だけが後の修正を取り込めないまま残るのが最悪の形なので、共有する。
///
/// 実行権を得た後にやることは 2 つ:
/// 1. **再読込して、まだ stale か確かめる。** gate 待ちの間に別プロセスが lock を確立して
///    いれば奪わない (fresh を踏み潰すと同時監視に戻る)。
/// 2. **rename で atomic に置換する。** remove + create_new にすると path が一瞬不在に
///    なり、その窓で他スレッドの fast-path `create_new` が成功して 2 本とも取得できる。
fn takeover_stale_lock(
    path: PathBuf,
    token: String,
    content: String,
    stale_threshold_secs: i64,
) -> LockResult {
    let Some(now_unix) = current_unix_secs() else {
        return LockResult::Unavailable {
            reason: "system clock を取得できず takeover 判定不能".to_string(),
        };
    };
    match acquire_takeover_gate(&path, now_unix) {
        TakeoverGate::Acquired(gate) => {
            if let Some((holder, age_secs)) = read_fresh_lock(&path, stale_threshold_secs) {
                log_info(
                    "[lock] takeover 実行権の取得中に別インスタンスが lock を確立したため奪いません",
                );
                return LockResult::Busy {
                    holder_pid: holder.pid,
                    holder_age_secs: age_secs,
                };
            }
            let result = match replace_file_atomically(&path, &token, &content) {
                Ok(()) => LockResult::Acquired(MonitorLock { path, token }),
                Err(e) => LockResult::Unavailable {
                    reason: format!("takeover の atomic 置換に失敗: {}", e),
                },
            };
            // 置換が終わるまで実行権を保持する (早く手放すと 2 本目の takeover が生まれる)。
            drop(gate);
            result
        }
        TakeoverGate::Busy => {
            log_info("[lock] 別インスタンスが takeover 実行中のため skip します");
            let (holder_pid, holder_age_secs) = read_fresh_lock(&path, stale_threshold_secs)
                .map(|(holder, age)| (holder.pid, age))
                .unwrap_or((UNKNOWN_HOLDER_PID, 0));
            LockResult::Busy {
                holder_pid,
                holder_age_secs,
            }
        }
        TakeoverGate::Unavailable(reason) => LockResult::Unavailable { reason },
    }
}

fn build_lock_content(token: &str, mode: &str) -> Option<String> {
    let lock = LockFile {
        token: token.to_string(),
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
            token: String::new(),
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

// test module は別ファイルへ分離している (本体 800 行ガイドライン、順位 147)。
// 分割方式は stages/poll/rate_limit.rs と同じ `#[path]` 方式に揃えた。
#[cfg(test)]
#[path = "lock/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lock/proptests.rs"]
mod proptests;
