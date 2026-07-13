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
//! 相違点: timestamp は ISO8601 ではなく unix epoch 秒を直接記録する (parser 不要)。
//! future timestamp は stale 扱い (破損 lock が永続 fresh 化する bug class の再発防止、
//! lock.rs の PastTime と同じ invariant)。
//!
//! ファイル形式は `key=value` 行 (pid / start_unix / label)。外部 config ではなく
//! 内部の一時ファイルのため、依存追加 (serde/toml) を避けた最小形式とする。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// lock ファイル名 (`.claude/` 配下、gitignore 対象、checkout ごとに独立)。
pub const PIPELINE_LOCK_FILENAME: &str = "pipeline.lock";

/// stale 判定 threshold。pipeline の実測最長 (push ~15 分) の 2x で安全マージン。
pub const PIPELINE_LOCK_STALE_SECS: i64 = 1800;

/// lock 取得成功時に保持する RAII guard。Drop で lock ファイルを削除する。
pub struct PipelineLock {
    path: PathBuf,
}

impl Drop for PipelineLock {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[pipeline-lock] cleanup 失敗: {}", e);
            }
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
    let content = build_lock_content(std::process::id(), now_unix, label);

    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                eprintln!("[pipeline-lock] 書き込み失敗 (継続): {}", e);
            }
            PipelineLockResult::Acquired(PipelineLock { path })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if let Some((pid, age_secs)) =
                read_fresh_lock(&path, stale_threshold_secs, now_unix)
            {
                return PipelineLockResult::Busy {
                    holder_pid: pid,
                    holder_age_secs: age_secs,
                };
            }
            if let Err(e) = std::fs::write(&path, content) {
                eprintln!("[pipeline-lock] takeover 書き込み失敗 (継続): {}", e);
            }
            PipelineLockResult::Acquired(PipelineLock { path })
        }
        Err(e) => PipelineLockResult::Unavailable {
            reason: e.to_string(),
        },
    }
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

fn build_lock_content(pid: u32, start_unix: i64, label: &str) -> String {
    format!(
        "pid={}\nstart_unix={}\nlabel={}\n",
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
    let pid: u32 = parse_field(&content, "pid")?.parse().ok()?;
    let start_unix: i64 = parse_field(&content, "start_unix")?.parse().ok()?;
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
mod tests {
    use super::*;

    fn temp_lock_path(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "pipeline-lock-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn acquire_creates_lock_and_drop_removes_it() {
        let path = temp_lock_path("acquire");
        let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
        assert!(matches!(result, PipelineLockResult::Acquired(_)));
        assert!(path.exists());
        drop(result);
        assert!(!path.exists(), "RAII drop で lock が削除される");
    }

    #[test]
    fn second_acquire_is_busy_while_fresh() {
        let path = temp_lock_path("busy");
        let _guard = acquire_pipeline_lock_at(path.clone(), "merge", 1800, 1_000_000);
        let second = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_100);
        match second {
            PipelineLockResult::Busy {
                holder_pid,
                holder_age_secs,
            } => {
                assert_eq!(holder_pid, std::process::id());
                assert_eq!(holder_age_secs, 100);
            }
            _ => panic!("fresh lock 保持中は Busy になるべき"),
        }
    }

    #[test]
    fn stale_lock_is_taken_over() {
        let path = temp_lock_path("stale");
        std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();
        let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000 + 1800);
        assert!(
            matches!(result, PipelineLockResult::Acquired(_)),
            "threshold 到達で takeover"
        );
    }

    #[test]
    fn future_dated_lock_is_treated_as_stale() {
        let path = temp_lock_path("future");
        std::fs::write(&path, "pid=99999\nstart_unix=2000000\nlabel=corrupt\n").unwrap();
        let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
        assert!(
            matches!(result, PipelineLockResult::Acquired(_)),
            "future timestamp は stale 扱い (永続 fresh 化 bug class の防止)"
        );
    }

    #[test]
    fn corrupt_lock_is_taken_over() {
        let path = temp_lock_path("corrupt");
        std::fs::write(&path, "not a lock file").unwrap();
        let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
        assert!(matches!(result, PipelineLockResult::Acquired(_)));
    }

    #[test]
    fn read_fresh_lock_parses_fields_and_age() {
        let path = temp_lock_path("read");
        std::fs::write(&path, "pid=4321\nstart_unix=1000000\nlabel=merge\n").unwrap();
        let held = read_fresh_lock(&path, 1800, 1_000_500);
        assert_eq!(held, Some((4321, 500)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_lock_reads_as_not_held() {
        let path = temp_lock_path("missing");
        assert_eq!(read_fresh_lock(&path, 1800, 1_000_000), None);
    }
}
