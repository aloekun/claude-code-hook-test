//! lib-subprocess のテスト (production は lib.rs)。
//! ファイル長 800 行ガイドライン (順位 147) 遵守のため、順位 323 の
//! 回帰テスト追加にあたって test mod を切り出した。

use super::*;

#[test]
fn combine_output_both_present_inserts_newline() {
    assert_eq!(combine_output("out", "err"), "out\nerr");
}

#[test]
fn combine_output_only_stdout_returns_stdout() {
    assert_eq!(combine_output("out", ""), "out");
}

#[test]
fn combine_output_only_stderr_returns_stderr() {
    assert_eq!(combine_output("", "err"), "err");
}

#[test]
fn combine_output_both_empty_returns_empty() {
    assert_eq!(combine_output("", ""), "");
}

#[test]
fn combine_output_stdout_trailing_newline_does_not_insert_separator() {
    assert_eq!(combine_output("out\n", "err"), "out\nerr");
}

use std::process::Stdio;

/// timeout 経路を踏ませるための「十分に長く走る」コマンド (両 OS で約 10 秒、WP-15)。
///
/// `exit 0` / `exit 1` / `echo hello` は cmd.exe と POSIX sh の双方で同じ意味に
/// 解釈されるためテスト内に直接書けるが、「長時間走る」「N 行吐く」は構文が非互換
/// なので cfg で出し分ける。**振る舞い (所要時間 / 出力行数) は両 OS で揃えること** —
/// ここがズレると片方の OS だけ timeout / truncation を検証しない抜け穴になる。
#[cfg(windows)]
const LONG_RUNNING_CMD: &str = "ping 127.0.0.1 -n 10";
#[cfg(not(windows))]
const LONG_RUNNING_CMD: &str = "sleep 10";

/// capped variant の cap (40 行) を超える 60 行を吐くコマンド。
/// POSIX 側は `seq` が無い最小環境でも動くよう while ループで書く。
#[cfg(windows)]
const EMIT_60_LINES_CMD: &str = "(for /L %i in (1,1,60) do @echo line %i)";
#[cfg(not(windows))]
const EMIT_60_LINES_CMD: &str = "i=1; while [ $i -le 60 ]; do echo line $i; i=$((i+1)); done";

fn spawn_quick_exit() -> std::process::Child {
    shell_command("exit 0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn quick-exit command")
}

fn spawn_long_running() -> std::process::Child {
    shell_command(LONG_RUNNING_CMD)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn long-running command")
}

#[test]
fn wait_with_timeout_safe_returns_exit_status_on_quick_completion() {
    let mut child = spawn_quick_exit();
    let result = wait_with_timeout_safe("test", &mut child, 10).expect("safe wait failed");
    assert!(result.is_some(), "child should have exited cleanly");
    assert!(result.unwrap().success(), "exit 0 should report success");
}

#[test]
fn wait_with_timeout_safe_kills_child_on_timeout() {
    let mut child = spawn_long_running();
    let result = wait_with_timeout_safe("test", &mut child, 1).expect("safe wait failed");
    assert!(result.is_none(), "timeout path should return Ok(None)");
    assert!(
        child.try_wait().expect("try_wait after kill failed").is_some(),
        "child should be reaped after safe-variant timeout",
    );
}

#[test]
fn wait_with_timeout_basic_returns_exit_status_on_quick_completion() {
    let mut child = spawn_quick_exit();
    let result = wait_with_timeout_basic("test", &mut child, 10).expect("basic wait failed");
    assert!(result.is_some(), "child should have exited cleanly");
    assert!(result.unwrap().success(), "exit 0 should report success");
}

#[test]
fn wait_with_timeout_basic_kills_child_on_timeout() {
    let mut child = spawn_long_running();
    let result = wait_with_timeout_basic("test", &mut child, 1).expect("basic wait failed");
    assert!(result.is_none(), "timeout path should return Ok(None)");
    assert!(
        child.try_wait().expect("try_wait after kill failed").is_some(),
        "child should be reaped after basic-variant timeout",
    );
}

use std::io::Cursor;

#[test]
fn drain_pipe_unlimited_reads_entire_input_and_trims_trailing_whitespace() {
    let input = Cursor::new(b"line1\nline2\nline3\n".to_vec());
    let handle = drain_pipe_unlimited(input);
    assert_eq!(handle.join().unwrap(), "line1\nline2\nline3");
}

#[test]
fn drain_pipe_unlimited_preserves_long_output_without_truncation() {
    let input: String = (0..500).map(|i| format!("line{}\n", i)).collect();
    let expected: String = (0..500)
        .map(|i| format!("line{}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let handle = drain_pipe_unlimited(Cursor::new(input.into_bytes()));
    assert_eq!(handle.join().unwrap(), expected);
}

#[test]
fn drain_pipe_capped_truncates_silently_at_max_lines() {
    let input = Cursor::new(b"a\nb\nc\nd\ne\n".to_vec());
    let handle = drain_pipe_capped(input, 3);
    assert_eq!(handle.join().unwrap(), "a\nb\nc");
}

#[test]
fn drain_pipe_capped_returns_all_lines_when_under_cap() {
    let input = Cursor::new(b"only\ntwo\n".to_vec());
    let handle = drain_pipe_capped(input, 100);
    assert_eq!(handle.join().unwrap(), "only\ntwo");
}

#[test]
fn truncation_notice_uses_plural_form_for_multiple_lines() {
    assert_eq!(truncation_notice(2), "... (2 lines truncated)");
}

#[test]
fn truncation_notice_uses_singular_form_for_one_line() {
    assert_eq!(truncation_notice(1), "... (1 line truncated)");
}

#[test]
fn drain_pipe_capped_reporting_appends_truncation_summary_when_over_cap() {
    let input = Cursor::new(b"a\nb\nc\nd\ne\n".to_vec());
    let handle = drain_pipe_capped_reporting(input, 3);
    assert_eq!(handle.join().unwrap(), "a\nb\nc\n... (2 lines truncated)");
}

#[test]
fn drain_pipe_capped_reporting_omits_summary_when_within_cap() {
    let input = Cursor::new(b"a\nb\n".to_vec());
    let handle = drain_pipe_capped_reporting(input, 10);
    assert_eq!(handle.join().unwrap(), "a\nb");
}

#[test]
fn drain_pipe_capped_n_minus_1_keeps_all() {
    let input = Cursor::new(b"a\nb\nc\nd\n".to_vec());
    let handle = drain_pipe_capped(input, 5);
    assert_eq!(handle.join().unwrap(), "a\nb\nc\nd");
}

#[test]
fn drain_pipe_capped_n_keeps_all() {
    let input = Cursor::new(b"a\nb\nc\nd\ne\n".to_vec());
    let handle = drain_pipe_capped(input, 5);
    assert_eq!(handle.join().unwrap(), "a\nb\nc\nd\ne");
}

#[test]
fn drain_pipe_capped_n_plus_1_truncates_one() {
    let input = Cursor::new(b"a\nb\nc\nd\ne\nf\n".to_vec());
    let handle = drain_pipe_capped(input, 5);
    assert_eq!(handle.join().unwrap(), "a\nb\nc\nd\ne");
}

#[test]
fn drain_pipe_capped_reporting_n_minus_1_keeps_all_omits_summary() {
    let input = Cursor::new(b"a\nb\nc\nd\n".to_vec());
    let handle = drain_pipe_capped_reporting(input, 5);
    assert_eq!(handle.join().unwrap(), "a\nb\nc\nd");
}

#[test]
fn drain_pipe_capped_reporting_n_keeps_all_omits_summary() {
    let input = Cursor::new(b"a\nb\nc\nd\ne\n".to_vec());
    let handle = drain_pipe_capped_reporting(input, 5);
    assert_eq!(handle.join().unwrap(), "a\nb\nc\nd\ne");
}

#[test]
fn drain_pipe_capped_reporting_n_plus_1_truncates_one_appends_summary() {
    let input = Cursor::new(b"a\nb\nc\nd\ne\nf\n".to_vec());
    let handle = drain_pipe_capped_reporting(input, 5);
    assert_eq!(
        handle.join().unwrap(),
        "a\nb\nc\nd\ne\n... (1 line truncated)",
    );
}

#[test]
fn run_cmd_shell_capped_returns_true_on_exit_zero() {
    let (ok, _output) = run_cmd_shell_capped("test", "exit 0", 10, 40);
    assert!(ok, "exit 0 should report success");
}

#[test]
fn run_cmd_shell_capped_returns_false_on_exit_nonzero() {
    let (ok, _output) = run_cmd_shell_capped("test", "exit 1", 10, 40);
    assert!(!ok, "exit 1 should report failure");
}

#[test]
fn run_cmd_shell_capped_captures_stdout_within_cap() {
    let (ok, output) = run_cmd_shell_capped("test", "echo hello", 10, 40);
    assert!(ok);
    assert!(
        output.contains("hello"),
        "stdout should be captured: {:?}",
        output,
    );
}

/// timeout 1s のテストで制御が戻るまでに許す上限。`LONG_RUNNING_CMD` の寿命 (10s) より
/// 十分小さく取る。
///
/// 戻り値の文言だけを見る timeout テストは、制御が戻るのが遅れても通る — PR #436 で
/// 実際に穴を素通りさせた (T6 / PR #283 と同型)。よって経過時間も assert する (順位 481)。
const TIMEOUT_TEST_MAX_ELAPSED_SECS: u64 = 4;

/// `run` を実行し、1s の timeout として報告され、かつ上限内に制御が戻ったことを確かめる。
fn assert_times_out_promptly(label: &str, run: impl FnOnce() -> (bool, String)) {
    let started = std::time::Instant::now();
    let (ok, output) = run();
    let elapsed = started.elapsed();
    assert!(!ok, "{}: timeout should report failure", label);
    assert!(
        output.starts_with("timed out after 1s"),
        "{}: timeout message expected: {:?}",
        label,
        output,
    );
    assert!(
        elapsed.as_secs() < TIMEOUT_TEST_MAX_ELAPSED_SECS,
        "{}: timeout=1s なのに制御が戻るまで {:?} かかった",
        label,
        elapsed,
    );
}

#[test]
fn run_cmd_shell_capped_reports_timeout_with_message() {
    assert_times_out_promptly("capped", || {
        run_cmd_shell_capped("test", LONG_RUNNING_CMD, 1, 40)
    });
}

#[test]
fn run_cmd_shell_capped_reporting_returns_true_on_exit_zero() {
    let (ok, _output) = run_cmd_shell_capped_reporting("test", "exit 0", 10, 40);
    assert!(ok, "exit 0 should report success");
}

#[test]
fn run_cmd_shell_capped_reporting_reports_timeout_with_message() {
    assert_times_out_promptly("capped_reporting", || {
        run_cmd_shell_capped_reporting("test", LONG_RUNNING_CMD, 1, 40)
    });
}

#[test]
fn run_cmd_shell_unlimited_returns_true_on_exit_zero() {
    let (ok, _output) = run_cmd_shell_unlimited("test", "exit 0", 10);
    assert!(ok, "exit 0 should report success");
}

#[test]
fn run_cmd_shell_unlimited_returns_false_on_exit_nonzero() {
    let (ok, _output) = run_cmd_shell_unlimited("test", "exit 1", 10);
    assert!(!ok, "exit 1 should report failure");
}

/// 本 variant の存在理由: capped variant が silent truncate する行数を超えても
/// 全行が戻り値に残ること (= control flow 判定に使える)。
#[test]
fn run_cmd_shell_unlimited_preserves_output_beyond_the_capped_variant_cap() {
    let cmd = EMIT_60_LINES_CMD;
    let (ok, output) = run_cmd_shell_unlimited("test", cmd, 30);
    assert!(ok, "command should succeed: {:?}", output);
    assert_eq!(
        output.lines().count(),
        60,
        "all lines must survive; unlimited variant must not truncate: {:?}",
        output,
    );
}

#[test]
fn run_cmd_shell_unlimited_reports_timeout_with_message() {
    assert_times_out_promptly("unlimited", || {
        run_cmd_shell_unlimited("test", LONG_RUNNING_CMD, 1)
    });
}

/// 順位 323: timeout が孫プロセスに素通りする穴。
///
/// `shell_command` の child はシェルで、実際のコマンド (cargo / jj / ping) は**孫**。
/// timeout 時に kill されるのはシェルだけなので、孫はパイプの書き込み端を握ったまま
/// 生き残り、reader thread の `join()` が孫の自然終了までブロックする。
/// 既存の timeout テストは戻り値の文言しか見ていないため、**この穴を通過する**
/// (T6 / PR #283 と同じ「Err 内容だけの assert では素通りする」形)。
///
/// よって本 module は**経過時間を assert する**。
mod rank323_grandchild_outliving_the_shell {
    use super::*;
    use std::time::Instant;

    const TIMEOUT_SECS: u64 = 1;

    /// timeout 値 + 猶予。孫の寿命 (10s) より十分小さく取り、join でブロックしたら
    /// 必ず落ちるようにする。CI の負荷変動を吸収する余裕は残す。
    const MAX_ELAPSED_SECS: u64 = 4;

    /// 孫がパイプを握ったまま生き残るコマンド。
    ///
    /// **両 OS で「シェルが fork して孫を作る」形にすること**。`sh -c "sleep 10"` は
    /// 単一コマンドなので sh が `exec` に置き換わり孫が生まれず、Linux 側だけ
    /// この穴を検証しない抜け穴になる。複合コマンドにして fork を強制する。
    #[cfg(windows)]
    const GRANDCHILD_HOLDS_PIPE_CMD: &str = "ping 127.0.0.1 -n 10";
    #[cfg(not(windows))]
    const GRANDCHILD_HOLDS_PIPE_CMD: &str = "sleep 10; echo done";

    fn assert_returned_promptly(label: &str, elapsed: std::time::Duration) {
        assert!(
            elapsed.as_secs() < MAX_ELAPSED_SECS,
            "{}: timeout={}s なのに制御が戻るまで {:?} かかった。\
             孫プロセスがパイプを握っている間 join がブロックしている \
             (= ハング保護が実質無効)",
            label,
            TIMEOUT_SECS,
            elapsed,
        );
    }

    #[test]
    fn capped_returns_promptly_when_a_grandchild_holds_the_pipe() {
        let started = Instant::now();
        let (ok, output) =
            run_cmd_shell_capped("test", GRANDCHILD_HOLDS_PIPE_CMD, TIMEOUT_SECS, 40);
        let elapsed = started.elapsed();
        assert!(!ok, "timeout should report failure: {:?}", output);
        assert_returned_promptly("capped", elapsed);
    }

    #[test]
    fn capped_reporting_returns_promptly_when_a_grandchild_holds_the_pipe() {
        let started = Instant::now();
        let (ok, output) =
            run_cmd_shell_capped_reporting("test", GRANDCHILD_HOLDS_PIPE_CMD, TIMEOUT_SECS, 40);
        let elapsed = started.elapsed();
        assert!(!ok, "timeout should report failure: {:?}", output);
        assert_returned_promptly("capped_reporting", elapsed);
    }

    #[test]
    fn unlimited_returns_promptly_when_a_grandchild_holds_the_pipe() {
        let started = Instant::now();
        let (ok, output) =
            run_cmd_shell_unlimited("test", GRANDCHILD_HOLDS_PIPE_CMD, TIMEOUT_SECS);
        let elapsed = started.elapsed();
        assert!(!ok, "timeout should report failure: {:?}", output);
        assert_returned_promptly("unlimited", elapsed);
    }

    /// 正常終了は従来どおり (kill 経路が正常系を巻き込んでいない対照)。
    #[test]
    fn a_quick_command_still_returns_its_output_and_status() {
        let (ok, output) = run_cmd_shell_unlimited("test", "echo hello", 10);
        assert!(ok, "exit 0 should still succeed: {:?}", output);
        assert_eq!(output.trim(), "hello");
    }
}

/// 順位 481: **正常終了経路**でも reader thread の join が無制限だった穴。
///
/// 順位 323 は失敗経路 (timeout / wait 失敗) の join に上限を入れたが、child が
/// 自力で終了した経路は素の `join()` のまま残っていた。child がパイプの書き込み端を
/// 子孫へ継承させて終了すると (`cmd &` / `start /b`)、子孫が生きている間 EOF が
/// 来ず、join がブロックする — timeout の外側で起きるので **timeout では止まらない**。
///
/// 経過時間を assert する (戻り値だけを見るテストは PR #436 で穴を素通りさせた)。
///
/// ## 子孫を残したままテストを終えない
///
/// Windows では子孫が**テストプロセスの継承可能ハンドルも**受け継ぐ。テストを
/// `run_cmd_shell_*` 経由で起動した側 (push-runner の quality gate が `cargo test` を
/// 起動する) のパイプまで子孫が握るので、テストが先に終わると**呼び出し側が本修正の
/// 上限に当たって gate が落ちる** (初回 push で実際に落ちた)。よって各テストは
/// drain thread が EOF に達する = 子孫がパイプを離すまで待ってから終わる。
/// 待つために公開 variant ではなく共通骨格 `run_cmd_shell_with` を直接呼び、各
/// variant の drain を EOF 通知付きで包む。
mod rank481_descendant_outliving_a_clean_exit {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    /// timeout は十分長く取る。本テストの経路は timeout に達しないことが前提。
    const TIMEOUT_SECS: u64 = 60;

    /// 正常終了後の猶予 (3s) + CI の負荷変動の余裕。子孫の寿命 (約 8s) より小さく取り、
    /// join でブロックしたら必ず落ちるようにする。
    const MAX_ELAPSED_SECS: u64 = 6;

    /// 子孫がパイプを離すのを待つ上限。寿命 (約 8s) に十分な余裕を足した値。
    const RELEASE_WAIT_SECS: u64 = 30;

    /// 子孫にパイプを握らせたまま、シェル自身は exit 0 で即座に終わるコマンド。
    /// 子孫の寿命は両 OS で約 8s に揃える (`ping -n 9` は 8 秒)。
    #[cfg(windows)]
    const DESCENDANT_HOLDS_PIPE_CMD: &str = "start /b ping 127.0.0.1 -n 9 & exit 0";
    #[cfg(not(windows))]
    const DESCENDANT_HOLDS_PIPE_CMD: &str = "sleep 8 & exit 0";

    /// `drain` の結果を返す thread を、EOF に達したら `released` へ通知する thread で包む。
    fn notify_on_eof(drain: JoinHandle<String>, released: mpsc::Sender<()>) -> JoinHandle<String> {
        thread::spawn(move || {
            let output = drain.join().unwrap_or_default();
            let _ = released.send(());
            output
        })
    }

    /// `drain` 戦略で子孫の残るコマンドを走らせ、制御が戻るまでの時間と戻り値を検査し、
    /// 最後に stdout / stderr の両パイプが離されるまで待つ。
    fn assert_bounded_and_wait_for_release<D>(label: &str, drain: D)
    where
        D: Fn(Box<dyn Read + Send>) -> JoinHandle<String>,
    {
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        let (ok, output) =
            run_cmd_shell_with("test", DESCENDANT_HOLDS_PIPE_CMD, TIMEOUT_SECS, |pipe| {
                notify_on_eof(drain(pipe), tx.clone())
            });
        let elapsed = started.elapsed();
        drop(tx);

        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(RELEASE_WAIT_SECS))
                .expect("子孫プロセスがパイプを離さない (テスト後に孤児が残る)");
        }

        assert!(
            elapsed.as_secs() < MAX_ELAPSED_SECS,
            "{}: child は即座に終了したのに制御が戻るまで {:?} かかった。\
             子孫プロセスがパイプを握っている間 join がブロックしている",
            label,
            elapsed,
        );
        // 出力を読み切れていないのに成功と返すと、出力を control flow 判定に使う
        // callsite が空の出力で誤判定する。
        assert!(!ok, "{}: 出力を回収できていないのに成功扱い: {:?}", label, output);
        assert!(
            output.contains(OUTPUT_PIPE_HELD_NOTICE),
            "{}: 上限到達の理由が出力に無い: {:?}",
            label,
            output,
        );
    }

    #[test]
    fn capped_returns_promptly_when_a_descendant_outlives_a_clean_exit() {
        assert_bounded_and_wait_for_release("capped", |pipe| drain_pipe_capped(pipe, 40));
    }

    #[test]
    fn capped_reporting_returns_promptly_when_a_descendant_outlives_a_clean_exit() {
        assert_bounded_and_wait_for_release("capped_reporting", |pipe| {
            drain_pipe_capped_reporting(pipe, 40)
        });
    }

    #[test]
    fn unlimited_returns_promptly_when_a_descendant_outlives_a_clean_exit() {
        assert_bounded_and_wait_for_release("unlimited", drain_pipe_unlimited);
    }
}

// ─── run_cmd_direct_capture (順位 238) ───
//
// シェルを挟まない経路なので、長時間コマンドも `ping` / `sleep` を直接起動する。
// 標準出力と標準エラーの書き分けにはシェルの構文が要るため、そこだけ `cmd /C` / `sh -c`
// を program として渡す。

#[cfg(windows)]
const DIRECT_SHELL: (&str, &str) = ("cmd", "/C");
#[cfg(not(windows))]
const DIRECT_SHELL: (&str, &str) = ("sh", "-c");

#[cfg(windows)]
const DIRECT_LONG_RUNNING: (&str, &[&str]) = ("ping", &["127.0.0.1", "-n", "10"]);
#[cfg(not(windows))]
const DIRECT_LONG_RUNNING: (&str, &[&str]) = ("sleep", &["10"]);

fn run_direct_script(script: &str) -> CmdCapture {
    let (program, flag) = DIRECT_SHELL;
    run_cmd_direct_capture(program, &[flag, script], 30)
}

/// PR #238 regression: stderr の警告ログが stdout の機械可読出力に混入しない。
#[test]
fn run_cmd_direct_capture_separates_stdout_and_stderr() {
    let cap = run_direct_script("echo OUT&& echo ERR 1>&2");
    assert!(cap.ok, "stderr: {}", cap.stderr);
    assert!(!cap.timed_out);
    assert_eq!(cap.stdout.trim(), "OUT");
    assert_eq!(cap.stderr.trim(), "ERR");
}

#[test]
fn run_cmd_direct_capture_reports_nonzero_exit_as_failure() {
    let cap = run_direct_script("exit 1");
    assert!(!cap.ok);
    assert!(!cap.timed_out);
}

#[test]
fn run_cmd_direct_capture_reports_spawn_failure_via_stderr() {
    let cap = run_cmd_direct_capture("no-such-program-direct-238", &["x"], 5);
    assert!(!cap.ok);
    assert!(!cap.timed_out);
    assert!(cap.stdout.is_empty());
    assert!(
        cap.stderr.starts_with("Failed to execute no-such-program-direct-238"),
        "stderr: {:?}",
        cap.stderr
    );
}

/// timeout を報告するだけでなく、上限内に制御が戻ること (`assert_times_out_promptly` と同じ理由)。
#[test]
fn run_cmd_direct_capture_times_out_promptly() {
    let (program, args) = DIRECT_LONG_RUNNING;
    let started = std::time::Instant::now();
    let cap = run_cmd_direct_capture(program, args, 1);
    let elapsed = started.elapsed();
    assert!(!cap.ok);
    assert!(cap.timed_out);
    assert!(
        elapsed.as_secs() < TIMEOUT_TEST_MAX_ELAPSED_SECS,
        "timeout=1s なのに制御が戻るまで {:?} かかった",
        elapsed
    );
}
