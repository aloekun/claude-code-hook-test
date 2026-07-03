//! cli-finding-classifier.exe を subprocess で起動し、diff を stdin に流して
//! lint-screen JSON を stdout から回収する層。
//!
//! Pipe orchestration は **drain-first** (stdout/stderr の drain thread を spawn
//! してから stdin へ書く)。順序を逆にすると、子プロセスが stdin 読込前に大量
//! 出力した際にパイプバッファ (~64KB) 満杯で親子相互ブロックの deadlock になる
//! (PR #231 CodeRabbit Major、`run_cmd_capture` / Safe Subprocess Stdout Pattern
//! と同型)。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::JoinHandle;

use lib_subprocess::wait_with_timeout_basic;

use super::{InvokeParams, STAGE};

#[derive(Debug)]
pub(super) struct ClassifierOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
}

/// classifier exe を lint-screen mode で spawn する (stdin/stdout/stderr は piped)。
fn spawn_classifier(params: &InvokeParams<'_>) -> Result<std::process::Child, String> {
    let timeout_str = params.timeout_secs.to_string();
    Command::new(params.exe)
        .args([
            "--mode",
            "lint-screen",
            "--model",
            params.model,
            "--endpoint",
            params.endpoint,
            "--timeout-secs",
            &timeout_str,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn 失敗: {}", e))
}

pub(super) fn invoke_classifier(
    params: &InvokeParams<'_>,
    diff: &str,
) -> Result<ClassifierOutput, String> {
    if !Path::new(params.exe).exists() {
        return Err(format!("exe 不在 ({})", params.exe));
    }

    let child = spawn_classifier(params)?;
    pump_child_io(child, diff, params.timeout_secs)
}

/// spawn 済みの子プロセスと stdin/stdout/stderr をやり取りする pipe orchestration。
///
/// drain thread を **stdin write より前に** spawn する (drain-first)。子が stdin
/// 読込前に stdout/stderr へ大量出力してもパイプが詰まらず、`write_all` が
/// ブロックしない。stdin はスコープ終了時の drop で閉じ、EOF を子へ通知する。
fn pump_child_io(
    mut child: std::process::Child,
    stdin_payload: &str,
    timeout_secs: u64,
) -> Result<ClassifierOutput, String> {
    let stdout_handle = lib_subprocess::drain_pipe_capped(
        child.stdout.take().expect("stdout piped"),
        crate::runner::MAX_LINES,
    );
    let stderr_handle = lib_subprocess::drain_pipe_capped(
        child.stderr.take().expect("stderr piped"),
        crate::runner::MAX_LINES,
    );

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(stdin_payload.as_bytes()) {
            abort_child(&mut child, stdout_handle, stderr_handle);
            return Err(format!("stdin 書き込み失敗: {}", e));
        }
    }

    let exit = match wait_with_timeout_basic(STAGE, &mut child, timeout_secs + 5) {
        Ok(v) => v,
        Err(e) => {
            abort_child(&mut child, stdout_handle, stderr_handle);
            return Err(format!("wait 失敗: {}", e));
        }
    };
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match exit {
        None => Err(format!("timeout ({}s)", timeout_secs + 5)),
        Some(status) if !status.success() => Err(format!("非 0 終了: {}", stderr)),
        Some(_) if stdout.trim().is_empty() => Err("stdout 空".to_string()),
        Some(_) => Ok(ClassifierOutput { stdout, stderr }),
    }
}

/// エラー経路で子プロセスと drain thread を回収する (孤児プロセスと detached
/// thread の残留防止)。kill 後は pipe が閉じるため join は速やかに返る。
fn abort_child(
    child: &mut std::process::Child,
    stdout_handle: JoinHandle<String>,
    stderr_handle: JoinHandle<String>,
) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn spawn_piped(program: &str, args: &[&str]) -> std::process::Child {
        Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn 失敗")
    }

    /// `cmd /C more` は stdin を EOF まで読んで stdout へ echo する。
    /// stdin write → EOF 通知 → stdout 回収の正常系を固定化する。
    #[test]
    #[cfg(windows)]
    fn pump_child_io_roundtrips_stdin_to_stdout() {
        let child = spawn_piped("cmd", &["/C", "more"]);
        let out = pump_child_io(child, "hello classifier", 30).expect("正常完走すべき");
        assert!(
            out.stdout.contains("hello classifier"),
            "stdout: {:?}",
            out.stdout
        );
    }

    /// 非 0 終了は Err(非 0 終了) に分類される。stdin payload は空にして
    /// 「子が stdin を読まずに即終了 → broken pipe」の race を排除する。
    #[test]
    #[cfg(windows)]
    fn pump_child_io_reports_nonzero_exit() {
        let child = spawn_piped("cmd", &["/C", "exit 3"]);
        let err = pump_child_io(child, "", 30).expect_err("非 0 終了は Err のはず");
        assert!(err.contains("非 0 終了"), "err: {}", err);
    }

    /// exit 0 かつ stdout 空は Err(stdout 空) に分類される。
    #[test]
    #[cfg(windows)]
    fn pump_child_io_reports_empty_stdout() {
        let child = spawn_piped("cmd", &["/C", "exit 0"]);
        let err = pump_child_io(child, "", 30).expect_err("stdout 空は Err のはず");
        assert!(err.contains("stdout 空"), "err: {}", err);
    }

    /// タイムアウト経路 (`None` 分岐) の regression test。
    /// 子プロセスが `timeout_secs + 5` 秒以内に終了しない場合に `Err("timeout (Ns)")`
    /// を返すことを確認する。`timeout_secs=1` を渡すと内部で 6s タイムアウトが発火する。
    #[test]
    #[cfg(windows)]
    #[ignore = "integration: tests timeout behavior (~6s actual wait); run via `cargo test -- --ignored --test-threads=1`"]
    fn pump_child_io_reports_timeout_when_child_exceeds_deadline() {
        let child = spawn_piped(
            "powershell",
            &["-NoProfile", "-Command", "Start-Sleep -Seconds 10"],
        );
        let err = pump_child_io(child, "", 1).expect_err("タイムアウトは Err のはず");
        assert!(err.contains("timeout"), "err: {}", err);
        assert!(err.contains("6s"), "err: {}", err);
    }

    /// PR #231 CodeRabbit Major の regression test: 子プロセスが stdin を読む前に
    /// パイプバッファ (~64KB) を大きく超える stdout (~256KB) を吐いても deadlock
    /// しないこと (drain-first の検証)。修正前の順序 (stdin write → drain spawn)
    /// ではこのテストは `write_all` でハングし (実測 78 分継続を確認)、push gate
    /// の step_timeout が FAIL として検出する。
    #[test]
    #[cfg(windows)]
    #[ignore = "integration: spawns real process with ~256KB stdout + ~1MB stdin; run via `cargo test -- --ignored --test-threads=1`"]
    fn pump_child_io_survives_child_flooding_stdout_before_reading_stdin() {
        let flood_script = "$d = 'x' * 8192; \
             for ($i = 0; $i -lt 32; $i++) { Write-Output $d }; \
             [Console]::In.ReadToEnd() | Out-Null; \
             Write-Output 'DRAINED'";
        let child = spawn_piped("powershell", &["-NoProfile", "-Command", flood_script]);

        let large_diff = "y".repeat(1_000_000);
        let out = pump_child_io(child, &large_diff, 60).expect("deadlock せず完走すべき");
        assert!(
            out.stdout.contains("DRAINED"),
            "子プロセスが stdin を消費し切って完走していること (stdout 末尾): {:?}",
            out.stdout.chars().rev().take(60).collect::<String>()
        );
    }
}
