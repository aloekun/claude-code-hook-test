//! hooks smoke test (ADR-065) — PreToolUse の block/pass verdict を実 exe で検証する。
//!
//! `hooks-pre-tool-validate` はリポジトリで唯一「Claude の操作を実際に止める」hook
//! (exit 2 = block) でありながら、これまで unit test しか無く **exe を通る経路
//! (stdin JSON parse -> config -> preset/protected -> stderr + exit code)** は無検証だった。
//! CI matrix (windows-latest / ubuntu-latest) の目的は「片方の OS でしか通らない経路」を
//! 露出させることなので、両 OS で同一の verdict が出ることを機械検証する土台としてここに置く。
//!
//! 設計は ADR-049 の incident-eval スイートに倣う:
//! - **1 case = 1 failure mode**、かつ block (fire すべき入力) と pass (fire してはならない
//!   入力) を対で持つ。検出退行と false positive 退行の両方を止める。
//! - assert は **exit code と stderr の有無のみ**に限定する。block メッセージ本文は
//!   固定しない (文言修正でテストが壊れないようにする)。
//!
//! **exe は temp dir へ staging してから spawn する** (ADR-010 の実配置を再現する
//! `t7_cwd_independence` と同方式)。この hook は config を `current_exe()` の隣から
//! 解決するため、`target/debug` の exe を直接叩くと「そこに残っている config」に
//! verdict が左右され、fresh clone の CI とローカルで結果が食い違う。staging により
//! **deploy 済 config で判定する**ことを固定し、同時に `target/debug` を汚して
//! 他 crate の exe-spawn テストへ干渉することも防ぐ。
//!
//! `.env` / `rm -rf` 等の値は synthetic な test data であり、実在のパス・実行対象ではない。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// spawn した hook exe の bounded wait (dev-conventions.md § bounded wait)。
/// ハングした子プロセスは kill してテストを失敗させ、CI を無期限に止めない。
const HOOK_TIMEOUT_SECS: u64 = 30;

/// PreToolUse の block を表す exit code (main.rs のドキュメント参照)。
const EXIT_BLOCK: i32 = 2;
/// PreToolUse の許可を表す exit code。
const EXIT_PASS: i32 = 0;

/// 1 つの failure mode に対する block/pass 期待。
struct Case {
    /// 失敗時に「どの経路が壊れたか」が判るラベル。
    name: &'static str,
    /// hook JSON の `tool_name`。
    tool_name: &'static str,
    /// `tool_input` に載せるキー (`command` or `file_path`)。
    field: &'static str,
    /// `field` の値 (synthetic test data)。
    value: &'static str,
    /// true = exit 2 + stderr にメッセージ、false = exit 0 + stderr 空。
    expect_block: bool,
}

/// 通す経路ごとに block/pass を 1 対ずつ持つ:
/// preset 経路 (`blocked_patterns`、default preset の rm -rf ガード)、
/// protected_files 経路 (機密ファイルの Write ブロック)、
/// `main.rs` の match default arm (未知の tool は素通し = 過剰ブロックの退行ガード)。
const CASES: &[Case] = &[
    Case {
        name: "Bash: rm -rf を block する",
        tool_name: "Bash",
        field: "command",
        value: "rm -rf /tmp/smoke-test-target",
        expect_block: true,
    },
    Case {
        name: "Bash: 無害なコマンドは pass する",
        tool_name: "Bash",
        field: "command",
        value: "ls -la",
        expect_block: false,
    },
    Case {
        name: "Write: 保護対象ファイルを block する",
        tool_name: "Write",
        field: "file_path",
        value: "/srv/example-project/.env",
        expect_block: true,
    },
    Case {
        name: "Write: 通常のファイルは pass する",
        tool_name: "Write",
        field: "file_path",
        value: "/srv/example-project/docs/notes.md",
        expect_block: false,
    },
    Case {
        name: "未知の tool は pass する",
        tool_name: "Read",
        field: "file_path",
        value: "/srv/example-project/docs/notes.md",
        expect_block: false,
    },
];

fn built_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hooks-pre-tool-validate"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// exe と deploy 済 `hooks-config.toml` を temp dir へ配置し、staging 先の exe パスを返す。
/// 返り値の `TempDir` は生存させ続けること (drop で削除される)。
fn stage_hook() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("create temp dir");

    let exe_name = built_exe()
        .file_name()
        .expect("built exe has a file name")
        .to_owned();
    let staged_exe = tmp.path().join(exe_name);
    std::fs::copy(built_exe(), &staged_exe).expect("stage hook exe");

    let config_src = repo_root().join(".claude").join("hooks-config.toml");
    assert!(
        config_src.exists(),
        "deployed hooks-config.toml missing at {} (false-green guard)",
        config_src.display()
    );
    std::fs::copy(&config_src, tmp.path().join("hooks-config.toml"))
        .expect("stage hooks-config.toml");

    (tmp, staged_exe)
}

/// staging 済み hook exe を spawn し `(exit code, stderr)` を返す。
///
/// telemetry の kill-switch を立てるのは、staging した config が `[telemetry]` を
/// enable していても書き込みを起こさないため (テストの副作用を config に依存させない)。
fn run_hook(exe: &Path, payload: &str) -> (i32, String) {
    let mut child = Command::new(exe)
        .env("CLAUDE_TELEMETRY_DISABLE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hooks-pre-tool-validate");
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
    let _stdout = stdout_drain.join().expect("stdout drain thread panicked");
    let stderr = stderr_drain.join().expect("stderr drain thread panicked");
    let status = status.unwrap_or_else(|| {
        panic!("hooks-pre-tool-validate hung > {HOOK_TIMEOUT_SECS}s (killed) — investigate")
    });
    let code = status
        .code()
        .expect("hook exited via signal instead of an exit code");
    (code, stderr)
}

#[test]
fn pre_tool_validate_block_pass_verdicts() {
    let (_keep, exe) = stage_hook();
    for case in CASES {
        let payload = serde_json::json!({
            "tool_name": case.tool_name,
            "tool_input": { case.field: case.value },
        })
        .to_string();
        let (code, stderr) = run_hook(&exe, &payload);

        if case.expect_block {
            assert_eq!(
                code, EXIT_BLOCK,
                "{}: block されるべき入力が exit {} で通過した (stderr: {})",
                case.name, code, stderr
            );
            assert!(
                !stderr.is_empty(),
                "{}: block したが stderr が空 (Claude に理由が伝わらない)",
                case.name
            );
        } else {
            assert_eq!(
                code, EXIT_PASS,
                "{}: pass すべき入力が exit {} で止められた (false positive、stderr: {})",
                case.name, code, stderr
            );
            assert!(
                stderr.is_empty(),
                "{}: pass したのに stderr へ出力した: {}",
                case.name,
                stderr
            );
        }
    }
}

/// 不正な stdin でも panic せず、かつ **block 側に倒れない**ことを確認する。
///
/// PreToolUse は全ツール呼び出しの前段に居るため、ここが exit 2 に倒れると Claude の
/// 操作が全面的に止まる。JSON parse 失敗は `ExitCode::FAILURE` (=1) で、Claude Code は
/// これを block として扱わない。
#[test]
fn malformed_stdin_does_not_block() {
    let (_keep, exe) = stage_hook();
    let (code, _stderr) = run_hook(&exe, "{ this is not json");
    assert_ne!(
        code, EXIT_BLOCK,
        "不正な stdin が block (exit 2) になった — 全ツール呼び出しを止めうる"
    );
}
