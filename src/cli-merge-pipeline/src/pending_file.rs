//! post-merge-feedback pending file の読み書きと入力検証 (ADR-029)
//!
//! pending file は `.claude/post-merge-feedback-pending.json` に配置され、
//! cli-merge-pipeline が post-merge ステップ (`type = "ai"`) で書き込み、
//! hooks-stop-feedback-dispatch が Stop 時に検出して Claude に skill 起動を指示する。
//!
//! 共有スキーマ・定数・UTC ヘルパーは `lib-pending-file` に集約。
//! 本モジュールは cli-merge-pipeline 固有の書き込みロジックと I/O を担う。
//!
//! 書き込み経路は 2 種類 (ADR-029 §破損耐性):
//!   - 新規作成: `OpenOptions::new().write(true).create_new(true).open(path)` で
//!     最終ファイルを直接 atomic 排他作成 (O_EXCL 相当)。rename は使わない。
//!   - 上書き (既存 Consumed/Corrupt 削除後): tmp file → `fs::rename` の 2 段階。
//!
//! 排他性保証:
//!   - 新規作成経路: `create_new` の `AlreadyExists` で TOCTOU race を atomic に検出
//!     (ADR-029 §競合ポリシーの「skip + WARN で取りこぼしを観測可能」を実装レベルで保証)
//!   - 上書き経路: read→write 間の race は許容 (Consumed/Corrupt は稀経路、
//!     破損ポリシーで自己回復)

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// ─── Re-exports from lib-pending-file ───

pub(crate) use lib_pending_file::PendingFile;
pub(crate) use lib_pending_file::{
    is_valid_owner_repo, utc_now_iso8601, FILE_NAME, SCHEMA_VERSION, STATUS_CONSUMED,
    STATUS_DISPATCHED, STATUS_PENDING,
};

// ─── cli-merge-pipeline-local items ───

/// プロセス内で一意な tmp ファイル名を生成するためのカウンタ。
/// 複数の writer が同時に `write_overwrite` を呼んでも tmp パスが衝突しない。
/// `write_new_exclusive` は tmp path を使わないため本カウンタを参照しない。
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// producer 文字列を生成する (`cli-merge-pipeline@pid-{pid}@{iso8601}`)。
///
/// PID は再利用されるため timestamp を併記して時系列追跡可能にする。hostname は YAGNI で省略。
pub(crate) fn producer_string() -> String {
    format!(
        "cli-merge-pipeline@pid-{}@{}",
        std::process::id(),
        utc_now_iso8601()
    )
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

/// pending file 書き込みの失敗種別。
///
/// `AlreadyExists` は新規作成経路で TOCTOU race を atomic に検出した場合に返る
/// (ADR-029 §競合ポリシー)。呼び出し側は `WARN + skip` でログに残すことで
/// 「取りこぼしの可視化」を保証する。
#[derive(Debug)]
pub(crate) enum WriteError {
    /// 新規作成時に既に pending file が存在した (他プロセスが先に書き込んだ)
    AlreadyExists,
    /// I/O エラー (書き込み / sync / rename 失敗)
    Io(String),
    /// serde_json シリアライズ失敗
    Serialize(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::AlreadyExists => write!(f, "pending file already exists"),
            WriteError::Io(msg) => write!(f, "I/O error: {}", msg),
            WriteError::Serialize(msg) => write!(f, "serialize error: {}", msg),
        }
    }
}

/// pending file を **新規排他作成** する (ADR-029 §競合ポリシー)。
///
/// `OpenOptions::create_new(true)` (O_EXCL 相当) で最終ファイルを直接 atomic 排他作成する。
/// 他プロセスが先に作成済みなら `WriteError::AlreadyExists` を返し、呼び出し側は
/// WARN + skip で取りこぼしを観測可能にする。
///
/// **rename は使わない** — `rename` は既存を無条件上書きするため、placeholder 方式だと
/// 排他予約が自壊する (CodeRabbit PR #70 Major 指摘の本質)。
pub(crate) fn write_new_exclusive(path: &Path, pending: &PendingFile) -> Result<(), WriteError> {
    let json =
        serde_json::to_string_pretty(pending).map_err(|e| WriteError::Serialize(e.to_string()))?;

    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(json.as_bytes()) {
                return Err(WriteError::Io(format!("write_all 失敗: {}", e)));
            }
            if let Err(e) = file.sync_all() {
                return Err(WriteError::Io(format!("sync_all 失敗: {}", e)));
            }
            Ok(())
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(WriteError::AlreadyExists),
        Err(e) => Err(WriteError::Io(format!(
            "create_new 失敗 ({}): {}",
            path.display(),
            e
        ))),
    }
}

/// pending file を **上書き書き込み** する (tmp → rename 2 段階)。
///
/// 呼び出し元は事前に `read_existing` で `Consumed` / `Corrupt` を判定し、
/// ファイルを削除してから本関数を呼ぶことを想定。稀経路なので read→write 間の
/// race は許容 (ADR-029 §競合ポリシー)。
pub(crate) fn write_overwrite(path: &Path, pending: &PendingFile) -> Result<(), WriteError> {
    let json =
        serde_json::to_string_pretty(pending).map_err(|e| WriteError::Serialize(e.to_string()))?;
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pending".to_string());
    let tmp_name = format!("{}.tmp.{}.{}", file_name, std::process::id(), counter);
    let tmp_path = path.with_file_name(tmp_name);
    if let Err(e) = std::fs::write(&tmp_path, &json) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(WriteError::Io(format!(
            "tmp 書き込み失敗 ({}): {}",
            tmp_path.display(),
            e
        )));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(WriteError::Io(format!(
            "rename 失敗 ({} → {}): {}",
            tmp_path.display(),
            path.display(),
            e
        )));
    }
    Ok(())
}

/// pending file のデフォルト配置先 (exe と同じディレクトリ = `.claude/`)。
///
/// 本プロジェクトは `pnpm deploy:hooks` で exe を `.claude/` に配置するため、
/// exe の親ディレクトリが pending file の正しい置き場になる。派生プロジェクトも同様。
pub(crate) fn default_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
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
            producer: None,
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
    fn write_new_exclusive_creates_file() {
        let path = unique_tmp("write-new-exclusive");
        let pending = sample_pending(STATUS_PENDING);

        write_new_exclusive(&path, &pending).unwrap();

        let loaded: PendingFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, pending);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_new_exclusive_returns_already_exists_when_target_present() {
        let path = unique_tmp("already-exists");
        let pending = sample_pending(STATUS_PENDING);

        // 1 回目は成功
        write_new_exclusive(&path, &pending).unwrap();
        // 2 回目は AlreadyExists
        match write_new_exclusive(&path, &pending) {
            Err(WriteError::AlreadyExists) => {}
            other => panic!("expected AlreadyExists, got {:?}", other),
        }
        // 既存内容は上書きされていない
        let loaded: PendingFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, pending);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_new_exclusive_atomic_under_concurrent_writers() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::atomic::Ordering as AtomicOrdering;
        use std::sync::Arc;

        let path = Arc::new(unique_tmp("concurrent-new-exclusive"));
        let success_count = Arc::new(AtomicUsize::new(0));
        let already_exists_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = Arc::clone(&path);
                let ok = Arc::clone(&success_count);
                let ae = Arc::clone(&already_exists_count);
                std::thread::spawn(move || {
                    let mut pf = sample_pending(STATUS_PENDING);
                    pf.pr_number = i + 1;
                    match write_new_exclusive(&p, &pf) {
                        Ok(()) => {
                            ok.fetch_add(1, AtomicOrdering::Relaxed);
                        }
                        Err(WriteError::AlreadyExists) => {
                            ae.fetch_add(1, AtomicOrdering::Relaxed);
                        }
                        Err(other) => panic!("unexpected write error: {:?}", other),
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // atomic 排他予約が効いているなら、成功は 1 スレッドのみ・残りは AlreadyExists
        assert_eq!(success_count.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(already_exists_count.load(AtomicOrdering::Relaxed), 7);

        // 最終ファイルは有効な JSON
        let content = std::fs::read_to_string(path.as_ref()).unwrap();
        let _: PendingFile = serde_json::from_str(&content).unwrap();

        let _ = std::fs::remove_file(path.as_ref());
    }

    #[test]
    fn write_overwrite_replaces_existing_file() {
        let path = unique_tmp("overwrite");
        let first = sample_pending(STATUS_CONSUMED);
        let mut second = sample_pending(STATUS_PENDING);
        second.pr_number = 999;

        write_new_exclusive(&path, &first).unwrap();
        write_overwrite(&path, &second).unwrap();

        let loaded: PendingFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, second);

        // tmp 残骸 (`{basename}.tmp.{pid}.{counter}`) が残っていないこと
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        let basename = path.file_name().unwrap().to_string_lossy().into_owned();
        let tmp_prefix = format!("{}.tmp.", basename);
        let residues: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&tmp_prefix))
            .collect();
        assert!(
            residues.is_empty(),
            "tmp residue: {} entries",
            residues.len()
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn producer_string_contains_pid_and_timestamp() {
        let s = producer_string();
        assert!(s.starts_with("cli-merge-pipeline@pid-"));
        // @{iso8601} が含まれることを "Z" で軽く確認
        assert!(s.ends_with('Z'), "expected iso8601 suffix: {}", s);
        // pid 部分が数値
        let pid_part = s
            .strip_prefix("cli-merge-pipeline@pid-")
            .and_then(|rest| rest.split('@').next())
            .unwrap();
        assert!(pid_part.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn pending_file_roundtrip_with_producer() {
        let mut pending = sample_pending(STATUS_PENDING);
        pending.producer = Some("cli-merge-pipeline@pid-1234@2026-04-23T12:34:56Z".to_string());

        let json = serde_json::to_string(&pending).unwrap();
        let loaded: PendingFile = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, pending);
    }

    #[test]
    fn pending_file_without_producer_field_deserializes() {
        // producer フィールド不在の JSON (schema v1 既存ファイル) が正しく読めること
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
        let loaded: PendingFile = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.producer, None);
        assert_eq!(loaded.pr_number, 42);
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
        write_new_exclusive(&path, &pending).unwrap();

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
        write_new_exclusive(&path, &pending).unwrap();

        match read_existing(&path) {
            ExistingPending::Corrupt(reason) => assert!(reason.contains("unknown status")),
            other => panic!("expected Corrupt, got {:?}", other),
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_existing_returns_active_for_pending_and_dispatched() {
        let path_p = unique_tmp("active-pending");
        write_new_exclusive(&path_p, &sample_pending(STATUS_PENDING)).unwrap();
        match read_existing(&path_p) {
            ExistingPending::Active(s) => assert_eq!(s, STATUS_PENDING),
            other => panic!("expected Active(pending), got {:?}", other),
        }
        let _ = std::fs::remove_file(&path_p);

        let path_d = unique_tmp("active-dispatched");
        write_new_exclusive(&path_d, &sample_pending(STATUS_DISPATCHED)).unwrap();
        match read_existing(&path_d) {
            ExistingPending::Active(s) => assert_eq!(s, STATUS_DISPATCHED),
            other => panic!("expected Active(dispatched), got {:?}", other),
        }
        let _ = std::fs::remove_file(&path_d);
    }

    #[test]
    fn read_existing_returns_consumed_for_consumed_status() {
        let path = unique_tmp("consumed");
        write_new_exclusive(&path, &sample_pending(STATUS_CONSUMED)).unwrap();
        assert!(matches!(read_existing(&path), ExistingPending::Consumed));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn utc_now_iso8601_matches_expected_format() {
        let s = utc_now_iso8601();
        // YYYY-MM-DDTHH:MM:SSZ = 20 chars
        assert_eq!(
            s.len(),
            "1970-01-01T00:00:00Z".len(),
            "unexpected length: {}",
            s
        );
        assert!(s.ends_with('Z'));
        assert_eq!(s.chars().nth(4), Some('-'));
        assert_eq!(s.chars().nth(7), Some('-'));
        assert_eq!(s.chars().nth(10), Some('T'));
        assert_eq!(s.chars().nth(13), Some(':'));
        assert_eq!(s.chars().nth(16), Some(':'));
    }
}
