//! cli-finding-classifier.exe を subprocess で起動し、diff を stdin に流して
//! lint-screen JSON を stdout から回収する層。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use lib_subprocess::wait_with_timeout_basic;

use super::{InvokeParams, STAGE};

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

    let mut child = spawn_classifier(params)?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(diff.as_bytes())
            .map_err(|e| format!("stdin 書き込み失敗: {}", e))?;
    }

    let stdout_handle = lib_subprocess::drain_pipe_capped(
        child.stdout.take().expect("stdout piped"),
        crate::runner::MAX_LINES,
    );
    let stderr_handle = lib_subprocess::drain_pipe_capped(
        child.stderr.take().expect("stderr piped"),
        crate::runner::MAX_LINES,
    );

    let exit = wait_with_timeout_basic(STAGE, &mut child, params.timeout_secs + 5)
        .map_err(|e| format!("wait 失敗: {}", e))?;
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match exit {
        None => Err(format!("timeout ({}s)", params.timeout_secs + 5)),
        Some(status) if !status.success() => Err(format!("非 0 終了: {}", stderr)),
        Some(_) if stdout.trim().is_empty() => Err("stdout 空".to_string()),
        Some(_) => Ok(ClassifierOutput { stdout, stderr }),
    }
}
