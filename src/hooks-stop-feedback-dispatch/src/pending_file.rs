//! post-merge-feedback pending file の読み取りと status 更新 (ADR-029)
//!
//! 共有スキーマ・定数・UTC ヘルパーは `lib-pending-file` に集約。
//! 本モジュールは dispatcher 固有の読み取りロジックと status 遷移を担う。
//!
//! dispatcher 固有の差分:
//!   - `ExistingPending` を `Pending` / `Dispatched` に分けて扱う
//!     (cli-merge-pipeline 側は両方 `Active(String)` にまとめているが、hook 側は分岐が必要)
//!   - status 遷移 (pending → dispatched) は `write_overwrite` (tmp → rename) を使う
//!   - stale TTL (24h) 判定のため `epoch_secs_to_iso8601` を内部公開
//!   - pending→dispatched の process-level 排他は `PendingLock` (RAII) で担保 (CodeRabbit PR #71)

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ─── Re-exports from lib-pending-file ───

pub(crate) use lib_pending_file::PendingFile;
pub(crate) use lib_pending_file::{
    epoch_secs_to_iso8601, is_valid_owner_repo, utc_now_epoch_secs, utc_now_iso8601, FILE_NAME,
    SCHEMA_VERSION, STATUS_CONSUMED, STATUS_DISPATCHED, STATUS_PENDING,
};

// ─── Dispatcher-local types ───

/// 既存 pending file の読み取り結果 (dispatcher 版)。
///
/// cli-merge-pipeline 側は `pending` / `dispatched` を `Active(String)` にまとめているが、
/// dispatcher は両者で挙動が異なる (pending → 発火、dispatched → 二重通知しない) ので
/// 別 variant にする。
#[derive(Debug)]
pub(crate) enum ExistingPending {
    /// ファイル不在。通常経路で silent exit。
    None,
    /// size 0 / parse 失敗 / schema_version 不一致 / 未知 status。削除 + silent exit。
    Corrupt(String),
    /// status = "pending"。additionalContext 発火 + dispatched へ遷移。
    Pending(PendingFile),
    /// status = "dispatched"。二重通知防止で silent exit。
    Dispatched,
    /// status = "consumed"。skill 完了後の残骸。削除 + silent exit。
    Consumed,
}

/// 既存 pending file の状態を判定する。
///
/// SEC-001: STATUS_PENDING 時に `is_valid_owner_repo` で `owner_repo` を検証する。
/// 不正値 (newline 等) は `Corrupt` として廃棄し `additionalContext` への注入を防ぐ。
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
        STATUS_PENDING => {
            if !is_valid_owner_repo(&pending.owner_repo) {
                return ExistingPending::Corrupt(format!(
                    "invalid owner_repo '{}'",
                    pending.owner_repo
                ));
            }
            ExistingPending::Pending(pending)
        }
        STATUS_DISPATCHED => ExistingPending::Dispatched,
        STATUS_CONSUMED => ExistingPending::Consumed,
        other => ExistingPending::Corrupt(format!("unknown status '{}'", other)),
    }
}

/// pending file を上書き書き込み (tmp → rename の 2 段階)。
///
/// status 更新 (pending → dispatched) で使用。呼び出し元は事前に read_existing で
/// ファイル存在を確認済みの前提。tmp 名は `{file_name}.tmp.{pid}` の一意形式。
pub(crate) fn write_overwrite(path: &Path, pending: &PendingFile) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(pending).map_err(|e| format!("serialize error: {}", e))?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pending".to_string());
    let tmp_name = format!("{}.tmp.{}", file_name, std::process::id());
    let tmp_path = path.with_file_name(tmp_name);
    if let Err(e) = std::fs::write(&tmp_path, &json) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("tmp write failed ({}): {}", tmp_path.display(), e));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "rename failed ({} → {}): {}",
            tmp_path.display(),
            path.display(),
            e
        ));
    }
    Ok(())
}

// ─── Process-level dispatch lock (CodeRabbit PR #71 Major fix) ───

/// 壊れた lock file を stale とみなす TTL (秒)。
///
/// dispatch 処理は ms オーダーで完了するため、60 秒は十分な猶予。これを超えた lock は
/// `kill -9` 等で Drop が走らなかった crash 残骸とみなし、削除後に 1 度だけ再取得を試す。
/// この閾値を無くすと、クラッシュした holder の残骸により以降すべての dispatch が
/// 永続的に stuck する可能性がある (defense-in-depth としては不十分)。
const LOCK_STALE_SECS: u64 = 60;

/// pending→dispatched 遷移を process-level で排他化する RAII lock。
///
/// ADR-029 の「ロックファイル不要」は `create_new` による pending file 自体の **新規作成
/// atomicity** についての記述であり、pending を **read して dispatched へ write** する
/// 遷移の同期とは別軸の問題。複数の Stop hook が同一の pending を並行処理できる以上、
/// read→emit→write の区間を process-level で排他化しないと additionalContext が重複発火する。
///
/// **排他方式**: `{pending}.lock` を `OpenOptions::create_new(true)` (O_EXCL 相当) で
/// 排他作成する。`write_new_exclusive` と同じ pattern。
///
/// **解放**: Drop で `remove_file`。panic / 早期 return でも RAII で安全に片付く。
///
/// **stale 回復**: `AlreadyExists` 時に既存 lock の mtime を確認し `LOCK_STALE_SECS` を
/// 超えていれば削除 → 1 回だけ再取得を試す (CodeRabbit 指摘の defense-in-depth)。
pub(crate) struct PendingLock {
    lock_path: PathBuf,
}

impl PendingLock {
    /// ロック取得を試みる。
    ///
    /// 戻り値:
    ///   - `Ok(Some(lock))`: 排他取得成功。`lock` を drop するまで他プロセスは取得不能
    ///   - `Ok(None)`: 他プロセスが保持中 (active) → dispatch しない
    ///   - `Err(...)`: I/O エラー (呼び出し側は stderr WARN + fail-open で継続)
    pub(crate) fn try_acquire(pending_path: &Path) -> Result<Option<Self>, String> {
        let lock_path = pending_path.with_extension("lock");

        match Self::try_create(&lock_path)? {
            Some(()) => return Ok(Some(Self { lock_path })),
            None => {}
        }

        // AlreadyExists: stale 判定で gc 試行
        if Self::is_stale(&lock_path) {
            let _ = std::fs::remove_file(&lock_path);
            // 1 度だけ再取得を試す (race で別プロセスが先に取っても None で諦める)
            return match Self::try_create(&lock_path)? {
                Some(()) => Ok(Some(Self { lock_path })),
                None => Ok(None),
            };
        }
        Ok(None)
    }

    /// 低レベル create_new。`Ok(Some(()))` = 取得、`Ok(None)` = AlreadyExists、`Err` = I/O エラー。
    fn try_create(lock_path: &Path) -> Result<Option<()>, String> {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut f) => {
                // observation 目的で PID と timestamp を記録 (best effort、失敗しても非致命)
                let _ = writeln!(f, "pid={} at={}", std::process::id(), utc_now_iso8601());
                Ok(Some(()))
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(None),
            Err(e) => Err(format!(
                "lock create_new 失敗 ({}): {}",
                lock_path.display(),
                e
            )),
        }
    }

    /// lock file が `LOCK_STALE_SECS` を超えて古ければ stale とみなす。
    /// mtime 取得不能 / 未来時刻 は stale 扱いしない (保守的)。
    fn is_stale(lock_path: &Path) -> bool {
        Self::is_stale_with_threshold(lock_path, LOCK_STALE_SECS)
    }

    /// テスト可能性のため閾値を引数化した下位関数。
    fn is_stale_with_threshold(lock_path: &Path, threshold_secs: u64) -> bool {
        let meta = match std::fs::metadata(lock_path) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let modified = match meta.modified() {
            Ok(t) => t,
            Err(_) => return false,
        };
        SystemTime::now()
            .duration_since(modified)
            .map(|d| d.as_secs() > threshold_secs)
            .unwrap_or(false)
    }
}

impl Drop for PendingLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
            producer: None,
        }
    }

    fn unique_tmp(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pending-dispatch-{}-{}-{}.json",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ))
    }

    fn write_raw(path: &std::path::Path, pending: &PendingFile) {
        let json = serde_json::to_string_pretty(pending).unwrap();
        std::fs::write(path, json).unwrap();
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
        std::fs::write(&path, "garbage").unwrap();
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
        write_raw(&path, &pending);
        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => assert!(reason.contains("schema_version")),
            other => panic!("expected Corrupt, got {:?}", other),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_corrupt_for_unknown_status() {
        let path = unique_tmp("bad-status");
        write_raw(&path, &sample_pending("garbage"));
        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => assert!(reason.contains("unknown status")),
            other => panic!("expected Corrupt, got {:?}", other),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_pending_with_payload() {
        let path = unique_tmp("pending-payload");
        write_raw(&path, &sample_pending(STATUS_PENDING));
        match read_existing(&path) {
            ExistingPending::Pending(p) => {
                assert_eq!(p.status, STATUS_PENDING);
                assert_eq!(p.pr_number, 123);
                assert_eq!(p.owner_repo, "aloekun/claude-code-hook-test");
            }
            other => panic!("expected Pending, got {:?}", other),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_corrupt_for_invalid_owner_repo() {
        let path = unique_tmp("owner-repo-injection");
        let mut pending = sample_pending(STATUS_PENDING);
        pending.owner_repo = "owner/repo\nmalicious".to_string();
        write_raw(&path, &pending);
        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => {
                assert!(reason.contains("invalid owner_repo"), "reason: {}", reason);
            }
            other => panic!("expected Corrupt for injected owner_repo, got {:?}", other),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_dispatched() {
        let path = unique_tmp("dispatched");
        write_raw(&path, &sample_pending(STATUS_DISPATCHED));
        assert!(matches!(read_existing(&path), ExistingPending::Dispatched));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_consumed() {
        let path = unique_tmp("consumed");
        write_raw(&path, &sample_pending(STATUS_CONSUMED));
        assert!(matches!(read_existing(&path), ExistingPending::Consumed));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_overwrite_updates_status() {
        let path = unique_tmp("overwrite");
        write_raw(&path, &sample_pending(STATUS_PENDING));

        let mut next = sample_pending(STATUS_DISPATCHED);
        next.dispatched_at = Some("2026-04-23T10:01:00Z".to_string());
        write_overwrite(&path, &next).unwrap();

        let loaded: PendingFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.status, STATUS_DISPATCHED);
        assert_eq!(
            loaded.dispatched_at.as_deref(),
            Some("2026-04-23T10:01:00Z")
        );

        // tmp 残骸がないこと
        let dir = path.parent().unwrap();
        let basename = path.file_name().unwrap().to_string_lossy().into_owned();
        let tmp_prefix = format!("{}.tmp.", basename);
        let residues: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&tmp_prefix))
            .collect();
        assert!(residues.is_empty(), "tmp residue left: {}", residues.len());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn utc_now_iso8601_format() {
        let s = utc_now_iso8601();
        assert_eq!(s.len(), "1970-01-01T00:00:00Z".len());
        assert!(s.ends_with('Z'));
    }

    #[test]
    fn epoch_secs_zero_is_unix_epoch() {
        assert_eq!(epoch_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn epoch_secs_day_boundary() {
        assert_eq!(epoch_secs_to_iso8601(86400), "1970-01-02T00:00:00Z");
    }

    #[test]
    fn pending_without_producer_deserializes() {
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

    // ─── PendingLock tests (CodeRabbit PR #71 Major fix) ───

    #[test]
    fn pending_lock_acquires_and_releases_on_drop() {
        let pending_path = unique_tmp("lock-acquire");
        let lock_path = pending_path.with_extension("lock");

        let lock = PendingLock::try_acquire(&pending_path).unwrap();
        assert!(lock.is_some(), "first try_acquire should succeed");
        assert!(lock_path.exists(), "lock file should exist while lock held");

        drop(lock);
        assert!(!lock_path.exists(), "lock file should be removed on drop");
    }

    #[test]
    fn pending_lock_returns_none_when_already_held() {
        let pending_path = unique_tmp("lock-double");
        let lock1 = PendingLock::try_acquire(&pending_path).unwrap();
        assert!(lock1.is_some());

        let lock2 = PendingLock::try_acquire(&pending_path).unwrap();
        assert!(lock2.is_none(), "second try_acquire should return None");

        drop(lock1);
        let lock3 = PendingLock::try_acquire(&pending_path).unwrap();
        assert!(
            lock3.is_some(),
            "after first drop, re-acquire should succeed"
        );
        drop(lock3);
    }

    #[test]
    fn pending_lock_recovers_from_stale_lock() {
        let pending_path = unique_tmp("lock-stale");
        let lock_path = pending_path.with_extension("lock");

        // stale lock の残骸を手動で置く (中身は観測用メタなので任意)
        std::fs::write(&lock_path, "pid=0 at=crashed").unwrap();

        // mtime を現在より 120 秒前に設定 (LOCK_STALE_SECS=60 より古い)
        let old_time = SystemTime::now() - std::time::Duration::from_secs(120);
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&lock_path)
            .unwrap();
        f.set_modified(old_time).unwrap();
        drop(f);

        // is_stale_with_threshold が stale と判定する (60 秒閾値で 120 秒前 → stale)
        assert!(PendingLock::is_stale_with_threshold(&lock_path, 60));

        // try_acquire が stale 残骸を gc して取得成功する (end-to-end 回復パスの検証)
        let lock = PendingLock::try_acquire(&pending_path).unwrap();
        assert!(lock.is_some(), "try_acquire should recover from stale lock");
        drop(lock);
        assert!(!lock_path.exists());
    }

    #[test]
    fn pending_lock_is_stale_with_threshold_rejects_fresh_lock() {
        let pending_path = unique_tmp("lock-fresh");
        let lock_path = pending_path.with_extension("lock");
        std::fs::write(&lock_path, "fresh").unwrap();

        // 60 秒閾値で作成直後 → stale ではない
        assert!(!PendingLock::is_stale_with_threshold(&lock_path, 60));

        let _ = std::fs::remove_file(&lock_path);
    }

    #[test]
    fn pending_lock_is_stale_with_threshold_on_missing_file_returns_false() {
        let pending_path = unique_tmp("lock-missing");
        let lock_path = pending_path.with_extension("lock");
        // ファイル不在 → stale 扱いしない (保守的)
        assert!(!PendingLock::is_stale_with_threshold(&lock_path, 0));
    }

    #[test]
    fn pending_lock_atomic_under_concurrent_acquirers() {
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
        use std::sync::Arc;

        let pending_path = Arc::new(unique_tmp("lock-concurrent"));
        let success_count = Arc::new(AtomicUsize::new(0));
        let none_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let p = Arc::clone(&pending_path);
                let ok = Arc::clone(&success_count);
                let ne = Arc::clone(&none_count);
                std::thread::spawn(move || match PendingLock::try_acquire(&p) {
                    Ok(Some(lock)) => {
                        ok.fetch_add(1, AtomicOrdering::Relaxed);
                        // 確実に lock を保持したまま他スレッドが None を返すのを観測できるよう
                        // 短時間保持してから drop する
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        drop(lock);
                    }
                    Ok(None) => {
                        ne.fetch_add(1, AtomicOrdering::Relaxed);
                    }
                    Err(e) => panic!("unexpected error: {}", e),
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // 排他予約が効いているなら、成功は 1 スレッドのみ・残り 7 は None
        assert_eq!(
            success_count.load(AtomicOrdering::Relaxed),
            1,
            "exactly one thread should win the lock"
        );
        assert_eq!(
            none_count.load(AtomicOrdering::Relaxed),
            7,
            "remaining seven threads should get None"
        );

        // lock path は drop 済みで存在しないはず
        let lock_path = pending_path.with_extension("lock");
        assert!(!lock_path.exists());
    }
}
