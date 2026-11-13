//! hooks-stop-report-language の exe-spawn E2E テスト (ADR-091)。
//!
//! 実 exe を `CARGO_BIN_EXE_*` 経由で spawn し、Stop hook の stdin JSON と一時 transcript を与えて
//! stdout の decision を検証する。単体テストが判定関数を直接呼ぶのに対し、本テストは
//! stdin parse -> config 読込 -> 言語の判定 -> transcript からのセッション判定 -> block JSON 出力の
//! 全チェーンを通す。
//!
//! 報告の本文は、2026-11-13 のセッションでユーザーに英語だと指摘された報告と、同じセッションの
//! 日本語の報告の書き出し。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;

/// spawn した exe の bounded wait (custom lint rule `no-unbounded-child-wait`)
const HOOK_TIMEOUT_SECS: u64 = 30;

const OVERRIDE_ENV_VAR: &str = "STOP_REPORT_LANGUAGE_OVERRIDE";

const ENGLISH_REPORT: &str = "The feedback items you chose have been filed in the ledger as **[PR #637](https://github.com/o/r/pull/637)**. CI and the CodeRabbit review are still running.\n\n| 順位 | Tier |\n|---|---|\n| 609 | Tier 2 |";

const JAPANESE_REPORT: &str = "PR #636 をマージしました (master `7f3c425e`)。手元の作業コピーも master に追随しています。";

static WRITE_CONFIG_ONCE: Once = Once::new();

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn exe_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-stop-report-language"))
}

/// exe は自 binary の隣の hooks-config.toml を読む。`cargo test` 時の binary は `target/debug/` に
/// あり deploy 済み config が無いため、repo の config をもとにしたテスト用 config を隣に 1 回だけ
/// 書く (並列テストの partial-copy race を Once で回避)。
///
/// repo の config に `[stop_report_language]` が無ければ失敗させる (false-green guard)。
/// `enabled` はテストが自分で true にする。運用で切り替える kill-switch に E2E を引きずらせない。
fn ensure_config_beside_exe() {
    WRITE_CONFIG_ONCE.call_once(|| {
        let src = repo_root().join(".claude").join("hooks-config.toml");
        let content = std::fs::read_to_string(&src)
            .unwrap_or_else(|e| panic!("repo hooks-config.toml read failed: {e}"));
        let mut config: toml::Value = toml::from_str(&content)
            .unwrap_or_else(|e| panic!("repo hooks-config.toml parse failed: {e}"));
        let section = config
            .get_mut("stop_report_language")
            .and_then(toml::Value::as_table_mut)
            .unwrap_or_else(|| panic!("repo config に [stop_report_language] section が必要"));
        section.insert("enabled".to_string(), toml::Value::Boolean(true));
        let dst = exe_path()
            .parent()
            .expect("exe has a parent dir")
            .join("hooks-config.toml");
        let text = toml::to_string(&config).expect("test config を toml に書ける");
        std::fs::write(&dst, text).unwrap_or_else(|e| panic!("write config beside exe failed: {e}"));
    });
}

fn user_entry(text: &str) -> Value {
    json!({"type": "user", "message": {"role": "user", "content": text}})
}

fn write_transcript(dir: &tempfile::TempDir, first_prompt: &str) -> PathBuf {
    let path = dir.path().join("transcript.jsonl");
    let entries = [
        json!({"type": "user", "isMeta": true, "message": {"content": "SessionStart hook output"}}),
        user_entry(first_prompt),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "..."}]}}),
    ];
    let content = entries.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(&path, content).expect("write transcript fixture");
    path
}

fn stdin_for(transcript_path: &Path, message: &str, stop_hook_active: bool) -> String {
    json!({
        "session_id": "e2e-test",
        "transcript_path": transcript_path.to_string_lossy(),
        "hook_event_name": "Stop",
        "stop_hook_active": stop_hook_active,
        "last_assistant_message": message,
    })
    .to_string()
}

/// 子へ stdin payload を書く。子が読まずに終了済みでも失敗させない (kill-switch は stdin を読む前に
/// return するため、Unix では親の write が BrokenPipe になりうる。ADR-053 の E2E と同じ扱い)。
fn write_stdin_tolerating_early_exit(child: &mut std::process::Child, stdin_payload: &str) {
    let mut stdin = child.stdin.take().expect("child stdin");
    match stdin.write_all(stdin_payload.as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(e) => panic!("write stdin payload: {e:?}"),
    }
}

/// exe を spawn して stdin payload を与え、(stdout, stderr) を返す
fn run_hook(stdin_payload: &str, override_env: Option<&str>) -> (String, String) {
    ensure_config_beside_exe();
    let mut cmd = Command::new(exe_path());
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match override_env {
        Some(value) => cmd.env(OVERRIDE_ENV_VAR, value),
        None => cmd.env_remove(OVERRIDE_ENV_VAR),
    };
    let mut child = cmd.spawn().expect("spawn hook exe");
    write_stdin_tolerating_early_exit(&mut child, stdin_payload);
    let stdout_handle = drain_pipe_unlimited(child.stdout.take().expect("child stdout"));
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().expect("child stderr"));
    let status = wait_with_timeout_safe("hooks-stop-report-language", &mut child, HOOK_TIMEOUT_SECS)
        .expect("bounded wait");
    assert!(status.is_some(), "hook exe が {HOOK_TIMEOUT_SECS} 秒以内に終了しない");
    (
        stdout_handle.join().expect("join stdout drain"),
        stderr_handle.join().expect("join stderr drain"),
    )
}

fn interactive_transcript(dir: &tempfile::TempDir) -> PathBuf {
    write_transcript(dir, "PR のレビューを確認してください。")
}

#[test]
fn english_report_in_interactive_session_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = interactive_transcript(&dir);
    let (stdout, _) = run_hook(&stdin_for(&transcript, ENGLISH_REPORT, false), None);
    let decision: Value = serde_json::from_str(stdout.trim()).expect("block JSON を出す");
    assert_eq!(decision["decision"], "block");
    assert!(decision["reason"].as_str().unwrap().contains("日本語で書き直して"));
}

#[test]
fn japanese_report_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = interactive_transcript(&dir);
    let (stdout, _) = run_hook(&stdin_for(&transcript, JAPANESE_REPORT, false), None);
    assert_eq!(stdout.trim(), "");
}

#[test]
fn english_report_in_takt_session_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(&dir, "\n## Execution Context\n- Working Directory: C:\\work\n");
    let (stdout, _) = run_hook(&stdin_for(&transcript, ENGLISH_REPORT, false), None);
    assert_eq!(stdout.trim(), "");
}

#[test]
fn stop_hook_active_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = interactive_transcript(&dir);
    let (stdout, _) = run_hook(&stdin_for(&transcript, ENGLISH_REPORT, true), None);
    assert_eq!(stdout.trim(), "");
}

#[test]
fn unreadable_transcript_fails_open() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.jsonl");
    let (stdout, stderr) = run_hook(&stdin_for(&missing, ENGLISH_REPORT, false), None);
    assert_eq!(stdout.trim(), "");
    assert!(stderr.contains("fail-open"), "stderr: {stderr}");
}

#[test]
fn malformed_stdin_fails_open() {
    let (stdout, stderr) = run_hook("not json", None);
    assert_eq!(stdout.trim(), "");
    assert!(stderr.contains("fail-open"), "stderr: {stderr}");
}

#[test]
fn kill_switch_env_skips_check() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = interactive_transcript(&dir);
    let (stdout, stderr) = run_hook(&stdin_for(&transcript, ENGLISH_REPORT, false), Some("1"));
    assert_eq!(stdout.trim(), "");
    assert!(stderr.contains(OVERRIDE_ENV_VAR), "stderr: {stderr}");
}
