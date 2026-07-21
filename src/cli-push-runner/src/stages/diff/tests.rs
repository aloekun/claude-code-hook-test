//! diff stage のテスト (production は ../diff.rs)。ファイル長 800 行ガイドライン
//! (順位 147) 遵守のため #312 pipeline_lock と同じく test mod を切り出した。

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

/// SIM-NEW-diff-rs-L146: rename 行 (`R <old> <new>`) は new path だけを
/// パス集合に採用すること。旧実装は old+new を 1 個の壊れたパスにしてしまい、
/// coverage 検査が rename ファイルを絶対に "covered" と一致させられなかった。
#[test]
fn parse_summary_paths_extracts_new_path_for_rename() {
    let paths = parse_summary_paths("R docs/a.md docs/b.md\n").expect("valid rename は Ok");
    assert_eq!(
        paths,
        std::collections::BTreeSet::from(["docs/b.md".to_string()]),
        "rename は new path のみを採用し、old path や結合パスを含めないこと: {:?}",
        paths
    );
}

/// copy 行 (`C <old> <new>`) も rename と同じ扱い (new path のみ採用)。
#[test]
fn parse_summary_paths_extracts_new_path_for_copy() {
    let paths = parse_summary_paths("C docs/a.md docs/c.md\n").expect("valid copy は Ok");
    assert_eq!(
        paths,
        std::collections::BTreeSet::from(["docs/c.md".to_string()]),
        "copy は new path のみを採用すること: {:?}",
        paths
    );
}

/// incident 再現 (rename 版): rename されたファイルを含む PR 範囲でも、
/// new path を収録した diff は coverage 検査を通ること。
#[test]
fn verify_accepts_diff_with_renamed_file() {
    let summary = "R docs/a.md docs/b.md\n";
    let diff = "diff --git a/docs/a.md b/docs/b.md\n@@ -1 +1 @@\n-old\n+new\n";
    assert!(
        verify_diff_covers_pr_range(diff, || Ok(summary.to_string())).is_ok(),
        "rename ファイルの new path が --git ヘッダの b/ 側と一致し、coverage 検査を\
         通ること (旧実装は old+new の壊れたパスと一致せず常に未収録扱いだった)"
    );
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

/// security-review 指摘: summary に行はあるのに 1 件もパースできない (= jj の
/// 出力書式変更) 場合、空集合に丸めて素通しすると gate が無言で機能停止する。
/// 「行はあるがパース不能」と「そもそも変更なし」を区別して前者は fail-closed。
///
/// 入力は**未知 status のみ**の行にして、意図した「パース不能」分岐を確実に突く
/// (旧テストは status を含む行が catch-all で妥当パス化され、後段の「missing files」
/// 分岐でたまたま Err になっていた = 主張と別分岐を検証していた。SIM-NEW-diff-rs-L178)。
#[test]
fn verify_fails_closed_when_summary_is_unparseable() {
    let unparseable = "X docs/plan.md\nZ src/checker/decide.rs\n";
    let result =
        verify_diff_covers_pr_range(INCIDENT_TIP_ONLY_DIFF, || Ok(unparseable.to_string()));
    let err = result.expect_err("パース不能を素通しすると gate が無言で死ぬ");
    assert!(
        err.contains("パースできませんでした"),
        "『missing files』ではなく『summary パース不能』分岐で fail-closed すべき: {}",
        err
    );
}

/// CodeRabbit #313 (per-line fail-open): valid な docs/plan.md は diff に含まれるが、
/// 未知 status の src/checker/decide.rs 行が混ざる。旧実装は未知行を silent drop し、
/// docs/plan.md だけで coverage を通していた。1 行でも解釈不能なら fail-closed。
#[test]
fn verify_fails_closed_when_summary_has_mixed_unparseable_line() {
    let mixed = "M docs/plan.md\nX src/checker/decide.rs\n";
    let result = verify_diff_covers_pr_range(INCIDENT_TIP_ONLY_DIFF, || Ok(mixed.to_string()));
    let err = result.expect_err("valid 行と未知行の混在は fail-closed");
    assert!(
        err.contains("パースできませんでした"),
        "混在 summary は『summary パース不能』分岐で fail-closed すべき: {}",
        err
    );
}

/// 未知 status 行が「妥当なパス」として取り込まれないこと (catch-all で status を
/// 素通しすると書式変化を検知できなくなる。SIM-NEW-diff-rs-L178)。
#[test]
fn parse_summary_paths_rejects_unknown_status() {
    assert!(
        parse_summary_paths("X docs/plan.md\n").is_err(),
        "未知 status 行は Err (silent drop しない)"
    );
    assert_eq!(
        parse_summary_paths("M docs/plan.md\n").expect("既知 status は Ok"),
        std::collections::BTreeSet::from(["docs/plan.md".to_string()]),
        "既知 status (M/A/D/R/C) は従来どおり受理する"
    );
}

/// 書式が崩れた R/C 行 (2 トークン目が無い) を **明示的に reject** すること
/// (CodeRabbit #313: 旧実装は生トークンを残して coverage 不一致に頼っていたが、
/// fail-closed の判定をパース時点に前倒しする)。
#[test]
fn parse_summary_paths_rejects_malformed_rename_line() {
    assert!(
        parse_summary_paths("R docs/only-one-token.md\n").is_err(),
        "new path (末尾トークン) が無い崩れた rename 行は Err"
    );
    assert_eq!(
        parse_summary_paths("R docs/a.md docs/b.md\n").expect("正常 rename は Ok"),
        std::collections::BTreeSet::from(["docs/b.md".to_string()]),
        "正常な rename は new path のみ"
    );
}

/// CodeRabbit #313: valid 行と未知 status 行が混在した summary は、valid 行だけで
/// `expected` が非空になり coverage を通過してしまう per-line fail-open があった。
/// 1 行でも解釈不能なら全体を Err にして fail-closed に倒す。
#[test]
fn parse_summary_paths_rejects_mixed_valid_and_unparseable_lines() {
    assert!(
        parse_summary_paths("M docs/plan.md\nX src/foo.rs\n").is_err(),
        "valid 行があっても未知 status 行が 1 つでもあれば Err (silent drop しない)"
    );
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

/// CodeRabbit #313: diff コマンドが空出力でも、PR 範囲に変更があれば (summary 非空)
/// coverage 検査で fail-closed にする。旧実装は `output.is_empty()` を coverage 検査
/// より先に評価し、範囲がずれて空になった diff を「レビュー対象なし」と誤認して
/// gate を素通りさせていた。
#[test]
fn run_diff_errors_when_output_empty_but_pr_range_has_changes() {
    let out_path = std::env::temp_dir().join("test-run-diff-empty-but-changes.txt");
    let _ = std::fs::remove_file(&out_path);
    let config = DiffConfig {
        command: ZERO_BYTE_OUTPUT_CMD.to_string(),
        output_path: out_path.to_string_lossy().into_owned(),
        timeout: Some(30),
        default_branch: None,
    };

    let result = run_diff_with(&config, &test_pr_range(), || {
        Ok(INCIDENT_PR_SUMMARY.to_string())
    });

    assert_eq!(
        result,
        DiffResult::Error,
        "空出力でも PR 範囲に変更があれば coverage 検査で fail-closed になること"
    );
    assert!(
        !out_path.exists(),
        "coverage 不足では diff ファイルを書き出さない"
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
