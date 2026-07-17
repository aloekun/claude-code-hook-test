//! T7 incident 回帰テスト: Stop 品質ゲートの cwd 依存 (ADR-049 の流儀)。
//!
//! **由来 incident** (2026-07-16 に実発火。`docs/push-pipeline-fix-plan.md` §4 T7):
//! セッションの cwd がリポジトリルート以外 (`.takt/runs` に `cd` したまま Stop) のとき、
//! `hooks-config.toml` の file-length step
//! (`.\.claude\hooks-post-tool-comment-lint-rust.exe --check-modified-files`) が
//! 「指定されたパスが見つかりません」で失敗し、品質ゲートが**誤 block** した。
//! 同じ cwd drift で takt subsession 判定 (`<cwd>/.takt/runs` を探す) も黙って
//! 空振りしており、ADR-004 § takt subsession skip が効かない状態だった。
//!
//! **テスト方針**: 実 exe を `CARGO_BIN_EXE_*` で spawn するが、`target/debug/` の exe を
//! そのまま起動すると exe-relative のルート導出 (`<root>/.claude/<hook>.exe`) を
//! 素通りしてしまう。よって **ADR-010 の実配置と同じ `<root>/.claude/` へ exe を staging**
//! して起動し、stdin JSON → config 読込 → cwd 正規化 → step 実行 → block JSON の
//! 全経路を通す。
//!
//! bad = incident 状態 (cwd ≠ root) で誤 block しないこと。
//! good = 正規化がゲート自体を骨抜きにしていないこと (実失敗は cwd に依らず block)。
//!
//! `run_cmd_shell_capped` が `cmd /c` 依存のため Windows でのみ実行する
//! (WP-16 CI matrix の非 Windows leg では skip)。
#![cfg(windows)]

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// spawn した hook exe の bounded wait (dev-conventions.md § bounded wait)。
const HOOK_TIMEOUT_SECS: u64 = 60;

static UNIQUE_COUNTER: AtomicU32 = AtomicU32::new(0);

/// incident と同じ形の「ルート相対パスを含む step cmd」。
///
/// 由来 incident の file-length step (`.\.claude\hooks-post-tool-comment-lint-rust.exe ...`) と
/// 同じ**構造**を最小の形で再現する: cmd.exe から見て cwd 相対で解決されるルート相対パス。
/// 実 linter exe を呼ぶ代わりに probe を使うのは、本テストが検証するのが
/// linter の挙動ではなく **step がどの cwd で起動されるか**だけだから
/// (1 fixture = 1 failure mode)。
const ROOT_RELATIVE_STEP_CMD: &str = r".\.claude\probe.cmd";

fn exe_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-stop-quality"))
}

/// ADR-010 の実配置 (`<root>/.claude/<hook>.exe`) を temp に組み立て、root を返す。
///
/// - `<root>/.claude/hooks-stop-quality.exe` — 被テスト exe (exe-relative 解決の起点)
/// - `<root>/.claude/hooks-config.toml` — 与えられた step 定義
/// - `<root>/.claude/probe.cmd` — ルート相対で呼ばれる成功 probe
/// - `<root>/.takt/runs/` — incident の cwd (存在する非ルートディレクトリ)
fn stage_project(prefix: &str, steps_toml: &str) -> PathBuf {
    let n = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("t7_{}_{}_{}", prefix, std::process::id(), n));
    let claude_dir = root.join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("create .claude");
    std::fs::create_dir_all(root.join(".takt").join("runs")).expect("create .takt/runs");

    std::fs::copy(exe_path(), claude_dir.join("hooks-stop-quality.exe")).expect("stage exe");
    std::fs::write(claude_dir.join("probe.cmd"), "@exit 0\r\n").expect("write probe");
    std::fs::write(
        claude_dir.join("hooks-config.toml"),
        format!("[stop_quality]\nstep_timeout = 30\n\n{}", steps_toml),
    )
    .expect("write config");
    root
}

/// staging 済み exe を `cwd` から起動し、stdout を返す。
fn run_hook(root: &Path, cwd: &Path) -> String {
    let mut child = Command::new(root.join(".claude").join("hooks-stop-quality.exe"))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hooks-stop-quality");

    let stdout = drain_pipe_unlimited(Box::new(child.stdout.take().expect("stdout piped")));
    let stderr = drain_pipe_unlimited(Box::new(child.stderr.take().expect("stderr piped")));
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(br#"{"stop_hook_active": false}"#)
        .expect("write stdin");

    let status = wait_with_timeout_safe("t7-hook", &mut child, HOOK_TIMEOUT_SECS)
        .expect("hook wait")
        .expect("hook must not time out");
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    assert_hook_success(status, &out, &err);
    out
}

/// hook プロセスが正常終了 (exit code 0) したことを検証する。
///
/// 各テストの判定は stdout の block JSON だけを見るため、hook が crash / 非 0 exit で
/// **何も出力しなかった**場合に `block_reason` が `None` を返し、「block しなかった」= 期待通り
/// と誤って通ってしまう (`None` を期待する 3 本が false green になる)。exit code を独立に
/// assert してこの穴を塞ぐ。
///
/// 失敗時は **stderr を出す**のが要点。本 hook の診断は `eprintln!` (cwd 正規化の警告等) =
/// stderr に出るため、stdout が空になる失敗ケースでは stderr だけが手掛かりになる。
fn assert_hook_success(status: std::process::ExitStatus, out: &str, err: &str) {
    assert!(
        status.success(),
        "hook exited with exit code {:?}\n--- stdout ---\n{out}\n--- stderr ---\n{err}",
        status.code()
    );
}

/// stdout から block の reason を取り出す。block でなければ `None`。
///
/// assert は構造化 field (`decision`) のみに掛け、reason の文言は固定しない
/// (メッセージ修正でテストが壊れないように)。
fn block_reason(stdout: &str) -> Option<String> {
    let v: Value = serde_json::from_str(stdout.trim()).ok()?;
    if v.get("decision")?.as_str()? != "block" {
        return None;
    }
    Some(v.get("reason")?.as_str()?.to_string())
}

fn write_run_meta(root: &Path, slug: &str, status: &str) {
    let run_dir = root.join(".takt").join("runs").join(slug);
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(
        run_dir.join("meta.json"),
        format!(r#"{{"status": "{}"}}"#, status),
    )
    .expect("write meta.json");
}

fn step(name: &str, cmd: &str) -> String {
    format!("[[stop_quality.steps]]\nname = \"{name}\"\ncmd = '{cmd}'\n")
}

/// **bad (incident 再現 / 症状 1 = ルート相対 step の誤失敗)**:
/// cwd が `.takt/runs` でも、ルート相対パスの step が解決でき、
/// 品質ゲートが誤 block しないこと。
///
/// 修正前はここで cmd.exe が「指定されたパスが見つかりません」を返し block していた。
#[test]
fn root_relative_step_succeeds_from_non_root_cwd() {
    let root = stage_project("relstep-nonroot", &step("file-length", ROOT_RELATIVE_STEP_CMD));
    let out = run_hook(&root, &root.join(".takt").join("runs"));
    assert_eq!(
        block_reason(&out),
        None,
        "cwd ≠ root でもルート相対 step は解決できねばならない (T7 incident): {out}"
    );
}

/// **good**: cwd = root の正常経路を正規化が壊していないこと。
#[test]
fn root_relative_step_succeeds_from_root_cwd() {
    let root = stage_project("relstep-root", &step("file-length", ROOT_RELATIVE_STEP_CMD));
    let out = run_hook(&root, &root);
    assert_eq!(block_reason(&out), None, "cwd = root は従来どおり通る: {out}");
}

/// **good (fail-open 退行ガード)**: 本当に失敗する step は cwd ≠ root でも block すること。
///
/// 「cwd 正規化でゲートが黙って通るようになった」= T7 の修正がゲートを骨抜きにした、
/// という最悪の退行を固定する。
#[test]
fn failing_step_still_blocks_from_non_root_cwd() {
    let root = stage_project("failstep-nonroot", &step("test", "exit 1"));
    let out = run_hook(&root, &root.join(".takt").join("runs"));
    let reason = block_reason(&out).expect("実失敗する step は block されねばならない");
    assert!(reason.contains("test"), "失敗 step 名が reason に含まれる: {reason}");
}

/// **bad (incident 再現 / 症状 2 = takt subsession 判定の空振り)**:
/// cwd が `.takt/runs` でも active takt run を検知し、
/// 品質ゲートを skip すること (ADR-004 § takt subsession skip)。
///
/// step は必ず失敗する `exit 1` にしてあるため、skip されなければ block が出る。
/// 修正前は `<cwd>/.takt/runs` (= `.takt/runs/.takt/runs`) を探して空振りし、
/// edit: false の subsession に「直せ」を返していた。
#[test]
fn active_takt_run_skips_gate_from_non_root_cwd() {
    let root = stage_project("takt-active-nonroot", &step("test", "exit 1"));
    write_run_meta(&root, "active-run", "running");
    let out = run_hook(&root, &root.join(".takt").join("runs"));
    assert_eq!(
        block_reason(&out),
        None,
        "cwd ≠ root でも active takt run を検知して skip せねばならない (ADR-004): {out}"
    );
}

/// **good (過剰 skip ガード)**: active でない takt run では skip せず、通常どおり
/// 品質ゲートを実行すること。
#[test]
fn completed_takt_run_does_not_skip_gate_from_non_root_cwd() {
    let root = stage_project("takt-done-nonroot", &step("test", "exit 1"));
    write_run_meta(&root, "completed-run", "completed");
    let out = run_hook(&root, &root.join(".takt").join("runs"));
    assert!(
        block_reason(&out).is_some(),
        "completed run は skip 条件でない = ゲートは走る: {out}"
    );
}
