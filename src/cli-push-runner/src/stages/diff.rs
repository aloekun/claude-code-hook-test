//! Diff stage — `[diff] command` の出力を reviewers 用ファイルに書き出す。
//!
//! 出力は takt の reviewers が Read で参照するレビュー対象そのもののため、
//! **切り詰めない** (`run_diff_cmd` の doc)。実行は timeout 付き (T6)。

use std::path::Path;
use std::process::Stdio;

use lib_subprocess::{drain_pipe_unlimited, shell_command, wait_with_timeout_safe};

use crate::config::{DiffConfig, DEFAULT_DIFF_TIMEOUT_SECS};
use crate::log::log_stage;

#[derive(Debug, PartialEq)]
pub(crate) enum DiffResult {
    /// diff に内容があり、ファイルへの書き出しが完了した
    HasContent,
    /// diff 出力が空 (レビュー対象なし、push は続行可能)
    Empty,
    /// diff コマンドの実行またはファイル書き出しに失敗した
    Error,
}

/// diff 取得専用: 出力を切り詰めず、stdout / stderr を分離したまま timeout 付きで取得する。
///
/// 戻り値: `Ok(stdout)` / `Err(stderr | timeout メッセージ | 起動失敗メッセージ)`。
///
/// **stdout と stderr を結合しない**のが本関数の要件で、`lib_subprocess::run_cmd_shell_*`
/// (全 variant が `combine_output` で結合する) を使えない理由でもある。stdout は
/// reviewers が読む diff そのものとしてファイルに書かれるため、jj が stderr に出す警告
/// (並列 workspace 運用時の `Concurrent modification detected` 等) が混入すると
/// レビュー対象を汚す。読み取り戦略は cap なし (diff は全量が必要) で、shell 経由なのは
/// `[diff] command` が config 由来の文字列だから。同型の「全量 + 分離 + timeout」は
/// `bookmark_check::run_jj_bookmark_list` にもあるが、そちらは direct args で
/// signature が非互換のため共通化しない (ADR-044 層 1)。
///
/// timeout (T6): 旧実装は `Command::output()` で**無限待ち**だった。ADR-045 の並列
/// workspace 運用で jj の lock 競合が起きるとパイプラインが無言ハングする
/// (他 stage は全て timeout 付きで、diff だけが穴だった)。timeout 時は Err を返し、
/// 呼び出し側が `DiffResult::Error` = exit 5 で中断する (fail-closed / ADR-043)。
///
/// child の lifecycle: timeout 経路・try_wait 失敗経路とも `wait_with_timeout_safe` が
/// child を kill + reap する (`_basic` ではなく `_safe` を選ぶ理由 = ADR-044 層 2)。
///
/// **child を kill した 2 経路 (timeout / wait 失敗) では reader thread を join しない**。
/// `shell_command` の child はシェル (cmd.exe / sh) で、その孫 (実際の `jj` 等) は
/// kill の対象外になり得る (cmd.exe は常に、sh も複合コマンドを fork した場合)。
/// 孫は pipe の書き込み端を継承したまま生き残るため EOF が来ず、join すると孫が自然終了する
/// までブロックする = timeout が意味を成さない (T6 が直そうとしているハングの再生産)。
/// 実測: 9s 走るコマンドに 1s の timeout を設定し join すると、制御が戻るまで 9.6s 掛かった。
/// よってこの 2 経路では thread を detach して即座に返す (push-runner は直後に exit 5 で
/// 終了するため thread は道連れになる)。出力も不要 (診断は timeout メッセージ自身が持つ)。
/// 子が自力で終了した経路 (exit 0 / 非 0) は pipe が閉じるため join してよい。
fn run_diff_cmd(cmd: &str, timeout_secs: u64) -> Result<String, String> {
    let mut child = shell_command(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute {}: {}", cmd, e))?;

    let stdout_handle = drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status = wait_with_timeout_safe("diff", &mut child, timeout_secs)
        .map_err(|e| format!("diff コマンドの wait に失敗: {}", e))?;

    let Some(status) = status else {
        return Err(format!(
            "diff コマンドがタイムアウトしました ({}s): {}\n\
             jj の lock 競合 (並列 workspace 実行中の別 jj プロセス) を疑ってください。\
             大 diff で恒常的に超過する場合は `[diff] timeout` を延長してください。",
            timeout_secs, cmd,
        ));
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    if status.success() {
        Ok(stdout)
    } else {
        Err(stderr)
    }
}

/// takt 実行後の diff snapshot を取得する (T12 post-takt re-gate の変化検出用)。
///
/// Stage 1.5 と同じ `[diff] command` を再実行し stdout を返す。呼び出し側 (re-gate) は
/// takt 起動前に保持した snapshot と本値を**前後比較**し、一致すれば「fix はコードを
/// 書き換えていない」= re-gate skip に倒す。取得失敗 (jj 失敗 / timeout) は `None` を返し、
/// 呼び出し側が fail-closed (= 変化ありとみなし re-gate 実行) に扱う (ADR-043)。
///
/// `run_diff` と違いファイルには書かない (比較のためメモリ上で保持するだけ)。timeout /
/// stderr 分離の要件は `run_diff_cmd` と同一 (同 doc 参照)。
pub(crate) fn capture_diff_snapshot(config: &DiffConfig) -> Option<String> {
    let timeout = config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS);
    run_diff_cmd(&config.command, timeout).ok()
}

pub(crate) fn run_diff(config: &DiffConfig) -> DiffResult {
    log_stage("diff", &format!("実行: {}", config.command));

    let timeout = config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS);
    let output = match run_diff_cmd(&config.command, timeout) {
        Ok(output) => output,
        Err(err) => {
            log_stage("diff", "diff コマンド失敗");
            if !err.is_empty() {
                eprintln!("{}", err);
            }
            return DiffResult::Error;
        }
    };

    if output.is_empty() {
        log_stage(
            "diff",
            "diff 出力が空です。レビューをスキップして push に進みます。",
        );
        return DiffResult::Empty;
    }

    let path = Path::new(&config.output_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log_stage("diff", &format!("ディレクトリ作成失敗: {}", e));
            return DiffResult::Error;
        }
    }

    match std::fs::write(path, &output) {
        Ok(()) => {
            let line_count = output.lines().count();
            log_stage(
                "diff",
                &format!("書き出し完了: {} ({} 行)", config.output_path, line_count),
            );
            DiffResult::HasContent
        }
        Err(e) => {
            log_stage("diff", &format!("ファイル書き出し失敗: {}", e));
            DiffResult::Error
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 行を吐くコマンド。cmd.exe と POSIX sh で構文が非互換なため OS 別に
    /// 出し分ける (WP-15)。POSIX 側は `seq` 不在の最小環境でも動く while ループ。
    #[cfg(windows)]
    const EMIT_100_LINES_CMD: &str = "for /L %i in (1,1,100) do @echo line %i";
    #[cfg(not(windows))]
    const EMIT_100_LINES_CMD: &str = "i=1; while [ $i -le 100 ]; do echo line $i; i=$((i+1)); done";

    /// 何も出力せず正常終了するコマンド (0 バイト出力の検証用)。
    #[cfg(windows)]
    const ZERO_BYTE_OUTPUT_CMD: &str = "type nul";
    #[cfg(not(windows))]
    const ZERO_BYTE_OUTPUT_CMD: &str = "true";

    /// stderr へ出力してから非 0 で終わるコマンド (失敗診断の検証用)。
    #[cfg(windows)]
    const STDERR_THEN_FAIL_CMD: &str = "echo boom 1>&2& exit /b 1";
    #[cfg(not(windows))]
    const STDERR_THEN_FAIL_CMD: &str = "echo boom 1>&2; exit 1";

    /// stdout と stderr の両方へ出しつつ正常終了するコマンド。
    /// stderr (jj の警告相当) が diff 本体に混ざらない契約の検証用。
    #[cfg(windows)]
    const STDOUT_AND_STDERR_CMD: &str = "echo real diff& echo Concurrent modification 1>&2";
    #[cfg(not(windows))]
    const STDOUT_AND_STDERR_CMD: &str = "echo real diff; echo Concurrent modification 1>&2";

    #[test]
    fn run_diff_cmd_captures_more_than_40_lines() {
        let result = run_diff_cmd(EMIT_100_LINES_CMD, 30);
        assert!(result.is_ok(), "command should succeed");
        let output = result.unwrap();
        let line_count = output.lines().count();
        assert!(
            line_count > 40,
            "expected >40 lines captured, got {}; run_diff_cmd must not apply the 40-line cap",
            line_count
        );
    }

    #[test]
    fn run_diff_returns_empty_when_output_is_empty() {
        let out_path = std::env::temp_dir().join("test-run-diff-empty.txt");
        let _ = std::fs::remove_file(&out_path);

        let config = DiffConfig {
            command: ZERO_BYTE_OUTPUT_CMD.to_string(),
            output_path: out_path.to_string_lossy().into_owned(),
            timeout: None,
        };

        let result = run_diff(&config);

        assert_eq!(
            result,
            DiffResult::Empty,
            "run_diff must return Empty when the diff command produces empty output"
        );
        assert!(
            !out_path.exists(),
            "output file must not be created for an empty diff"
        );
    }

    /// T12: capture_diff_snapshot は成功時に stdout を Some で返す
    /// (post-takt re-gate の pre/post 比較の材料)。
    #[test]
    fn capture_diff_snapshot_returns_output_on_success() {
        let config = DiffConfig {
            command: "echo snapshot-content".to_string(),
            output_path: String::new(),
            timeout: None,
        };
        let snap = capture_diff_snapshot(&config).expect("成功時は Some");
        assert!(
            snap.contains("snapshot-content"),
            "stdout をそのまま返すこと: {:?}",
            snap
        );
    }

    /// T12: 取得失敗 (コマンド exit 非 0) は None を返す
    /// (呼び出し側は None を fail-closed = 変化ありに倒す)。
    #[test]
    fn capture_diff_snapshot_returns_none_on_failure() {
        let config = DiffConfig {
            command: STDERR_THEN_FAIL_CMD.to_string(),
            output_path: String::new(),
            timeout: Some(30),
        };
        assert!(
            capture_diff_snapshot(&config).is_none(),
            "失敗時は None (呼び出し側で fail-closed に扱う)"
        );
    }

    /// T6 回帰テスト群: diff stage に timeout が無く無限ハングし得た不具合
    /// (ADR-049 の流儀: 1 test = 1 failure mode + good/bad)。
    ///
    /// 由来: 2026-07-16 の push パイプライン調査 (コード監査で発見。T5 と同じく
    /// in the wild の発火記録は無く、「他 stage は全て timeout 付き = diff だけが穴」
    /// という非対称として特定された)。
    ///
    /// 事故の形: `run_diff_cmd` は `Command::output()` で子プロセスの終了を**無限に**
    /// 待っていた。ADR-045 の並列 workspace 運用で jj の lock 競合が起きると
    /// `pnpm push` は診断も timeout も無いまま停止し、ユーザーは手動 kill するしかない。
    ///
    /// 修正の核心は「timeout 付きで待ち、超過時は Err → `DiffResult::Error` = exit 5 で
    /// 中断する (fail-closed / ADR-043)」。あわせて、判定に使う stdout を stderr と
    /// 混ぜない契約 (レビュー対象を汚さない) も本 mod で seal する。
    mod t6_diff_timeout {
        use super::*;
        use std::time::{Duration, Instant};

        /// 実行し続けるコマンド (ハングした jj の代役)。timeout が無ければ約 9s 待たされる。
        /// cmd.exe と POSIX sh で構文が非互換なため OS 別に出し分ける (WP-15)。
        /// 所要時間を両 OS で揃えないと片側だけ timeout 経路を検証しない穴になる。
        #[cfg(windows)]
        const HANGING_COMMAND: &str = "ping 127.0.0.1 -n 10";
        #[cfg(not(windows))]
        const HANGING_COMMAND: &str = "sleep 10";

        const SHORT_TIMEOUT_SECS: u64 = 1;

        /// incident 再現 (bad): 応答しないコマンドを **timeout で打ち切る**こと。
        /// 修正前は `Command::output()` が返るまで待ち続け、本 assert には到達しなかった。
        #[test]
        fn hanging_command_times_out_instead_of_waiting_forever() {
            let started = Instant::now();
            let result = run_diff_cmd(HANGING_COMMAND, SHORT_TIMEOUT_SECS);
            let elapsed = started.elapsed();

            let err = result.expect_err("timeout は Err で返ること (無限待ちしない)");
            assert!(
                err.contains("タイムアウト"),
                "timeout と判る診断を返すこと: {:?}",
                err,
            );
            assert!(
                elapsed < Duration::from_secs(5),
                "timeout ({}s) 後すぐ制御を返すこと。{:?} 掛かった = コマンドの自然終了を\
                 待っている (T6 の不具合)",
                SHORT_TIMEOUT_SECS,
                elapsed,
            );
        }

        /// timeout の診断は原因調査に足りること: 超過秒数と実行コマンドを含む。
        #[test]
        fn timeout_error_reports_the_limit_and_the_command() {
            let err = run_diff_cmd(HANGING_COMMAND, SHORT_TIMEOUT_SECS)
                .expect_err("timeout は Err で返ること");
            assert!(
                err.contains(&format!("{}s", SHORT_TIMEOUT_SECS)) && err.contains(HANGING_COMMAND),
                "超過秒数と実行コマンドを診断に含めること: {:?}",
                err,
            );
        }

        /// timeout は fail-closed で pipeline を止めること (ADR-043)。
        /// `DiffResult::Error` は main.rs で exit 5 = 中断になる。空 diff 扱いで
        /// **レビューを skip したまま push に進んではならない**。
        #[test]
        fn timeout_aborts_the_pipeline_and_writes_no_diff_file() {
            let out_path = std::env::temp_dir().join("test-run-diff-timeout.txt");
            let _ = std::fs::remove_file(&out_path);

            let config = DiffConfig {
                command: HANGING_COMMAND.to_string(),
                output_path: out_path.to_string_lossy().into_owned(),
                timeout: Some(SHORT_TIMEOUT_SECS),
            };

            assert_eq!(
                run_diff(&config),
                DiffResult::Error,
                "timeout は Error (= exit 5 で中断) になること。Empty だとレビューを\
                 skip して push に進んでしまう",
            );
            assert!(
                !out_path.exists(),
                "timeout 時に diff ファイルを書かないこと (古い/欠けた diff でレビューさせない)",
            );
        }

        /// good: timeout 内に終わるコマンドを誤って打ち切らないこと。
        #[test]
        fn command_within_the_timeout_succeeds() {
            let output = run_diff_cmd("echo diff line", 30).expect("即終了するコマンドは Ok");
            assert!(output.contains("diff line"), "stdout を返すこと: {:?}", output);
        }

        /// `[diff] timeout` 未指定なら既定値が使われること (既定値の適用漏れ防止)。
        #[test]
        fn absent_config_timeout_falls_back_to_the_default() {
            let config = DiffConfig {
                command: "echo ok".to_string(),
                output_path: std::env::temp_dir()
                    .join("test-run-diff-default-timeout.txt")
                    .to_string_lossy()
                    .into_owned(),
                timeout: None,
            };
            assert_eq!(
                config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS),
                DEFAULT_DIFF_TIMEOUT_SECS,
            );
            assert_eq!(run_diff(&config), DiffResult::HasContent);
            let _ = std::fs::remove_file(&config.output_path);
        }

        /// stderr を stdout に混ぜないこと: stdout は reviewers が読む diff そのものとして
        /// ファイルに書かれるため、jj の警告 (並列 workspace 時の `Concurrent modification
        /// detected` 等) が混入するとレビュー対象を汚す。`run_cmd_shell_*` (全 variant が
        /// stdout/stderr を結合する) に載せ替えるとこのテストが落ちる。
        #[test]
        fn stderr_is_not_merged_into_the_diff_output() {
            let output = run_diff_cmd(STDOUT_AND_STDERR_CMD, 30).expect("exit 0 なら Ok");
            assert!(output.contains("real diff"), "stdout は残ること: {:?}", output);
            assert!(
                !output.contains("Concurrent modification"),
                "stderr の警告が diff 内容に混入しないこと: {:?}",
                output,
            );
        }

        /// 失敗時は stderr を診断として返すこと (従来契約の維持)。
        #[test]
        fn failure_returns_stderr_as_the_diagnostic() {
            let err = run_diff_cmd(STDERR_THEN_FAIL_CMD, 30).expect_err("exit 1 は Err");
            assert!(err.contains("boom"), "stderr を診断に返すこと: {:?}", err);
        }
    }
}
