//! 実 exe を起動して **exit code = run の色** であることを固定する E2E (順位 488)。
//!
//! unit test は [`classify`] の純関数を直接呼ぶが、それだけでは
//! 「env → 分類 → exit code」の配線が繋がっているかを見ていない。順位 488 の失敗は
//! まさに「判定は正しいが色にならない」形だったので、色そのものを実 exe で assert する。
//!
//! `.github/workflows/nightly-todo.yml` の `Report outcome` step が渡す env をここで
//! 再現している。step の `env:` を書き換えたら本テストの `ENV_NAMES` も追随させること。

use lib_subprocess::{drain_pipe_unlimited, wait_with_timeout_safe};
use std::process::{Command, Stdio};

/// 判定は I/O を持たないので即座に返る。ハングは配線バグなのでテストを落とす。
const TIMEOUT_SECS: u64 = 30;

/// `Report outcome` step が渡す env の全量。**継承した値が混ざらないよう全件を明示的に
/// 上書きする** (空にしたい列も空文字で渡す)。
const ENV_NAMES: &[&str] = &[
    "CLEANUP_OUTCOME",
    "PREFLIGHT_OUTCOME",
    "SELECT_OUTCOME",
    "IMPLEMENT_OUTCOME",
    "VERIFY_OUTCOME",
    "PUBLISH_TREE_OUTCOME",
    "GUARD_OUTCOME",
    "INTEGRITY_OUTCOME",
    "LEDGER_COMPLETION_OUTCOME",
    "LEDGER_REMOVAL_OUTCOME",
    "GATE_OUTCOME",
    "APP_TOKEN_OUTCOME",
    "PUBLISH_OUTCOME",
    "HANDOFF_OUTCOME",
    "RANK",
    "DRY_RUN",
];

struct Run {
    code: i32,
    stdout: String,
}

fn run_exe(overrides: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cli-nightly-outcome"));
    for name in ENV_NAMES {
        let value = overrides.iter().find(|(k, _)| k == name).map_or("", |(_, v)| *v);
        command.env(name, value);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cli-nightly-outcome を起動できない");
    let stdout_drain = drain_pipe_unlimited(child.stdout.take().expect("child stdout"));
    let stderr_drain = drain_pipe_unlimited(child.stderr.take().expect("child stderr"));
    let status = wait_with_timeout_safe("cli-nightly-outcome", &mut child, TIMEOUT_SECS)
        .expect("wait_with_timeout_safe errored");
    let stdout = stdout_drain.join().expect("stdout drain thread panicked");
    let _stderr = stderr_drain.join().expect("stderr drain thread panicked");
    let status = status.unwrap_or_else(|| panic!("cli-nightly-outcome が {TIMEOUT_SECS}s で終わらなかった"));
    Run { code: status.code().expect("exit code が取れない"), stdout }
}

/// 完走した夜は green。
#[test]
fn a_created_pr_exits_zero() {
    let run = run_exe(&[
        ("SELECT_OUTCOME", "success"),
        ("IMPLEMENT_OUTCOME", "success"),
        ("PUBLISH_OUTCOME", "success"),
        ("RANK", "488"),
    ]);
    assert_eq!(run.code, 0, "stdout:\n{}", run.stdout);
    assert!(run.stdout.contains("[NIGHTLY] PR を作成しました。"), "stdout:\n{}", run.stdout);
}

/// **順位 488 の実観測 (2026-08-22 run 32589642740) の再現** — guard deny で handoff が
/// 発火した夜。移送前はここが exit 0 で green になっていた。
#[test]
fn a_guard_deny_after_implementing_exits_one() {
    let run = run_exe(&[
        ("SELECT_OUTCOME", "success"),
        ("IMPLEMENT_OUTCOME", "success"),
        ("GUARD_OUTCOME", "failure"),
        ("PUBLISH_OUTCOME", "skipped"),
        ("HANDOFF_OUTCOME", "success"),
        ("RANK", "488"),
    ]);
    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);
    assert!(run.stdout.contains("[NIGHTLY_HANDOFF]"), "stdout:\n{}", run.stdout);
    assert!(run.stdout.contains("順位 488"), "stdout:\n{}", run.stdout);
}

/// **決定 10 の意図を保持する側** — agent を回していない夜は green のまま
/// (背圧 deny / 該当タスク無し)。この test が落ちたら red の範囲を広げすぎている。
#[test]
fn a_backpressure_deny_exits_zero() {
    let run = run_exe(&[("CLEANUP_OUTCOME", "success"), ("PREFLIGHT_OUTCOME", "failure")]);
    assert_eq!(run.code, 0, "stdout:\n{}", run.stdout);
    assert!(run.stdout.contains("[NIGHTLY_SKIP]"), "stdout:\n{}", run.stdout);
}

/// サマリ行は未実行の step を `<未実行>` で埋め、全 14 列を必ず出す。
#[test]
fn the_summary_line_lists_every_step() {
    let run = run_exe(&[("PREFLIGHT_OUTCOME", "success")]);
    let summary = run
        .stdout
        .lines()
        .find(|l| l.starts_with("[NIGHTLY] "))
        .expect("サマリ行が無い");
    assert!(summary.contains("preflight=success"), "{summary}");
    assert!(summary.contains("handoff=<未実行>"), "{summary}");
    assert!(summary.contains("ledger_completion=<未実行>"), "{summary}");
    assert_eq!(summary.matches('=').count(), 14, "{summary}");
}

/// **red になった夜がどこで止まったかをサマリだけで特定できること** (CodeRabbit #445)。
///
/// handoff は verify / guard / ledger-completion のいずれかが非 success なら発火する。
/// ledger-completion で止まった run は verify も guard も success なので、その列が
/// サマリに無いと「red だが停止段が分からない」状態になる。
#[test]
fn a_ledger_completion_stop_is_identifiable_from_the_summary() {
    let run = run_exe(&[
        ("SELECT_OUTCOME", "success"),
        ("IMPLEMENT_OUTCOME", "success"),
        ("VERIFY_OUTCOME", "success"),
        ("GUARD_OUTCOME", "success"),
        ("INTEGRITY_OUTCOME", "success"),
        ("LEDGER_COMPLETION_OUTCOME", "failure"),
        ("PUBLISH_OUTCOME", "skipped"),
        ("HANDOFF_OUTCOME", "success"),
        ("RANK", "488"),
    ]);
    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);
    let summary = run
        .stdout
        .lines()
        .find(|l| l.starts_with("[NIGHTLY] "))
        .expect("サマリ行が無い");
    assert!(summary.contains("ledger_completion=failure"), "{summary}");
    assert!(summary.contains("verify=success"), "{summary}");
    assert!(summary.contains("guard=success"), "{summary}");
}

/// **順位 193 の実観測の再現** (2026-08-25 18:08 UTC の定時 run / 2026-08-26 14:22 UTC の
/// dispatch run)。台帳削除で `[LEDGER_CLEANUP_BLOCK]` が出た夜。
///
/// **移送前はここが `[NIGHTLY_SKIP]` だった** — 台帳削除の失敗が handoff の発火条件に
/// 入っておらず、agent を 1 回まるごと回して捨てた夜が「何もすることが無かった夜」の
/// マーカーで報告されていた。D1 で handoff の対象に加えたので `[NIGHTLY_HANDOFF]` になる。
#[test]
fn a_ledger_removal_failure_is_reported_as_a_handoff() {
    let run = run_exe(&[
        ("SELECT_OUTCOME", "success"),
        ("IMPLEMENT_OUTCOME", "success"),
        ("VERIFY_OUTCOME", "success"),
        ("GUARD_OUTCOME", "success"),
        ("INTEGRITY_OUTCOME", "success"),
        ("LEDGER_COMPLETION_OUTCOME", "success"),
        ("LEDGER_REMOVAL_OUTCOME", "failure"),
        ("PUBLISH_OUTCOME", "skipped"),
        ("HANDOFF_OUTCOME", "success"),
        ("RANK", "193"),
    ]);
    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);
    assert!(run.stdout.contains("[NIGHTLY_HANDOFF]"), "stdout:\n{}", run.stdout);
    assert!(!run.stdout.contains("[NIGHTLY_SKIP]"), "stdout:\n{}", run.stdout);
    let summary = run
        .stdout
        .lines()
        .find(|l| l.starts_with("[NIGHTLY] "))
        .expect("サマリ行が無い");
    assert!(
        summary.contains("ledger_removal=failure"),
        "停止段がサマリだけで特定できること: {summary}"
    );
    assert!(summary.contains("ledger_completion=success"), "{summary}");
}

/// 未知の outcome 値は green へ倒さない。
#[test]
fn an_unknown_outcome_exits_one() {
    let run = run_exe(&[("PUBLISH_OUTCOME", "succeeded")]);
    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);
    assert!(run.stdout.contains("[NIGHTLY_ERROR]"), "stdout:\n{}", run.stdout);
}
