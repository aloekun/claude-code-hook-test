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

use std::path::Path;

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
}
