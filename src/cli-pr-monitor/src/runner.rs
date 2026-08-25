use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use lib_subprocess::drain_pipe_unlimited;

const POLL_INTERVAL_MS: u64 = 100;
pub(crate) const JJ_CMD_TIMEOUT_SECS: u64 = 30;

/// [`run_cmd_capture`] の結果。stdout / stderr を分離して保持する。
///
/// stdout を機械可読出力 (JSON 等) としてパースする呼び出しは本構造体を使い、
/// stderr の警告ログ混入でパースが壊れる事故 (PR #238 実観測) を構造的に防ぐ。
pub(crate) struct CmdCapture {
    pub(crate) ok: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
}

/// 引数を配列で直接渡し、stdout / stderr を分離キャプチャして返す。
pub(crate) fn run_cmd_capture(
    program: &str,
    fixed_args: &[&str],
    extra_args: &[String],
    timeout_secs: u64,
) -> CmdCapture {
    let mut child = match Command::new(program)
        .args(fixed_args)
        .args(extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return CmdCapture {
                ok: false,
                stdout: String::new(),
                stderr: format!("Failed to execute {} {:?}: {}", program, fixed_args, e),
                timed_out: false,
            }
        }
    };

    let stdout_handle = drain_pipe_unlimited(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().unwrap());

    let timed_out = wait_child_with_deadline(&mut child, timeout_secs);

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    if timed_out {
        return CmdCapture {
            ok: false,
            stdout,
            stderr,
            timed_out: true,
        };
    }

    let code = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    CmdCapture {
        ok: code == 0,
        stdout,
        stderr,
        timed_out: false,
    }
}

/// 子プロセスを deadline 付きで待機する。timeout 到達時は kill して `true` を返す。
/// try_wait の失敗も timeout 扱い (fail-safe 方向) に倒す。
fn wait_child_with_deadline(child: &mut std::process::Child, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(_) => break true,
        }
    }
}

/// 引数を配列で直接渡す版（スペースを含む引数を正しくハンドリング）。
///
/// stdout / stderr を結合した文字列を返す従来 API。機械可読出力をパースする
/// 用途には [`run_cmd_capture`] を使うこと (stderr 混入でパースが壊れるため)。
pub(crate) fn run_cmd_direct(
    program: &str,
    fixed_args: &[&str],
    extra_args: &[String],
    timeout_secs: u64,
) -> (bool, String) {
    let cap = run_cmd_capture(program, fixed_args, extra_args, timeout_secs);
    let combined = format!("{}{}", cap.stdout, cap.stderr).trim().to_string();

    if cap.timed_out {
        return (
            false,
            format!("{}\n(timeout after {}s)", combined, timeout_secs),
        );
    }
    (cap.ok, combined)
}

/// gh コマンドを静かに実行 (stderr 抑制)
pub(crate) fn run_gh_quiet(args: &[&str]) -> Option<String> {
    let output = Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

// ─── jj CLI ヘルパー ───

/// 現在の working copy (`@`) の commit id を取得する。
/// 失敗時は `None` を返し、呼び出し側で fail-safe に扱う。
pub(crate) fn capture_commit_id() -> Option<String> {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
        &[],
        10,
    );
    if !ok {
        crate::log::log_info(&format!("[state] capture_commit_id 失敗: {}", out.trim()));
        return None;
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// `from` と `to` の間の diff が空か判定する。
/// jj コマンドが失敗した場合は `true` (空扱い = NoChange = push しない) を返す。
/// capture_commit_id と同じ fail-closed 方向に揃えることで誤 push を防ぐ。
pub(crate) fn diff_is_empty(from: &str, to: &str) -> bool {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["diff", "--from", from, "--to", to, "--stat"],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        crate::log::log_info(&format!(
            "[state] diff_is_empty 判定失敗 (空として扱い push をスキップ): {}",
            out.trim()
        ));
        return true;
    }
    out.trim().is_empty()
}

/// `from` から `@` までの変更ファイル一覧 (`jj diff --summary`) を取る (I/O のみ)。
///
/// **失敗を「空」に潰さない。** 呼び手によって失敗の扱いが逆になる — scope guard は
/// fail-closed で block ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md)
/// のゲート関数)、takt の作業ツリー変更判定は「判定不能」として助言に留める (助言層)。
/// どちらに倒すかは呼び手が決められるよう `Result` のまま返す。
///
/// **成功時は stdout だけを返す。** [`run_cmd_direct`] は stdout と stderr を結合するため、
/// jj が警告を 1 行出しただけで「差分あり」に見える。呼び手の
/// [`crate::stages::monitor`] はこの非空を「takt が作業ツリーを変更した」と読むので、
/// 結合したままだと**順位 490 の誤警告 (と `jj restore` の案内) がそのまま再発する**
/// (CodeRabbit #446)。失敗時だけ stderr と timeout 情報を結合して診断に回す。
pub(crate) fn capture_diff_summary(from: &str) -> Result<String, String> {
    interpret_capture(
        run_cmd_capture(
            "jj",
            &["diff", "--from", from, "--to", "@", "--summary"],
            &[],
            JJ_CMD_TIMEOUT_SECS,
        ),
        JJ_CMD_TIMEOUT_SECS,
    )
}

/// [`CmdCapture`] を「成功なら stdout だけ / 失敗なら診断文字列」に落とす (I/O なし)。
///
/// **stdout と stderr を混ぜるかどうかの分かれ目がここ 1 箇所に集まる。** 判定に使う値へ
/// stderr を混ぜると呼び手が警告を差分と読むため、成功経路では必ず落とす (CodeRabbit #446)。
fn interpret_capture(cap: CmdCapture, timeout_secs: u64) -> Result<String, String> {
    if cap.ok {
        return Ok(cap.stdout);
    }
    Err(failure_detail(&cap, timeout_secs))
}

/// 失敗した [`CmdCapture`] を 1 本の診断文字列にする (I/O なし)。
fn failure_detail(cap: &CmdCapture, timeout_secs: u64) -> String {
    let combined = format!("{}{}", cap.stdout, cap.stderr).trim().to_string();
    if cap.timed_out {
        format!("{combined}\n(timeout after {timeout_secs}s)")
    } else {
        combined
    }
}

/// 現在の `@` が empty commit (親との差分なし) か判定する。
///
/// `diff_is_empty` は re-push 判定 (fail-safe で「空扱い → push しない」方向) だが、
/// こちらは **abandon 判定**用であり方向が逆: 失敗時は `false` (= diff あり扱い)
/// を返して abandon を見送る。
///
/// 理由: jj コマンド失敗時にうっかり `jj abandon` を走らせると、takt が部分的に
/// amend した child commit ごと消えるリスクがある。「判定不能なら何もしない」方向
/// に倒す。
///
/// 実装: `jj diff --stat` は空 commit でも "0 files changed, ..." のような
/// サマリ行を出力するため空判定に使えない。代わりに jj の `empty` テンプレート
/// keyword を使い、"true"/"false" の明示出力で判定する。
///
/// **これは「`@` が空コミットか」であって「直前の操作が作業ツリーを変えたか」ではない。**
/// 前提が成り立つのは `@` が空である文脈 (fix commit の後始末) だけで、feature ブランチの
/// `@` が PR の中身そのものである経路で before/after の代わりに使うと必ず「非空」になる
/// (順位 490 の誤警告)。前後比較が要る場合は [`diff_is_empty`] に基準コミットを渡すこと。
pub(crate) fn diff_at_is_empty() -> bool {
    let raw = query_at_emptiness();
    if let Err(reason) = &raw {
        crate::log::log_info(&format!(
            "[state] diff_at_is_empty 判定失敗 (diff あり扱いで abandon をスキップ): {reason}"
        ));
    }
    interpret_at_emptiness(raw)
}

/// jj へ問い合わせて `@` の empty 判定を生の文字列で取る (I/O のみ、判定を含まない)。
///
/// 失敗は `Err(理由)`。呼び手が理由をログに出せるよう握り潰さない。
///
/// [`capture_diff_summary`] と同じ理由で**成功時は stdout だけを返す** — jj が警告を出すと
/// 結合出力は `"true"` と一致しなくなり、空の `@` を「diff あり」と読んで abandon を
/// 取りこぼす (CodeRabbit #446 と同型)。
fn query_at_emptiness() -> Result<String, String> {
    interpret_capture(
        run_cmd_capture(
            "jj",
            &[
                "log",
                "-r",
                "@",
                "--no-graph",
                "-T",
                "if(empty, \"true\", \"false\")",
            ],
            &[],
            JJ_CMD_TIMEOUT_SECS,
        ),
        JJ_CMD_TIMEOUT_SECS,
    )
}

/// `@` の empty 判定 (I/O なし。ログ出力も呼び出し元に置く)。
///
/// 判定不能 (jj 失敗 / 未知の出力) はすべて `false` = 「diff あり扱い」に倒す。
/// `true` に倒すと `jj abandon` が走り、takt が部分的に amend した child commit ごと
/// 消えるため、「判定不能なら何もしない」向きが安全側になる。
fn interpret_at_emptiness(raw: Result<String, String>) -> bool {
    matches!(raw, Ok(out) if out.trim() == "true")
}

/// takt ワークフロー実行のデフォルトタイムアウト (10 分)
const TAKT_TIMEOUT_SECS: u64 = 600;

/// stdio を継承してコマンドを実行する (takt 呼び出し用、タイムアウト付き)
pub(crate) fn run_cmd_inherit(label: &str, program: &str, args: &[&str]) -> bool {
    crate::log::log_info(&format!("{}: {} {}", label, program, args.join(" ")));
    let mut child = match Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            crate::log::log_info(&format!("{} の起動に失敗: {}", label, e));
            return false;
        }
    };

    let deadline = std::time::Instant::now() + Duration::from_secs(TAKT_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    crate::log::log_info(&format!(
                        "{} タイムアウト ({}秒)",
                        label, TAKT_TIMEOUT_SECS
                    ));
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => {
                crate::log::log_info(&format!("{} の待機に失敗: {}", label, e));
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// check-ci-coderabbit 実行ファイルのパスを解決する (cli-pr-monitor と同 dir 前提)。
///
/// 実行ファイル拡張子は OS 依存 (Windows: `.exe` / それ以外: なし) のため
/// `std::env::consts::EXE_SUFFIX` で解決する (WP-13: EXE_SUFFIX 抽象化)。
pub(crate) fn checker_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("check-ci-coderabbit{}", std::env::consts::EXE_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PR #238 regression: stderr の警告ログが stdout の機械可読出力に
    /// 混入しないことを分離キャプチャで保証する。
    #[test]
    #[cfg(windows)]
    fn run_cmd_capture_separates_stdout_and_stderr() {
        let cap = run_cmd_capture("cmd", &["/C", "echo OUT& echo ERR 1>&2"], &[], 30);
        assert!(cap.ok, "stderr: {}", cap.stderr);
        assert!(!cap.timed_out);
        assert!(cap.stdout.contains("OUT"), "stdout: {:?}", cap.stdout);
        assert!(!cap.stdout.contains("ERR"), "stdout: {:?}", cap.stdout);
        assert!(cap.stderr.contains("ERR"), "stderr: {:?}", cap.stderr);
    }

    #[test]
    fn run_cmd_capture_spawn_failure_reports_via_stderr_field() {
        let cap = run_cmd_capture("no-such-program-gitdir-251", &[], &[], 5);
        assert!(!cap.ok);
        assert!(!cap.timed_out);
        assert!(cap.stdout.is_empty());
        assert!(cap.stderr.contains("Failed to execute"));
    }

    /// jj が `true` を返したときだけ空と判定する。
    #[test]
    fn at_emptiness_is_true_only_for_the_explicit_true_output() {
        assert!(interpret_at_emptiness(Ok("true\n".to_string())));
        assert!(!interpret_at_emptiness(Ok("false\n".to_string())));
    }

    /// **未知の出力は空と判定しない** — `jj abandon` が走ると child commit ごと消えるため、
    /// 曖昧さは「diff あり扱い = 何もしない」側へ倒す。
    #[test]
    fn an_unrecognised_output_is_not_treated_as_empty() {
        assert!(!interpret_at_emptiness(Ok(String::new())));
        assert!(!interpret_at_emptiness(Ok("TRUE".to_string())));
    }

    /// jj の失敗も同じく「diff あり扱い」。
    #[test]
    fn a_failed_query_is_not_treated_as_empty() {
        assert!(!interpret_at_emptiness(Err("jj: command not found".to_string())));
    }

    /// **stderr が差分要約に混ざらないこと** (CodeRabbit #446)。混ざると jj の警告 1 行で
    /// 「takt が作業ツリーを変更した」= 順位 490 の誤警告と `jj restore` の案内が再発する。
    ///
    /// `capture_diff_summary` / `query_at_emptiness` は jj を呼ぶので、ここでは同じ
    /// `run_cmd_capture` を別コマンドで叩き、**stdout と stderr が分離されている**ことを
    /// 押さえる (両関数はこの分離にそのまま乗っている)。
    #[test]
    #[cfg(windows)]
    fn a_summary_style_capture_keeps_stderr_out_of_stdout() {
        let cap = run_cmd_capture("cmd", &["/C", "echo WARNING 1>&2"], &[], 30);
        assert!(cap.ok, "stderr: {}", cap.stderr);
        assert!(
            cap.stdout.trim().is_empty(),
            "stderr だけの出力で stdout が非空になってはいけない: {:?}",
            cap.stdout
        );
        assert!(cap.stderr.contains("WARNING"));
    }

    /// **成功時は stderr を落とす** (CodeRabbit #446)。ここを混ぜると jj の警告 1 行が
    /// 「差分あり」に化け、順位 490 の誤警告と `jj restore` の案内が再発する。
    #[test]
    fn a_successful_capture_returns_stdout_without_stderr() {
        let cap = CmdCapture {
            ok: true,
            stdout: String::new(),
            stderr: "Warning: unrecognized config option\n".to_string(),
            timed_out: false,
        };
        assert_eq!(
            interpret_capture(cap, 60),
            Ok(String::new()),
            "stderr の警告だけで非空を返してはいけない"
        );
    }

    /// 成功時の stdout はそのまま (trim もしない — 呼び手が判定する)。
    #[test]
    fn a_successful_capture_preserves_stdout_verbatim() {
        let cap = CmdCapture {
            ok: true,
            stdout: "M src/lib.rs\n".to_string(),
            stderr: "noise".to_string(),
            timed_out: false,
        };
        assert_eq!(interpret_capture(cap, 60), Ok("M src/lib.rs\n".to_string()));
    }

    /// 失敗時は stderr も timeout も診断へ回す (握り潰さない)。
    #[test]
    fn a_failed_capture_surfaces_stderr() {
        let cap = CmdCapture {
            ok: false,
            stdout: String::new(),
            stderr: "jj: no such revision".to_string(),
            timed_out: false,
        };
        let err = interpret_capture(cap, 60).unwrap_err();
        assert!(err.contains("no such revision"), "{err}");
    }

    /// 失敗時は stdout / stderr / timeout をまとめて診断に回す。
    #[test]
    fn the_failure_detail_merges_stdout_stderr_and_timeout() {
        let cap = CmdCapture {
            ok: false,
            stdout: "partial\n".to_string(),
            stderr: "boom".to_string(),
            timed_out: false,
        };
        assert_eq!(failure_detail(&cap, 60), "partial\nboom");

        let timed_out = CmdCapture {
            timed_out: true,
            ..cap
        };
        let detail = failure_detail(&timed_out, 60);
        assert!(detail.contains("boom"), "{detail}");
        assert!(detail.contains("timeout after 60s"), "{detail}");
    }

    #[test]
    #[cfg(windows)]
    fn run_cmd_direct_keeps_combined_output_compatibility() {
        let (ok, combined) = run_cmd_direct("cmd", &["/C", "echo OUT& echo ERR 1>&2"], &[], 30);
        assert!(ok);
        assert!(combined.contains("OUT"));
        assert!(combined.contains("ERR"));
    }
}

