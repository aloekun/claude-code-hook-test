//! 引数を配列で直接渡す (シェルを挟まない) 実行の timeout 付き variant (順位 238)。
//!
//! `run_cmd_shell_*` はコマンド文字列をシェルに渡すが、`gh` / `jj` を引数配列のまま
//! 起動したい callsite (スペースを含む引数・機械可読出力のパース) には使えない。
//! 当初は `cli-pr-monitor` だけが同等の関数 (`run_cmd_capture`) を持っていたが、
//! `cli-merge-pipeline` の `gh` 呼び出しが timeout なしの `.output()` でネットワーク停止時に
//! 無期限ハングすることが分かり (PR #230 CodeRabbit Major、ADR-016 違反)、同じ部品が
//! 2 crate 目でも必要になったため本 lib へ移した ([ADR-044] 層 1)。
//!
//! [ADR-044]: ../../../docs/adr/adr-044-subprocess-utility-extraction-boundary.md

use super::{
    drain_pipe_unlimited, join_within, prefix_notice, wait_with_timeout_safe, JoinedOutput,
    EXIT_JOIN_GRACE_MS, JOIN_GRACE_MS, OUTPUT_PIPE_HELD_NOTICE,
};
use std::process::{Command, ExitStatus, Stdio};

/// [`run_cmd_direct_capture`] の結果。stdout / stderr を分離して保持する。
///
/// stdout を機械可読出力 (JSON 等) としてパースする呼び出しは本構造体を使い、
/// stderr の警告ログ混入でパースが壊れる事故 (PR #238 実観測) を構造的に防ぐ。
pub struct CmdCapture {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// `program` を引数配列 `args` で直接起動し、stdout / stderr を分離キャプチャして返す。
///
/// 戻り値:
/// - 起動失敗 → `ok = false`、`stderr` に `"Failed to execute ..."`
/// - timeout → `ok = false`、`timed_out = true` (child は子孫ごと kill + reap 済)
/// - try_wait 失敗 → `ok = false`、`stderr` の先頭行にエラー (child は子孫ごと kill + reap 済)
/// - 終了後に子孫がパイプを握り続けた → `ok = false`、`stderr` の先頭行に
///   [`OUTPUT_PIPE_HELD_NOTICE`] (理由は `run_cmd_shell_with` の doc を参照)
/// - それ以外 → `ok = status.success()`
///
/// 待機は `wait_with_timeout_safe` を使う。シェルを挟まないので通常は孫が生まれないが、
/// どの失敗経路でも child を残さない strict な variant を選んでおく。
pub fn run_cmd_direct_capture(program: &str, args: &[&str], timeout_secs: u64) -> CmdCapture {
    let mut child = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return CmdCapture {
                ok: false,
                stdout: String::new(),
                stderr: format!("Failed to execute {} {:?}: {}", program, args, e),
                timed_out: false,
            }
        }
    };

    let stdout_handle = drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let waited = wait_with_timeout_safe(program, &mut child, timeout_secs);
    let grace_ms = match waited {
        Ok(Some(_)) => EXIT_JOIN_GRACE_MS,
        _ => JOIN_GRACE_MS,
    };
    let joined = join_within(stdout_handle, stderr_handle, grace_ms);
    to_capture(waited, joined)
}

/// 待機結果と回収した出力を [`CmdCapture`] に落とす (I/O なし)。
fn to_capture(waited: Result<Option<ExitStatus>, String>, joined: JoinedOutput) -> CmdCapture {
    let (ok, stderr, timed_out) = match waited {
        Ok(Some(_)) if !joined.complete => (
            false,
            prefix_notice(OUTPUT_PIPE_HELD_NOTICE, &joined.stderr),
            false,
        ),
        Ok(Some(status)) => (status.success(), joined.stderr, false),
        Ok(None) => (false, joined.stderr, true),
        Err(e) => (false, prefix_notice(&e, &joined.stderr), false),
    };
    CmdCapture {
        ok,
        stdout: joined.stdout,
        stderr,
        timed_out,
    }
}
