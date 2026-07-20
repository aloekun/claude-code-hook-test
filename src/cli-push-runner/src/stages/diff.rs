//! Diff stage — `[diff] command` の出力を reviewers 用ファイルに書き出す。
//!
//! 出力は takt の reviewers が Read で参照するレビュー対象そのもののため、
//! **切り詰めない** (`run_diff_cmd` の doc)。実行は timeout 付き (T6)。
//!
//! ## レビュー範囲は PR 全体
//!
//! `[diff] command` の [`DIFF_PR_RANGE_PLACEHOLDER`] は `Config::diff_pr_range()`
//! (= `<base>..@`) に展開される。以前は config に `jj diff -r @` と直書きされており、
//! **tip コミットしかレビュアーに渡らなかった**。祖先コミットは AI レビューを一度も
//! 経ずに merge され、しかもレビュアー側からは「渡された diff が PR 全体か」を
//! 検証できないため誤りが誰にも検知されなかった (todo 順位 288、Severity High で
//! PR #268/#300/#301/#311 と 4 回再発)。
//!
//! 範囲の直書きを禁じるだけでは派生プロジェクトの古い config を救えないため、
//! [`verify_diff_covers_pr_range`] が生成された diff と PR 範囲の変更ファイル集合を
//! 突き合わせ、不足があれば fail-closed で中断する。

use std::path::Path;
use std::process::Stdio;

use lib_subprocess::{drain_pipe_unlimited, shell_command, wait_with_timeout_safe};

use crate::config::{DiffConfig, DEFAULT_DIFF_TIMEOUT_SECS, DIFF_PR_RANGE_PLACEHOLDER};
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

/// レビュー対象 diff が PR 範囲の全変更ファイルを含むか検査する (fail-closed / ADR-043)。
///
/// **なぜ必要か**: `[diff] command` は config 由来の自由文字列で、`-r @` のように
/// PR より狭い範囲を書けてしまう。狭い範囲を書いても reviewers 側からは「渡された
/// diff が全体か」を検証できず、正しく「docs-only」等と判定してしまうため、
/// 誤りが誰にも検知されないまま merge に至る (todo 順位 288、Severity High で 4 回再発)。
/// 検査の真実源は `jj diff --summary` = docs_only_routing / pr_size_check と同じ経路。
///
/// 判定不能 (summary 取得失敗 / diff がヘッダを持たない) は「網羅している」に倒さず
/// エラーにする。「検証できない」を「検証した」と扱わないための線引き。
fn verify_diff_covers_pr_range(
    diff_output: &str,
    fetch_summary: impl FnOnce() -> Result<String, String>,
) -> Result<(), String> {
    let summary = fetch_summary().map_err(|e| format!("PR 範囲の summary 取得に失敗: {}", e))?;

    let expected = parse_summary_paths(&summary);
    if expected.is_empty() {
        return Ok(());
    }

    let covered = parse_git_diff_paths(diff_output);
    if covered.is_empty() {
        return Err(
            "diff 出力に `diff --git` ヘッダが無く、対象ファイルを特定できません".to_string(),
        );
    }

    let missing: Vec<&String> = expected.iter().filter(|p| !covered.contains(*p)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} ファイルが未収録 (例: {})",
        missing.len(),
        missing
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// `jj diff --summary` の `M path` 形式からパス集合を作る。
///
/// Windows の jj は `\` 区切りで出すため `/` に正規化して `--git` 側と突き合わせる。
fn parse_summary_paths(summary: &str) -> std::collections::BTreeSet<String> {
    summary
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(_status, path)| path.trim().replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect()
}

/// unified diff の `diff --git a/X b/X` ヘッダからパス集合を作る。
fn parse_git_diff_paths(diff_output: &str) -> std::collections::BTreeSet<String> {
    diff_output
        .lines()
        .filter_map(|line| line.strip_prefix("diff --git "))
        .filter_map(|rest| rest.split_once(" b/"))
        .map(|(_a_side, b_path)| b_path.trim().replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect()
}

/// takt 実行後の diff snapshot を取得する (T12 post-takt re-gate の変化検出用)。
///
/// Stage 1.5 と同じ `[diff] command` を再実行し stdout を返す。呼び出し側 (re-gate) は
/// takt 起動前に保持した snapshot と本値を**前後比較**し、一致すれば「fix はコードを
/// 書き換えていない」= re-gate skip に倒す。取得失敗 (jj 失敗 / timeout) は `None` を返し、
/// 呼び出し側が fail-closed (= 変化ありとみなし re-gate 実行) に扱う (ADR-043)。
///
/// `run_diff` と違いファイルには書かない (比較のためメモリ上で保持するだけ)。timeout /
/// stderr 分離の要件は `run_diff_cmd` と同一 (同 doc 参照)。範囲カバレッジ検査も
/// 行わない (前後比較が目的で、レビュー入力にはならないため)。
pub(crate) fn capture_diff_snapshot(config: &DiffConfig, pr_range: &str) -> Option<String> {
    let timeout = config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS);
    run_diff_cmd(&resolve_diff_command(&config.command, pr_range), timeout).ok()
}

/// `[diff] command` の [`DIFF_PR_RANGE_PLACEHOLDER`] を PR 範囲 revset に展開する。
pub(crate) fn resolve_diff_command(command: &str, pr_range: &str) -> String {
    command.replace(DIFF_PR_RANGE_PLACEHOLDER, pr_range)
}

pub(crate) fn run_diff(config: &DiffConfig, pr_range: &str) -> DiffResult {
    run_diff_with(config, pr_range, || {
        super::docs_only_routing::run_jj_diff_summary(pr_range)
    })
}

/// `run_diff` の本体。PR 範囲の summary 取得を注入可能にして、範囲カバレッジ検査を
/// jj 実行なしでテストできるようにする (`post_takt_regate::decide_regate` と同じ流儀)。
fn run_diff_with(
    config: &DiffConfig,
    pr_range: &str,
    fetch_summary: impl FnOnce() -> Result<String, String>,
) -> DiffResult {
    let command = resolve_diff_command(&config.command, pr_range);
    log_stage("diff", &format!("実行: {}", command));

    let timeout = config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS);
    let output = match run_diff_cmd(&command, timeout) {
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

    if let Err(reason) = verify_diff_covers_pr_range(&output, fetch_summary) {
        report_coverage_failure(pr_range, &reason);
        return DiffResult::Error;
    }

    write_diff_output(&config.output_path, &output)
}

/// 範囲検査に落ちたときの fail-closed 通知 (ADR-043)。
///
/// 「レビュー範囲が PR より狭い」ことは検知できても自動修復はできない
/// (どこまで広げるべきかは config の意図次第) ため、push を止めて人間に返す。
fn report_coverage_failure(pr_range: &str, reason: &str) {
    log_stage("diff", &format!("レビュー範囲の検査に失敗: {}", reason));
    eprintln!(
        "[push-runner] [diff] レビュー対象 diff が PR 範囲 ({}) を網羅していません: {}\n\
         このまま進めると祖先コミットが AI レビューを経ずに merge されます (todo 順位 288)。\n\
         `[diff] command` が `{}` を使っているか、出力が unified diff (--git) 形式かを確認してください。",
        pr_range, reason, DIFF_PR_RANGE_PLACEHOLDER
    );
}

fn write_diff_output(output_path: &str, output: &str) -> DiffResult {
    let path = Path::new(output_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log_stage("diff", &format!("ディレクトリ作成失敗: {}", e));
            return DiffResult::Error;
        }
    }

    match std::fs::write(path, output) {
        Ok(()) => {
            let line_count = output.lines().count();
            log_stage(
                "diff",
                &format!("書き出し完了: {} ({} 行)", output_path, line_count),
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

    /// 範囲検査を jj 非依存でテストするための PR 範囲 revset。
    fn test_pr_range() -> String {
        format!("{}..@", crate::config::DEFAULT_BASE_BRANCH)
    }

    /// PR #268/#300/#301/#311 の incident 形状: PR に 2 ファイル変更があるのに、
    /// レビュー対象 diff には tip コミットの 1 ファイルしか入っていない。
    const INCIDENT_PR_SUMMARY: &str = "M docs/plan.md\nM src/checker/decide.rs\n";
    const INCIDENT_TIP_ONLY_DIFF: &str =
        "diff --git a/docs/plan.md b/docs/plan.md\n@@ -1 +1 @@\n-a\n+b\n";

    #[test]
    fn resolve_diff_command_expands_pr_range_placeholder() {
        let range = test_pr_range();
        assert_eq!(
            resolve_diff_command("jj diff --git -r {{PR_RANGE}}", &range),
            format!("jj diff --git -r {}", range)
        );
    }

    /// base branch が `main` の派生プロジェクトでも展開が成立すること
    /// (rule⑫ が防ごうとしている alternative branch の silent breakage)。
    #[test]
    fn resolve_diff_command_expands_alternative_base_branch() {
        assert_eq!(
            resolve_diff_command("jj diff --git -r {{PR_RANGE}}", "main..@"),
            "jj diff --git -r main..@"
        );
    }

    /// incident 再現: レビュー対象 diff が PR 範囲より狭いことを検知する。
    /// 旧実装にはこの検査が無く、狭い diff が「全体」として reviewers に渡っていた。
    #[test]
    fn verify_rejects_diff_narrower_than_pr_range() {
        let result = verify_diff_covers_pr_range(INCIDENT_TIP_ONLY_DIFF, || {
            Ok(INCIDENT_PR_SUMMARY.to_string())
        });
        let err = result.expect_err("PR 範囲より狭い diff は検知されなければならない");
        assert!(
            err.contains("src/checker/decide.rs"),
            "未収録ファイル名を示すこと: {}",
            err
        );
    }

    #[test]
    fn verify_accepts_diff_covering_whole_pr_range() {
        let full_diff = format!(
            "{}diff --git a/src/checker/decide.rs b/src/checker/decide.rs\n@@ -1 +1 @@\n-x\n+y\n",
            INCIDENT_TIP_ONLY_DIFF
        );
        assert!(
            verify_diff_covers_pr_range(&full_diff, || Ok(INCIDENT_PR_SUMMARY.to_string())).is_ok()
        );
    }

    /// Windows の jj は `--summary` を `\` 区切りで出すため、正規化しないと
    /// 全ファイルが「未収録」に見えて常時 fail する。
    #[test]
    fn verify_normalizes_windows_path_separators() {
        let summary = "M docs\\plan.md\n";
        assert!(verify_diff_covers_pr_range(INCIDENT_TIP_ONLY_DIFF, || Ok(
            summary.to_string()
        ))
        .is_ok());
    }

    /// summary 取得に失敗したら「網羅している」に倒さない (fail-closed / ADR-043)。
    #[test]
    fn verify_fails_closed_when_summary_unavailable() {
        let result =
            verify_diff_covers_pr_range(INCIDENT_TIP_ONLY_DIFF, || Err("jj 失敗".to_string()));
        assert!(
            result.is_err(),
            "検証できないことを検証したものとして扱ってはならない"
        );
    }

    /// jj 既定形式 (`diff --git` ヘッダを持たない) では収録ファイルを特定できないため
    /// fail-closed にする。--git 形式への切替 (todo 順位 264) を機械的に要求する層。
    #[test]
    fn verify_fails_closed_when_diff_has_no_git_headers() {
        let plain_format_diff = "Modified regular file docs/plan.md:\n   1    1: -a\n";
        let result = verify_diff_covers_pr_range(plain_format_diff, || {
            Ok(INCIDENT_PR_SUMMARY.to_string())
        });
        assert!(result.is_err(), "形式不明なら検証済みと扱わない");
    }

    /// PR 範囲が空 (変更なし) なら検査は素通しする (空 diff は上流で Empty 扱い)。
    #[test]
    fn verify_passes_when_pr_range_is_empty() {
        assert!(verify_diff_covers_pr_range("", || Ok(String::new())).is_ok());
    }

    /// run_diff の実経路で範囲不足が Error になること (report 経路まで含めた固定)。
    #[test]
    fn run_diff_errors_when_coverage_check_fails() {
        let out_path = std::env::temp_dir().join("test-run-diff-coverage.txt");
        let _ = std::fs::remove_file(&out_path);
        let config = DiffConfig {
            command: format!("echo {}", INCIDENT_TIP_ONLY_DIFF.lines().next().unwrap()),
            output_path: out_path.to_string_lossy().into_owned(),
            timeout: Some(30),
            default_branch: None,
        };

        let result = run_diff_with(&config, &test_pr_range(), || {
            Ok(INCIDENT_PR_SUMMARY.to_string())
        });

        assert_eq!(result, DiffResult::Error);
        assert!(
            !out_path.exists(),
            "範囲不足の diff はレビュー入力として書き出さない"
        );
    }

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
            default_branch: None,
        };

        let result = run_diff_with(&config, &test_pr_range(), || Ok(String::new()));

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
            default_branch: None,
        };
        let snap = capture_diff_snapshot(&config, &test_pr_range()).expect("成功時は Some");
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
            default_branch: None,
        };
        assert!(
            capture_diff_snapshot(&config, &test_pr_range()).is_none(),
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
                default_branch: None,
            };

            assert_eq!(
                run_diff_with(&config, &test_pr_range(), || Ok(String::new())),
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
                default_branch: None,
            };
            assert_eq!(
                config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS),
                DEFAULT_DIFF_TIMEOUT_SECS,
            );
            assert_eq!(run_diff_with(&config, &test_pr_range(), || Ok(String::new())), DiffResult::HasContent);
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
