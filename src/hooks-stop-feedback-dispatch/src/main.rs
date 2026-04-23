//! Stop フィードバックディスパッチフック (ADR-029)
//!
//! `.claude/post-merge-feedback-pending.json` を検出し、status="pending" なら
//! additionalContext で Claude に `/post-merge-feedback` skill 起動を指示して
//! status="dispatched" に atomic 更新する。
//!
//! 責務分離 (ADR-022 原則 1):
//!   - hooks-stop-quality (既存) は lint / test / build を担う
//!   - hooks-stop-feedback-dispatch (本 exe) は pending file の発火判定 + 更新のみ
//!   - 実行順序 1 → 2 は settings.local.json.template の array order で保証
//!
//! 無限ループ防止:
//!   stop_hook_active == true なら pending を読まず silent exit (ADR-004 の先行パターン)

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

mod pending_file;
use pending_file::{
    epoch_secs_to_iso8601, read_existing, utc_now_epoch_secs, utc_now_iso8601, write_overwrite,
    ExistingPending, PendingFile, PendingLock, FILE_NAME, STATUS_DISPATCHED,
};

/// stale TTL (24 時間、ADR-029 §破損耐性)
const STALE_TTL_SECS: u64 = 24 * 60 * 60;

/// Claude が実行すべき slash command (ADR-029 §additionalContext 構造化フォーマット)
const ACTION: &str = "invoke_skill";
const REASON: &str = "cli-merge-pipeline wrote pending artifact";

/// Stop hook 入力 (必要なフィールドのみ)
#[derive(Deserialize)]
struct HookInput {
    stop_hook_active: Option<bool>,
}

/// hookSpecificOutput JSON 出力 (additionalContext 発火用)
#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: &'static str,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

#[derive(Serialize)]
struct Output {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

/// pending file の配置パス (exe と同じディレクトリ = `.claude/`)
fn pending_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(FILE_NAME)
}

/// additionalContext 文字列を組み立てる (ADR-029 の固定キー順序)
fn build_additional_context(pending: &PendingFile) -> String {
    let command = format!("/post-merge-feedback {}", pending.pr_number);
    format!(
        "[POST_MERGE_FEEDBACK_TRIGGER]\n\
         schema_version: {}\n\
         pr_number: {}\n\
         owner_repo: {}\n\
         action: {}\n\
         command: {}\n\
         reason: {}",
        pending.schema_version, pending.pr_number, pending.owner_repo, ACTION, command, REASON,
    )
}

/// created_at + STALE_TTL_SECS < now なら true (stale)。
///
/// ISO 8601 UTC `YYYY-MM-DDTHH:MM:SSZ` は固定桁 + UTC のため文字列の辞書順が
/// 時刻順と等価になる性質を利用して、epoch 計算を 1 回 (threshold 生成) に抑える。
fn is_stale(created_at: &str, now_secs: u64) -> bool {
    let threshold = epoch_secs_to_iso8601(now_secs.saturating_sub(STALE_TTL_SECS));
    created_at < threshold.as_str()
}

/// additionalContext を stdout へ JSON として emit する
fn emit_trigger(pending: &PendingFile) {
    let output = Output {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "Stop",
            additional_context: build_additional_context(pending),
        },
    };
    match serde_json::to_string(&output) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!(
            "[stop-feedback-dispatch] Warning: failed to serialize output: {}",
            e
        ),
    }
}

/// pending ファイルを status="dispatched" に更新する。失敗時は stderr に WARN を出すが
/// exit code は 0 のまま (hook は fail-open)。
fn mark_dispatched(path: &Path, mut pending: PendingFile) {
    pending.status = STATUS_DISPATCHED.to_string();
    pending.dispatched_at = Some(utc_now_iso8601());
    if let Err(e) = write_overwrite(path, &pending) {
        eprintln!(
            "[stop-feedback-dispatch] Warning: failed to update status to dispatched: {}",
            e
        );
    }
}

/// stale pending を削除し、有効な pending から additionalContext を発火して dispatched に遷移する。
///
/// **排他制御** (CodeRabbit PR #71 Major fix): read→emit→write を process-level で排他化するため
/// `PendingLock` を取得する。lock 取得後に再読込して status=pending を再検証することで、初回
/// read と lock acquire の間に別プロセスが dispatched/consumed に書き換えた場合の二重発火を防ぐ。
///
/// stale 判定は lock 不要 (削除のみなら任意 hook が GC できる)。
fn handle_pending(path: &Path, pending: PendingFile) {
    if is_stale(&pending.created_at, utc_now_epoch_secs()) {
        eprintln!(
            "[stop-feedback-dispatch] Warning: stale pending file removed (created_at={})",
            pending.created_at
        );
        let _ = std::fs::remove_file(path);
        return;
    }

    let _lock = match PendingLock::try_acquire(path) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            // 他プロセスが dispatch 中 → 二重発火防止で何もしない
            return;
        }
        Err(e) => {
            eprintln!(
                "[stop-feedback-dispatch] Warning: lock acquisition failed ({}); skip dispatch",
                e
            );
            return;
        }
    };

    // lock 保有中に再読込: read→lock の race で別プロセスが dispatched/consumed に
    // 書き換えた可能性を排除する
    match read_existing(path) {
        ExistingPending::Pending(reread) => {
            emit_trigger(&reread);
            mark_dispatched(path, reread);
        }
        _ => {
            // 別プロセスが先に処理完了 / 破損 → 何もしない (RAII で lock 解放)
        }
    }

}

fn main() {
    // stdin を読む (エラー時も fail-open: silent exit)
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return,
    };

    // 無限ループ防止: stop_hook_active なら pending を読まない
    if hook_input.stop_hook_active.unwrap_or(false) {
        return;
    }

    let path = pending_path();

    match read_existing(&path) {
        ExistingPending::None => {}
        ExistingPending::Corrupt(reason) => {
            eprintln!(
                "[stop-feedback-dispatch] Warning: corrupt pending file removed ({})",
                reason
            );
            let _ = std::fs::remove_file(&path);
        }
        ExistingPending::Pending(pending) => handle_pending(&path, pending),
        ExistingPending::Dispatched => {
            // 二重通知防止で silent exit
        }
        ExistingPending::Consumed => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pending_file::{SCHEMA_VERSION, STATUS_PENDING};

    fn sample_pending() -> PendingFile {
        PendingFile {
            schema_version: SCHEMA_VERSION,
            pr_number: 123,
            owner_repo: "aloekun/claude-code-hook-test".to_string(),
            prompt: "post-merge-feedback".to_string(),
            status: STATUS_PENDING.to_string(),
            created_at: "2026-04-23T10:00:00Z".to_string(),
            dispatched_at: None,
            consumed_at: None,
            producer: None,
        }
    }

    #[test]
    fn stop_hook_active_true_field_parsed() {
        let json = r#"{"stop_hook_active": true}"#;
        let hook: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(hook.stop_hook_active, Some(true));
    }

    #[test]
    fn stop_hook_active_missing_defaults_false() {
        let json = r#"{}"#;
        let hook: HookInput = serde_json::from_str(json).unwrap();
        assert!(!hook.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn additional_context_key_order_is_fixed() {
        let pending = sample_pending();
        let ctx = build_additional_context(&pending);

        // キー順序: schema_version → pr_number → owner_repo → action → command → reason
        let lines: Vec<&str> = ctx.split('\n').collect();
        assert_eq!(lines[0], "[POST_MERGE_FEEDBACK_TRIGGER]");
        assert!(lines[1].starts_with("schema_version: "));
        assert!(lines[2].starts_with("pr_number: "));
        assert!(lines[3].starts_with("owner_repo: "));
        assert!(lines[4].starts_with("action: "));
        assert!(lines[5].starts_with("command: "));
        assert!(lines[6].starts_with("reason: "));

        // 値の埋め込みも検証
        assert!(ctx.contains("schema_version: 1"));
        assert!(ctx.contains("pr_number: 123"));
        assert!(ctx.contains("owner_repo: aloekun/claude-code-hook-test"));
        assert!(ctx.contains("action: invoke_skill"));
        assert!(ctx.contains("command: /post-merge-feedback 123"));
        assert!(ctx.contains("reason: cli-merge-pipeline wrote pending artifact"));
    }

    #[test]
    fn additional_context_tag_is_first_line() {
        let pending = sample_pending();
        let ctx = build_additional_context(&pending);
        assert!(
            ctx.starts_with("[POST_MERGE_FEEDBACK_TRIGGER]\n"),
            "tag must be on first line for reliable detection: {}",
            ctx
        );
    }

    #[test]
    fn is_stale_returns_true_when_older_than_24h() {
        // created_at = 1970-01-01T00:00:00Z, now = 2日後 (48h) → stale
        let now_secs = 48 * 3600;
        assert!(is_stale("1970-01-01T00:00:00Z", now_secs));
    }

    #[test]
    fn is_stale_returns_false_when_within_24h() {
        // created_at = 1970-01-02T00:00:00Z (86400s), now = 1日後 + 1h (90000s) → 差分 3600s < 24h
        let now_secs = 86400 + 3600;
        assert!(!is_stale("1970-01-02T00:00:00Z", now_secs));
    }

    #[test]
    fn is_stale_boundary_exactly_24h() {
        // 差分ちょうど 24h のとき、threshold == created_at → `<` なので stale=false
        let now_secs = 86400 + 24 * 3600;
        assert!(!is_stale("1970-01-02T00:00:00Z", now_secs));
    }

    #[test]
    fn is_stale_returns_true_just_past_24h() {
        // 差分 24h + 1s → threshold > created_at → stale=true
        let now_secs = 86400 + 24 * 3600 + 1;
        assert!(is_stale("1970-01-02T00:00:00Z", now_secs));
    }

    #[test]
    fn hook_specific_output_serializes_with_correct_keys() {
        let output = Output {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "Stop",
                additional_context: "dummy".to_string(),
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains(r#""hookSpecificOutput""#));
        assert!(json.contains(r#""hookEventName":"Stop""#));
        assert!(json.contains(r#""additionalContext":"dummy""#));
    }

    #[test]
    fn mark_dispatched_sets_status_and_timestamp() {
        let path = std::env::temp_dir().join(format!(
            "test-mark-dispatched-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        // 既存の pending ファイルを作成
        let json = serde_json::to_string_pretty(&sample_pending()).unwrap();
        std::fs::write(&path, json).unwrap();

        mark_dispatched(&path, sample_pending());

        let loaded: PendingFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.status, STATUS_DISPATCHED);
        assert!(loaded.dispatched_at.is_some());
        // dispatched_at は ISO 8601 フォーマット
        let ts = loaded.dispatched_at.unwrap();
        assert_eq!(ts.len(), "1970-01-01T00:00:00Z".len());
        assert!(ts.ends_with('Z'));

        let _ = std::fs::remove_file(&path);
    }
}
