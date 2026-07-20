//! hooks-stop-tool-call-leak の exe-spawn E2E テスト (ADR-049 準拠)。
//!
//! 実 exe を `CARGO_BIN_EXE_*` 経由で spawn し、Stop hook の stdin JSON と
//! 一時 transcript JSONL を与えて stdout の decision を検証する。ユニットテストが
//! 検知関数を直接呼ぶのに対し、本テストは stdin parse -> config 読込 -> transcript
//! 解析 -> block JSON 出力の全チェーンを通す。
//!
//! fixture は実 incident (4 セッション 197 件の leak、ADR-053 §調査結果) を
//! 再現する synthetic data。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Once;

/// spawn した exe の bounded wait (dev-conventions.md § bounded wait)
const HOOK_TIMEOUT_SECS: u64 = 30;

/// 実 leak を再現する text (87387df2 セッション由来の synthetic data)
const LEAK_TEXT: &str = "court\n<invoke name=\"Bash\">\n<parameter name=\"command\">pnpm push 2>&1</parameter>\n</invoke>";

static COPY_CONFIG_ONCE: Once = Once::new();

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn exe_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-stop-tool-call-leak"))
}

/// exe は自 binary の隣の hooks-config.toml を読む。`cargo test` 時の binary は
/// `target/debug/` にあり deploy 済み config が無いため、repo の config を
/// 隣に 1 回だけコピーする (並列テストの partial-copy race を Once で回避)。
fn ensure_config_beside_exe() {
    COPY_CONFIG_ONCE.call_once(|| {
        let src = repo_root().join(".claude").join("hooks-config.toml");
        let content = std::fs::read_to_string(&src)
            .unwrap_or_else(|e| panic!("repo hooks-config.toml read failed: {e}"));
        assert_leak_config_matches_test_assumptions(&content);
        let dst = exe_path()
            .parent()
            .expect("exe has a parent dir")
            .join("hooks-config.toml");
        std::fs::write(&dst, content)
            .unwrap_or_else(|e| panic!("copy config beside exe failed: {e}"));
    });
}

/// E2E fixture (実 config) が本テスト群の前提とする具体値と一致することを検証する。
/// section 存在だけでなく `enabled` / `max_consecutive_blocks` の値まで assert し、
/// config retuning や kill-switch flip (`enabled = false`) が cap 境界テストを原因の
/// 見えない形で silent break させるのを防ぐ (ADR-041、dev-conventions.md § 外部 fixture
/// 参照テストは値まで assert)。値を変えたら assert メッセージが更新箇所を指し示す。
fn assert_leak_config_matches_test_assumptions(content: &str) {
    let config: toml::Value = toml::from_str(content)
        .unwrap_or_else(|e| panic!("repo hooks-config.toml parse failed: {e}"));
    let leak = config.get("stop_tool_call_leak").unwrap_or_else(|| {
        panic!("repo config に [stop_tool_call_leak] section が必要 (false-green guard)")
    });
    assert_eq!(
        leak.get("enabled").and_then(toml::Value::as_bool),
        Some(true),
        "E2E は [stop_tool_call_leak] enabled = true を前提とする。config で無効化するなら \
         本テスト群 (leak_transcript_blocks_with_reason 等) の期待値も同時に更新すること"
    );
    assert_eq!(
        leak.get("max_consecutive_blocks")
            .and_then(toml::Value::as_integer),
        Some(3),
        "E2E は max_consecutive_blocks = 3 を前提とする。値を変えたら cap 境界テスト \
         (consecutive_leaks_at_cap_fail_open / second_consecutive_leak_still_blocks) の \
         leak 件数も同時に更新すること"
    );
}

fn assistant_text_entry(text: &str) -> Value {
    json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}})
}

fn meta_user_entry(text: &str) -> Value {
    json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": text}})
}

fn write_transcript(dir: &tempfile::TempDir, entries: &[Value]) -> PathBuf {
    let path = dir.path().join("transcript.jsonl");
    let content = entries
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, content).expect("write transcript fixture");
    path
}

/// 子へ stdin payload を書く。**子が読まずに終了済みでも失敗させない**。
///
/// `main` は kill-switch (`STOP_TOOL_CALL_LEAK_OVERRIDE`) と `enabled = false` の
/// 2 経路で **stdin を読む前に return** する。この場合パイプの読み手が消えるため、
/// 親の `write_all` は Unix で `BrokenPipe` (EPIPE) になる。子の exit と親の write の
/// どちらが先かは競合で、Windows は小さな payload がバッファに収まり成功しがちなのに対し
/// Linux では実際に失敗する (2026-07-20、ubuntu-22.04 CI で `kill_switch_env_skips_check`
/// が Broken pipe で落ちた。WSL では通っていたため CI matrix が初めて捕捉した)。
///
/// これらの test の主題は「skip されること」であって「stdin が消費されること」ではない。
/// よって `BrokenPipe` のみ正常として飲み込み、他の I/O エラーは従来どおり panic させる。
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
        Some(value) => cmd.env("STOP_TOOL_CALL_LEAK_OVERRIDE", value),
        None => cmd.env_remove("STOP_TOOL_CALL_LEAK_OVERRIDE"),
    };
    let mut child = cmd.spawn().expect("spawn hook exe");
    write_stdin_tolerating_early_exit(&mut child, stdin_payload);
    let stdout_handle = drain_pipe_unlimited(child.stdout.take().expect("child stdout"));
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().expect("child stderr"));
    let status = wait_with_timeout_safe("hooks-stop-tool-call-leak", &mut child, HOOK_TIMEOUT_SECS)
        .expect("bounded wait");
    assert!(status.is_some(), "hook exe が {} 秒以内に終了しない", HOOK_TIMEOUT_SECS);
    (
        stdout_handle.join().expect("join stdout drain"),
        stderr_handle.join().expect("join stderr drain"),
    )
}

fn stdin_for(transcript_path: &std::path::Path) -> String {
    json!({
        "session_id": "e2e-test",
        "transcript_path": transcript_path.to_string_lossy(),
        "hook_event_name": "Stop",
        "stop_hook_active": false
    })
    .to_string()
}

#[test]
fn leak_transcript_blocks_with_reason() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = write_transcript(&dir, &[assistant_text_entry(LEAK_TEXT)]);
    let (stdout, _stderr) = run_hook(&stdin_for(&path), None);
    let decision: Value = serde_json::from_str(&stdout).expect("stdout は block JSON");
    assert_eq!(decision["decision"], "block");
    let reason = decision["reason"].as_str().expect("reason は文字列");
    assert!(reason.contains("Bash"), "reason にツール名: {}", reason);
    assert!(reason.contains("実行されていません"), "reason: {}", reason);
}

#[test]
fn clean_transcript_allows_stop() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = write_transcript(&dir, &[assistant_text_entry("作業が完了しました。")]);
    let (stdout, _stderr) = run_hook(&stdin_for(&path), None);
    assert_eq!(stdout, "", "正常応答では出力なし (停止許可)");
}

#[test]
fn second_consecutive_leak_still_blocks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let entries = [
        assistant_text_entry(LEAK_TEXT),
        meta_user_entry("Stop hook feedback: 再実行してください"),
        assistant_text_entry(LEAK_TEXT),
    ];
    let path = write_transcript(&dir, &entries);
    let (stdout, _stderr) = run_hook(&stdin_for(&path), None);
    let decision: Value = serde_json::from_str(&stdout).expect("stdout は block JSON");
    let reason = decision["reason"].as_str().expect("reason は文字列");
    assert!(reason.contains("2 回目"), "検知回数を明示: {}", reason);
}

#[test]
fn consecutive_leaks_at_cap_fail_open() {
    let dir = tempfile::tempdir().expect("temp dir");
    let entries = [
        assistant_text_entry(LEAK_TEXT),
        meta_user_entry("Stop hook feedback: 1"),
        assistant_text_entry(LEAK_TEXT),
        meta_user_entry("Stop hook feedback: 2"),
        assistant_text_entry(LEAK_TEXT),
    ];
    let path = write_transcript(&dir, &entries);
    let (stdout, stderr) = run_hook(&stdin_for(&path), None);
    assert_eq!(stdout, "", "上限到達では block しない (fail-open)");
    assert!(stderr.contains("上限"), "fail-open を stderr に明示: {}", stderr);
}

#[test]
fn kill_switch_env_skips_check() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = write_transcript(&dir, &[assistant_text_entry(LEAK_TEXT)]);
    let (stdout, stderr) = run_hook(&stdin_for(&path), Some("1"));
    assert_eq!(stdout, "", "kill-switch 有効時は検査 skip");
    assert!(stderr.contains("STOP_TOOL_CALL_LEAK_OVERRIDE"), "skip 理由を明示: {}", stderr);
}

#[test]
fn missing_transcript_fails_open() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("no-such-transcript.jsonl");
    let (stdout, stderr) = run_hook(&stdin_for(&path), None);
    assert_eq!(stdout, "", "transcript 不在では block しない");
    assert!(stderr.contains("fail-open"), "fail-open を stderr に明示: {}", stderr);
}

#[test]
fn malformed_stdin_fails_open() {
    let (stdout, stderr) = run_hook("not-a-json{", None);
    assert_eq!(stdout, "", "壊れた stdin では block しない");
    assert!(stderr.contains("fail-open"), "fail-open を stderr に明示: {}", stderr);
}
