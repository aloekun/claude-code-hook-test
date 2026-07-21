//! Pipeline lock — 実行中 pipeline (merge/push) と Stop hook 品質ゲートの相互排他 (順位 280)。
//!
//! PR #267 のマージで「background の merge pipeline がローカル同期の checkout 実行中に、
//! ターン終了で発火した Stop hook 品質ゲート (cargo/jj) が同じ working copy 上で競合」し、
//! jj が Concurrent checkout で中断する事故が実発生した (ADR-045 § Known operational risks)。
//! 本モジュールは pipeline 実行区間で lock ファイルを保持し、hooks-stop-quality が
//! fresh な lock を検知したら品質ゲートを skip する (fail-open) ための基盤を提供する。
//!
//! 設計は `cli-pr-monitor/src/lock.rs` の実績パターンを踏襲:
//! - `OpenOptions::create_new` による atomic create (read-then-write TOCTOU の排除)
//! - age ベースの stale 判定 + takeover (クラッシュした pipeline の lock が永続しない)
//! - RAII guard (Drop で削除)
//!
//! **相互排他の要件が姉妹 lock と異なる**: `cli-pr-monitor/src/lock.rs` は「複数 takeover が
//! 同時成功しても無害」(last-write-wins) を許容するが、本 lock は pipeline の二重起動を
//! 防ぐため **stale takeover でちょうど 1 プロセスのみ `Acquired`** を要件とする。この単一
//! winner 性は `<lock>.takeover` sentinel の `create_new` で「path を除去し得るスレッド」を
//! 1 つに限定して担保する (`takeover_stale_lock` の doc に詳細)。
//!
//! 相違点: timestamp は ISO8601 ではなく unix epoch 秒を直接記録する (parser 不要)。
//! future timestamp は stale 扱い (破損 lock が永続 fresh 化する bug class の再発防止、
//! lock.rs の PastTime と同じ invariant)。
//!
//! ファイル形式は `key=value` 行 (pid / start_unix / label)。外部 config ではなく
//! 内部の一時ファイルのため、依存追加 (serde/toml) を避けた最小形式とする。
//!
//! sentinel 自身も age ベースで自己修復する (SIM-NEW-pipeline_lock-L157): sentinel
//! 保持者が `perform_takeover` 実行中にクラッシュすると sentinel が孤立し、以降
//! 本物の stale lock があっても永久に `Busy` へ倒れていた。`classify_lock_content`
//! を流用して stale (`SENTINEL_STALE_SECS` 経過) と判定できた sentinel は、その
//! content 固有の reclaim gate (`reclaim_gate_path`) 経由で回収する。単純な
//! 「stale 判定 → 無条件 remove」は、判定から除去までの間隙で他スレッドが正当に
//! 再確立した sentinel を巻き添えで消しうる (8 スレッド高競合の実測で 2 `Acquired`
//! が再現した regression) ため不採用とした。詳細は `reclaim_stale_sentinel` の doc。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// lock ファイル名 (`.claude/` 配下、gitignore 対象、checkout ごとに独立)。
pub const PIPELINE_LOCK_FILENAME: &str = "pipeline.lock";

/// stale 判定 threshold。pipeline の実測最長 (push ~15 分) の 2x で安全マージン。
pub const PIPELINE_LOCK_STALE_SECS: i64 = 1800;

/// lock 取得成功時に保持する RAII guard。Drop で **自分が書いた** lock ファイルのみ削除する。
pub struct PipelineLock {
    path: PathBuf,
    /// 取得インスタンスを一意識別するランダムトークン (PR #271 CodeRabbit Major 対応)。
    token: String,
}

impl Drop for PipelineLock {
    /// **所有権確認付き削除**: lock ファイルの token が自分のものと一致した場合のみ削除する。
    ///
    /// 無条件削除だと、stale takeover 後 (別プロセス B が同じパスに B の lock を書いた後) に
    /// 旧プロセス A の Drop が **B の lock を消してしまう** (CodeRabbit Major、典型的な
    /// stale-lock-takeover + unconditional-unlock 問題)。token 一致確認で「他人の lock を
    /// 消さない」ことを保証する。
    ///
    /// 残余 TOCTOU (read → remove 間の takeover): fresh な lock は takeover されない
    /// (stale threshold 到達が takeover の必要条件) ため、自分の token を read できた時点で
    /// 他プロセスは未 takeover。よって「read で自分の token → その直後に他プロセスが
    /// takeover」は fresh 中は起きず、実用上安全。pid/start_unix ではなく token を使うのは
    /// PID 再利用による誤一致を避けるため。
    fn drop(&mut self) {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                if parse_field(&content, "token") == Some(self.token.as_str()) {
                    if let Err(e) = std::fs::remove_file(&self.path) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            eprintln!("[pipeline-lock] cleanup 失敗: {}", e);
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("[pipeline-lock] cleanup 時の read 失敗: {}", e),
        }
    }
}

/// lock 取得結果。Busy / Unavailable でも pipeline 自体は継続してよい
/// (lock は Stop hook への advisory であり、pipeline の実行可否を左右しない)。
pub enum PipelineLockResult {
    Acquired(PipelineLock),
    Busy { holder_pid: u32, holder_age_secs: i64 },
    Unavailable { reason: String },
}

/// `claude_dir` (通常 `<repo>/.claude`) に pipeline lock を取得する。
pub fn acquire_pipeline_lock(claude_dir: &Path, label: &str) -> PipelineLockResult {
    acquire_pipeline_lock_at(
        claude_dir.join(PIPELINE_LOCK_FILENAME),
        label,
        PIPELINE_LOCK_STALE_SECS,
        current_unix_secs(),
    )
}

/// テスト用: path / threshold / now を引数化。
pub fn acquire_pipeline_lock_at(
    path: PathBuf,
    label: &str,
    stale_threshold_secs: i64,
    now_unix: i64,
) -> PipelineLockResult {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let token = generate_token();
    let content = build_lock_content(&token, std::process::id(), now_unix, label);

    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                eprintln!("[pipeline-lock] 書き込み失敗 (継続): {}", e);
            }
            PipelineLockResult::Acquired(PipelineLock { path, token })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                match classify_lock_content(&raw, stale_threshold_secs, now_unix) {
                    LockState::Fresh(pid, age_secs) => {
                        return PipelineLockResult::Busy {
                            holder_pid: pid,
                            holder_age_secs: age_secs,
                        };
                    }
                    LockState::Held => {
                        return PipelineLockResult::Busy {
                            holder_pid: 0,
                            holder_age_secs: 0,
                        };
                    }
                    LockState::Stale => {}
                }
            }
            takeover_stale_lock(path, token, content, stale_threshold_secs, now_unix)
        }
        Err(e) => PipelineLockResult::Unavailable {
            reason: e.to_string(),
        },
    }
}

/// stale と判定した lock を takeover する。ちょうど 1 プロセスのみ `Acquired` になる。
///
/// **単一 takeover 権を sentinel の `create_new` で選出する**。`<lock>.takeover` を
/// atomic な `create_new` で作れたスレッドだけが takeover 権限を持ち、実際の
/// 「stale 判定 → 除去 → fresh 作成」を行う (`perform_takeover`)。sentinel 取得に
/// 負けたスレッドは `Busy` に倒す (権限保持者が fresh lock を設置するため)。
///
/// sentinel は「takeover を試みるスレッド」を 1 つに絞る第 1 段の直列化。実際の単一 winner
/// 性は sentinel 保持者が [`perform_takeover`] で **rename による atomic 置換**を使い、path を
/// 一度も不在にしないことで担保する (詳細はそちらの doc)。sentinel だけでは不十分だった経緯
/// (remove + create_new の不在窓に親 fast-path が割り込む) も同 doc に記録。
/// SIM-NEW-pipeline_lock-L146。
///
/// sentinel 自体の取得は [`acquire_takeover_sentinel`] に委譲する (age ベースの
/// 自己修復込み、SIM-NEW-pipeline_lock-L157)。
fn takeover_stale_lock(
    path: PathBuf,
    token: String,
    content: String,
    stale_threshold_secs: i64,
    now_unix: i64,
) -> PipelineLockResult {
    let sentinel = takeover_sentinel_path(&path);
    match acquire_takeover_sentinel(&sentinel, now_unix) {
        SentinelGate::Acquired => {
            let result =
                perform_takeover(&path, token, &content, stale_threshold_secs, now_unix);
            if let Err(e) = std::fs::remove_file(&sentinel) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[pipeline-lock] takeover sentinel の除去に失敗 (継続): {}", e);
                }
            }
            result
        }
        SentinelGate::Busy => busy_from_disk(&path, stale_threshold_secs, now_unix),
        SentinelGate::Unavailable(reason) => PipelineLockResult::Unavailable { reason },
    }
}

/// sentinel の stale 判定 threshold。sentinel は `perform_takeover` 一回分の実行区間
/// (read/write/rename 数回、通常ミリ秒オーダー) だけ保持される想定であり、pipeline
/// 本体の `PIPELINE_LOCK_STALE_SECS` (1800s) よりずっと短くてよい。sentinel 保持者が
/// takeover 中にクラッシュしても、この秒数が経てば次の acquire が自己修復する
/// (SIM-NEW-pipeline_lock-L157: 従来は age 判定が皆無で永久 wedge していた)。
const SENTINEL_STALE_SECS: i64 = 30;

/// [`takeover_stale_lock`] 用 sentinel 取得結果。
enum SentinelGate {
    /// sentinel を確保した (自分が takeover 実行権を持つ)。
    Acquired,
    /// 別プロセスが sentinel を保持中 (fresh、または create_new 直後の書き込み待ち)。
    Busy,
    /// I/O エラーで判定不能。
    Unavailable(String),
}

/// `<lock>.takeover` sentinel の取得を試みる。通常は `create_new` の atomic 排他により
/// 1 プロセスのみ成功する。既存 sentinel が stale と判定できた場合に限り、
/// [`reclaim_stale_sentinel`] へ委譲して回収を試みる (SIM-NEW-pipeline_lock-L157)。
fn acquire_takeover_sentinel(sentinel: &Path, now_unix: i64) -> SentinelGate {
    match try_create_sentinel(sentinel, now_unix) {
        Ok(()) => return SentinelGate::Acquired,
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
            return SentinelGate::Unavailable(e.to_string());
        }
        Err(_) => {}
    }

    let Ok(raw) = std::fs::read_to_string(sentinel) else {
        return SentinelGate::Busy;
    };
    match classify_lock_content(&raw, SENTINEL_STALE_SECS, now_unix) {
        LockState::Held | LockState::Fresh(..) => SentinelGate::Busy,
        LockState::Stale => reclaim_stale_sentinel(sentinel, &raw, now_unix),
    }
}

/// sentinel に `pid=` / `start_unix=` を書き込む。`classify_lock_content` (メイン lock と
/// 共通ロジック) で stale 判定できるよう、フィールド名をメイン lock の形式と揃える。
fn try_create_sentinel(sentinel: &Path, now_unix: i64) -> std::io::Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sentinel)?;
    f.write_all(format!("pid={}\nstart_unix={}\n", std::process::id(), now_unix).as_bytes())
}

/// stale と判定した sentinel を、その content 固有の reclaim gate 経由で回収する。
///
/// **単純な「stale 判定 → 無条件 remove」は不採用**: 判定から除去までの間隙で別スレッドが
/// 正当に再確立した sentinel を巻き添えで消しうる (8 スレッド高競合の実測で 2 `Acquired`
/// が再現した regression、SIM-NEW-pipeline_lock-L157 修正の初版で発見)。
///
/// 代わりに、reclaim gate の path を **読んだ content から決定論的に導出**する
/// (`reclaim_gate_path`)。同じ stale content を読んだスレッド同士だけがその 1 つの
/// `create_new` で競い、勝者だけが実際の「除去 → 再作成」(`finish_reclaim`) を行う。
/// 負けたスレッドは「対象はまだ同じ stale content のまま (勝者が処理中)」と分かっている
/// ため、sentinel 自体には一切触れず安全に `Busy` へ倒せる。
fn reclaim_stale_sentinel(sentinel: &Path, stale_content: &str, now_unix: i64) -> SentinelGate {
    let reclaim = reclaim_gate_path(sentinel, stale_content);
    match try_create_reclaim_marker(&reclaim, now_unix) {
        Ok(()) => finish_reclaim(sentinel, &reclaim, now_unix),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            reap_orphaned_reclaim_marker(&reclaim, now_unix);
            SentinelGate::Busy
        }
        Err(e) => SentinelGate::Unavailable(e.to_string()),
    }
}

/// reclaim gate の勝者だけが呼ぶ: sentinel を除去して再作成する。
///
/// この関数を呼べるのは「この stale content 専用の reclaim gate」に勝った 1 スレッドのみ
/// (同じ content を読んだ他スレッドは gate 負けで `Busy` に倒れ、sentinel に触れない)。
/// 唯一の例外は「この stale content をまだ読んでいない、独立した新規取得試行」が
/// 除去直後の空隙で fast path の `create_new` に成功するケースで、その場合は素直に
/// `Busy` へ倒れる (単一 winner 性は保たれる)。
fn finish_reclaim(sentinel: &Path, reclaim: &Path, now_unix: i64) -> SentinelGate {
    let _ = std::fs::remove_file(sentinel);
    let result = match try_create_sentinel(sentinel, now_unix) {
        Ok(()) => SentinelGate::Acquired,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => SentinelGate::Busy,
        Err(e) => SentinelGate::Unavailable(e.to_string()),
    };
    let _ = std::fs::remove_file(reclaim);
    result
}

/// reclaim gate 自体に `pid=` / `start_unix=` を書き込む (sentinel と同形式、
/// `classify_lock_content` を再利用するため)。
fn try_create_reclaim_marker(reclaim: &Path, now_unix: i64) -> std::io::Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(reclaim)?;
    f.write_all(format!("pid={}\nstart_unix={}\n", std::process::id(), now_unix).as_bytes())
}

/// reclaim gate 保持者が `finish_reclaim` の 2 手 (remove + create_new) の間でクラッシュし、
/// gate 自体が孤立した場合の回収。
///
/// この gate は特定の stale content 専用 (path が content 由来のため) で、除去後に
/// **別の正当な世代が同じ path に再作成される余地が無い**。よって sentinel と違い、
/// stale 判定さえできれば無条件 remove で安全に回収できる (複数スレッドが同時に
/// 回収を試みても、`remove_file` 自体の排他性により実害は出ない)。
fn reap_orphaned_reclaim_marker(reclaim: &Path, now_unix: i64) {
    if let Ok(raw) = std::fs::read_to_string(reclaim) {
        if matches!(
            classify_lock_content(&raw, SENTINEL_STALE_SECS, now_unix),
            LockState::Stale
        ) {
            let _ = std::fs::remove_file(reclaim);
        }
    }
}

/// sentinel content から決定論的に reclaim gate の path を導出する。
/// 同じ (sentinel path, content) を読んだスレッドは必ず同じ path に collide する
/// (`DefaultHasher` は固定キーで決定論的、`RandomState` と異なる)。
fn reclaim_gate_path(sentinel: &Path, content: &str) -> PathBuf {
    suffixed_path(sentinel, &format!(".reclaim.{:016x}", content_fingerprint(content)))
}

fn content_fingerprint(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// sentinel 保持者だけが呼ぶ takeover 本体。stale なら新 lock を **atomic な rename で置換**し、
/// `Acquired` を返す。fresh / 書き込み中 (`Held`) なら奪わず `Busy`。
///
/// **rename で置換する (remove + create_new にしない)** のが単一 winner 性の要:
/// remove + create_new は remove と create の間で **path が一瞬不在になる窓**を作り、その窓で
/// 別スレッドの親 fast-path `create_new(path)` が成功して 2 スレッドとも `Acquired` になる
/// (Linux 高競合で実測: `TAKEOVER_REMOVE → TAKEOVER_CREATE_OK → PARENT_CREATE_OK`)。
/// temp に全内容を書いてから `rename(temp → path)` すれば、path は旧 stale 内容から新内容へ
/// **原子的に切り替わり一度も不在にならない**ため、親の `create_new` は常に `AlreadyExists` に
/// なり fast-path で割り込めない。同時に「読み手が空 / 部分書き込みを観測する」窓も消える
/// (rename は完成済みファイルを一括で差し込む)。SIM-NEW-pipeline_lock-L146。
fn perform_takeover(
    path: &Path,
    token: String,
    content: &str,
    stale_threshold_secs: i64,
    now_unix: i64,
) -> PipelineLockResult {
    if let Ok(raw) = std::fs::read_to_string(path) {
        match classify_lock_content(&raw, stale_threshold_secs, now_unix) {
            LockState::Fresh(pid, age_secs) => {
                return PipelineLockResult::Busy {
                    holder_pid: pid,
                    holder_age_secs: age_secs,
                };
            }
            LockState::Held => {
                return PipelineLockResult::Busy {
                    holder_pid: 0,
                    holder_age_secs: 0,
                };
            }
            LockState::Stale => {}
        }
    }
    replace_lock_atomically(path, token, content)
}

/// 新 lock 内容を temp ファイルへ書き、`rename` で `path` に atomic 置換する。
/// sentinel 保持中のみ呼ばれるため temp パスの競合は起きない。
fn replace_lock_atomically(path: &Path, token: String, content: &str) -> PipelineLockResult {
    let tmp = takeover_tmp_path(path, &token);
    if let Err(e) = std::fs::write(&tmp, content) {
        return PipelineLockResult::Unavailable {
            reason: format!("takeover temp 書き込み失敗: {}", e),
        };
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => PipelineLockResult::Acquired(PipelineLock {
            path: path.to_path_buf(),
            token,
        }),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            PipelineLockResult::Unavailable {
                reason: format!("takeover rename 失敗: {}", e),
            }
        }
    }
}

/// takeover 権選出用の sentinel パス (`<lock>.takeover`)。元 lock と同一ディレクトリ。
fn takeover_sentinel_path(path: &Path) -> PathBuf {
    suffixed_path(path, ".takeover")
}

/// atomic 置換用の temp パス (`<lock>.new.<token>`)。`rename` の atomic 性を保つため
/// 元 lock と同一ディレクトリに置く。
fn takeover_tmp_path(path: &Path, token: &str) -> PathBuf {
    suffixed_path(path, &format!(".new.{token}"))
}

fn suffixed_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// takeover レースに負けた際、ディスク上の現在の holder 情報から `Busy` を組み立てる。
/// 直後に holder が drop 済みで読めない場合は `holder_pid: 0` で `Busy` を返す
/// (相手が既に確保していた事実は変わらないため `Acquired` にはしない)。
fn busy_from_disk(path: &Path, stale_threshold_secs: i64, now_unix: i64) -> PipelineLockResult {
    match read_fresh_lock(path, stale_threshold_secs, now_unix) {
        Some((pid, age_secs)) => PipelineLockResult::Busy {
            holder_pid: pid,
            holder_age_secs: age_secs,
        },
        None => PipelineLockResult::Busy {
            holder_pid: 0,
            holder_age_secs: 0,
        },
    }
}

/// 取得インスタンスを一意識別する 128bit ランダムトークン (hex)。
///
/// `uuid` crate を追加せず std のみで生成する (本 crate は依存ゼロ方針)。
/// `RandomState` は生成ごとに OS エントロピー由来のハッシュキーで初期化されるため、
/// 空状態の `finish()` は毎回異なる値を返す。2 つ連結して 128bit の識別子とする。
/// 暗号用途ではなく「lock インスタンスの衝突しない識別」が目的。
fn generate_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let a = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    let b = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!("{a:016x}{b:016x}")
}

/// fresh な pipeline lock が存在するか (Stop hook 用の読み取り専用チェック)。
/// 戻り値は `Some((holder_pid, age_secs))`。lock 不在 / stale / parse 不能は `None`。
pub fn pipeline_lock_holder(claude_dir: &Path) -> Option<(u32, i64)> {
    read_fresh_lock(
        &claude_dir.join(PIPELINE_LOCK_FILENAME),
        PIPELINE_LOCK_STALE_SECS,
        current_unix_secs(),
    )
}

fn build_lock_content(token: &str, pid: u32, start_unix: i64, label: &str) -> String {
    format!(
        "token={}\npid={}\nstart_unix={}\nlabel={}\n",
        token,
        pid,
        start_unix,
        label.replace(['\r', '\n'], " ")
    )
}

/// 既存 lock が fresh なら `Some((pid, age_secs))`。
///
/// stale 条件 (いずれかで None = takeover 可):
/// - parse 失敗 (破損)
/// - age >= threshold (クラッシュした pipeline の残骸)
/// - start_unix が未来 (破損 future-dated lock の永続 fresh 化防止)
fn read_fresh_lock(path: &Path, stale_threshold_secs: i64, now_unix: i64) -> Option<(u32, i64)> {
    let content = std::fs::read_to_string(path).ok()?;
    is_fresh_content(&content, stale_threshold_secs, now_unix)
}

/// lock ファイル content の 3 値判定。
///
/// **`Empty` を `Stale` と別扱いするのが要点**: `create_new` は atomic だがその直後の
/// `write_all` までにファイルは**空**で存在する。この窓の空 content を stale と誤判定して
/// takeover (= remove) すると、`create_new` に成功して自分を holder と見なした別スレッドの
/// lock を破壊し、2 スレッドとも `Acquired` になる (8 スレッド高競合の Linux 実測で顕在化。
/// 姉妹 `cli-pr-monitor/src/lock.rs` が WP-15 で塞いだのと同じ bug class)。空は「書き込み中の
/// 保持者あり」= `Held` に倒す。
enum LockState {
    /// 有効期限内の holder が居る (pid, age)。
    Fresh(u32, i64),
    /// `create_new` 直後・`write_all` 前の空ファイル。保持者が書き込み中とみなす。
    Held,
    /// 破損 / 期限切れ / 未来日付。takeover 可。
    Stale,
}

fn classify_lock_content(content: &str, stale_threshold_secs: i64, now_unix: i64) -> LockState {
    if content.trim().is_empty() {
        return LockState::Held;
    }
    match is_fresh_content(content, stale_threshold_secs, now_unix) {
        Some((pid, age_secs)) => LockState::Fresh(pid, age_secs),
        None => LockState::Stale,
    }
}

/// `read_fresh_lock` の判定ロジック本体。生 content を直接受け取るため、呼び出し元が
/// content を再利用 (takeover 直前のスナップショット比較等) できる。
///
/// **空 content は `None` (= 非 fresh)** を返す点に注意: 「保持者が居るか」を厳密に問う
/// 読み取り専用チェック (`pipeline_lock_holder`) では、書き込み中の空 lock を「holder あり」と
/// 報告する意味がない (pid 不明)。空を Held として扱うのは takeover 判定側
/// (`classify_lock_content`) の責務。
fn is_fresh_content(content: &str, stale_threshold_secs: i64, now_unix: i64) -> Option<(u32, i64)> {
    let pid: u32 = parse_field(content, "pid")?.parse().ok()?;
    let start_unix: i64 = parse_field(content, "start_unix")?.parse().ok()?;
    if start_unix > now_unix {
        return None;
    }
    let age_secs = now_unix - start_unix;
    if age_secs < stale_threshold_secs {
        Some((pid, age_secs))
    } else {
        None
    }
}

fn parse_field<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    content
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .map(str::trim)
}

/// pipeline 実行区間で lock を保持する便宜関数 (merge-pipeline / push-runner 用)。
///
/// lock は Stop hook への advisory であり pipeline の実行可否を左右しないため、
/// Busy / Unavailable / exe dir 解決失敗はいずれも警告ログのみで `None` を返し、
/// pipeline は lock なしで継続する。戻り値の guard を pipeline 終了まで保持すること。
pub fn hold_pipeline_lock(label: &str, log: fn(&str)) -> Option<PipelineLock> {
    let Some(dir) = exe_claude_dir() else {
        log("[pipeline-lock] exe dir 解決失敗 (lock なしで継続)");
        return None;
    };
    match acquire_pipeline_lock(&dir, label) {
        PipelineLockResult::Acquired(lock) => Some(lock),
        PipelineLockResult::Busy {
            holder_pid,
            holder_age_secs,
        } => {
            log(&format!(
                "[pipeline-lock] 別 pipeline が実行中 (pid={}, age={}s) — lock なしで継続 (advisory)",
                holder_pid, holder_age_secs
            ));
            None
        }
        PipelineLockResult::Unavailable { reason } => {
            log(&format!("[pipeline-lock] 取得不可 (継続): {}", reason));
            None
        }
    }
}

/// 実行中 exe の親ディレクトリ (= `.claude/`) を返す。
///
/// pipeline exe / hook exe はいずれも `.claude/` 配下に配置される (ADR-010) ため、
/// lock ファイルの置き場所を exe-relative で解決する (cwd 非依存 = 順位 287 の規約)。
pub fn exe_claude_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "pipeline_lock/tests.rs"]
mod tests;
