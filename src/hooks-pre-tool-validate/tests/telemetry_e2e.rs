//! block 時の telemetry を実 exe で検査する e2e (順位 568)。
//!
//! config 由来の生 regex が telemetry に載らないことは、`normalize_source_tag` の型
//! (`&'static str`) と単体テストで守っている。ただ、`record_preset_block` から
//! `telemetry/firings-*.jsonl` までを通しで確かめるテストが無かった (#477 の提案)。
//! allowlist に無い名前が `custom-block` に落ちることも、jsonl の出力では確かめていなかった
//! (#485 の提案)。
//!
//! `tests/smoke.rs` は「exit code と stderr の有無だけを見る」方針なので、jsonl の中身を
//! 読む本テストは別ファイルに置く。exe の staging と ETXTBSY を避けるロックは smoke.rs から
//! 複製した (ADR-084)。config は deploy 済みのものを使わず、テストごとに最小の toml を書く。
//!
//! 値はすべて synthetic な test data であり、実在のコマンド・パターンではない。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// 1 hook 呼び出しの上限秒 (smoke.rs と同じ)。
const HOOK_TIMEOUT_SECS: u64 = 30;
const EXIT_BLOCK: i32 = 2;
const KILL_SWITCH_ENV: &str = "CLAUDE_TELEMETRY_DISABLE";

const NAMED_PRESET: &str = "cargo-fmt-block";
/// 生 regex として解釈される custom pattern。jsonl にこの文字列が残ってはならない。
const CUSTOM_REGEX: &str = "e2e-custom-[0-9]+-marker";
/// preset 名の形をしているが allowlist (`KNOWN_PRESET_NAMES`) に無い名前。
/// 生 regex として解釈され、`custom-block` に落ちる。
const UNLISTED_NAME: &str = "e2e-unlisted-guard";

fn built_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-pre-tool-validate"))
}

/// exe の staging (copy) と spawn を相互排除するロック。理由は smoke.rs の
/// `EXEC_STAGING_LOCK` を参照 (Linux の ETXTBSY 対策、copy と spawn の両方を囲む)。
static EXEC_STAGING_LOCK: Mutex<()> = Mutex::new(());

fn exec_staging_guard() -> MutexGuard<'static, ()> {
    EXEC_STAGING_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// exe と、telemetry を有効にした最小の `hooks-config.toml` を temp dir へ配置する。
/// telemetry は exe の隣 (`<tmp>/telemetry/`) に書かれる。
fn stage_hook_with_telemetry() -> (tempfile::TempDir, PathBuf) {
    let _guard = exec_staging_guard();
    let tmp = tempfile::tempdir().expect("create temp dir");

    let exe_name = built_exe()
        .file_name()
        .expect("built exe has a file name")
        .to_owned();
    let staged_exe = tmp.path().join(exe_name);
    std::fs::copy(built_exe(), &staged_exe).expect("stage hook exe");

    let config = format!(
        "[telemetry]\nenabled = true\n\n[pre_tool_validate]\nblocked_patterns = [\"{NAMED_PRESET}\", \"{CUSTOM_REGEX}\", \"{UNLISTED_NAME}\"]\n"
    );
    std::fs::write(tmp.path().join("hooks-config.toml"), config).expect("write hooks-config.toml");

    (tmp, staged_exe)
}

/// staging 済み hook に Bash の `command` を渡して spawn し、exit code を返す。
/// `kill_switch` が偽なら、親の環境にある kill-switch を子から外す。
fn run_bash_hook(exe: &Path, command: &str, kill_switch: bool) -> i32 {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": { "command": command },
    })
    .to_string();

    let guard = exec_staging_guard();
    let mut cmd = Command::new(exe);
    if kill_switch {
        cmd.env(KILL_SWITCH_ENV, "1");
    } else {
        cmd.env_remove(KILL_SWITCH_ENV);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hooks-pre-tool-validate");
    drop(guard);
    let stdout_drain = drain_pipe_unlimited(child.stdout.take().expect("child stdout"));
    let stderr_drain = drain_pipe_unlimited(child.stderr.take().expect("child stderr"));
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(payload.as_bytes())
        .expect("write stdin payload");
    let status = wait_with_timeout_safe("hooks-pre-tool-validate", &mut child, HOOK_TIMEOUT_SECS)
        .expect("wait_with_timeout_safe errored");
    stdout_drain.join().expect("stdout drain thread panicked");
    stderr_drain.join().expect("stderr drain thread panicked");
    let status = status.unwrap_or_else(|| {
        panic!("hooks-pre-tool-validate hung > {HOOK_TIMEOUT_SECS}s (killed) — investigate")
    });
    status
        .code()
        .expect("hook exited via signal instead of an exit code")
}

/// `<tmp>/telemetry/firings-*.jsonl` をすべて連結して返す。
fn read_firings(tmp: &Path) -> String {
    let dir = tmp.join("telemetry");
    let mut content = String::new();
    for entry in std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("telemetry ディレクトリを読めません: {}: {e}", dir.display()))
    {
        let path = entry.expect("telemetry の entry を読めません").path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with("firings-") && name.ends_with(".jsonl") {
            content.push_str(&std::fs::read_to_string(&path).expect("read firings jsonl"));
        }
    }
    content
}

/// jsonl の各行の `id` を返す。
fn firing_ids(jsonl: &str) -> Vec<String> {
    jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("jsonl の行を parse できません: {line}: {e}"));
            value["id"]
                .as_str()
                .unwrap_or_else(|| panic!("id が文字列ではありません: {line}"))
                .to_string()
        })
        .collect()
}

/// block した入力と、その入力が jsonl に残してはならない文字列の組を検査する。
fn assert_blocked_as_custom(command: &str, forbidden: &[&str]) {
    let (tmp, exe) = stage_hook_with_telemetry();
    assert_eq!(run_bash_hook(&exe, command, false), EXIT_BLOCK, "{command} が block されない");

    let jsonl = read_firings(tmp.path());
    assert_eq!(firing_ids(&jsonl), vec!["custom-block".to_string()], "jsonl: {jsonl}");
    for needle in forbidden {
        assert!(
            !jsonl.contains(needle),
            "telemetry に config 由来の文字列 {needle:?} が残っています (ADR-055): {jsonl}"
        );
    }
}

#[test]
fn custom_regex_block_is_recorded_as_custom_block_without_the_pattern() {
    assert_blocked_as_custom(
        "echo e2e-custom-42-marker",
        &[CUSTOM_REGEX, "e2e-custom", "echo e2e"],
    );
}

#[test]
fn unlisted_name_block_is_recorded_as_custom_block_without_the_name() {
    assert_blocked_as_custom("echo e2e-unlisted-guard", &[UNLISTED_NAME, "e2e-unlisted"]);
}

#[test]
fn named_preset_block_is_recorded_under_the_preset_name() {
    let (tmp, exe) = stage_hook_with_telemetry();
    assert_eq!(run_bash_hook(&exe, "cargo fmt --all", false), EXIT_BLOCK);

    let jsonl = read_firings(tmp.path());
    assert_eq!(firing_ids(&jsonl), vec![NAMED_PRESET.to_string()], "jsonl: {jsonl}");
}

#[test]
fn kill_switch_prevents_the_telemetry_directory() {
    let (tmp, exe) = stage_hook_with_telemetry();
    assert_eq!(run_bash_hook(&exe, "cargo fmt --all", true), EXIT_BLOCK);

    assert!(
        !tmp.path().join("telemetry").exists(),
        "kill-switch ({KILL_SWITCH_ENV}=1) を立てても telemetry ディレクトリが作られました"
    );
}
