//! 夜間ループ run の結末を 1 行サマリ + 説明行で報告し、**run の色を exit code で決める**
//! (順位 488、ADR-072 決定 10)。
//!
//! # 使い方
//!
//! `.github/workflows/nightly-todo.yml` の `Report outcome` step が env 経由で各 step の
//! outcome を渡して呼ぶ。引数は取らない。
//!
//! ```text
//! CLEANUP_OUTCOME=success ... HANDOFF_OUTCOME=success cli-nightly-outcome
//! ```
//!
//! # なぜ exe か
//!
//! 移送前は同じ判定を shell の `if` 連鎖で書いていた。**回帰テストを書く場が無い判定を
//! 無人経路に置かない** (ADR-072 決定 1) の適用で、色の分類だけを exe へ移した。分類の
//! 中身と根拠は [`classify`] の module doc にある。
//!
//! # 何を「しない」か
//!
//! 停止理由の再分類はしない。marker の作成もしない (handoff step の担当)。本 exe は
//! **既に確定した step outcome を読んで色を返すだけ**で、外部 I/O を一切行わない。
//!
//! # exit コード
//!
//! - `0` = green (PR 作成 / agent を回していない停止)
//! - `1` = red (implement 後の停止 / handoff 失敗 / 分類不能)

mod classify;

use classify::{classify, render, residue_lines, residue_ranks, stop_stage, summary_line};

/// `Report outcome` step が渡す step outcome の env 名と、サマリ行での表示名。
///
/// **順序がそのままサマリ行の順序**で、workflow の step 実行順に並べてある。
///
/// **handoff の発火条件に出てくる step はすべてここに要る。** handoff は verify / guard /
/// ledger-completion / ledger-removal のいずれかで止まったときに発火するため、4 つ揃って
/// いないと「red になったがどこで止まったか分からない」サマリになる (CodeRabbit #445)。
/// この対応は [`tests::the_summary_contract_covers_every_outcome_the_workflow_passes`] が
/// 実 workflow と照合して固定している。
const OUTCOME_FIELDS: &[(&str, &str)] = &[
    ("cleanup", "CLEANUP_OUTCOME"),
    ("preflight", "PREFLIGHT_OUTCOME"),
    ("select", "SELECT_OUTCOME"),
    ("implement", "IMPLEMENT_OUTCOME"),
    ("verify", "VERIFY_OUTCOME"),
    ("publish_tree", "PUBLISH_TREE_OUTCOME"),
    ("guard", "GUARD_OUTCOME"),
    ("integrity", "INTEGRITY_OUTCOME"),
    ("ledger_completion", "LEDGER_COMPLETION_OUTCOME"),
    ("ledger_removal", "LEDGER_REMOVAL_OUTCOME"),
    ("gate", "GATE_OUTCOME"),
    ("app_token", "APP_TOKEN_OUTCOME"),
    ("publish", "PUBLISH_OUTCOME"),
    ("handoff", "HANDOFF_OUTCOME"),
];

fn env_or_empty(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// `true` ちょうどの完全一致だけを dry_run と見る (workflow 側の `if:` と同じ受理幅)。
fn is_dry_run(raw: &str) -> bool {
    raw.trim() == "true"
}

fn main() {
    let values: Vec<(String, String)> = OUTCOME_FIELDS
        .iter()
        .map(|(label, env_name)| (label.to_string(), env_or_empty(env_name)))
        .collect();
    let pairs: Vec<(&str, &str)> =
        values.iter().map(|(label, raw)| (label.as_str(), raw.as_str())).collect();
    println!("{}", summary_line(&pairs));

    let publish = env_or_empty("PUBLISH_OUTCOME");
    let handoff = env_or_empty("HANDOFF_OUTCOME");
    let verdict = classify(&publish, &handoff);
    for line in render(
        &verdict,
        &env_or_empty("RANK"),
        is_dry_run(&env_or_empty("DRY_RUN")),
        stop_stage(&pairs),
    ) {
        println!("{line}");
    }
    let residue = residue_ranks(&env_or_empty("LEDGER_RESIDUE_RANKS"));
    for line in residue_lines(&residue) {
        println!("{line}");
    }
    if verdict.is_red() || !residue.is_empty() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_requires_an_exact_true() {
        assert!(is_dry_run("true"));
        assert!(is_dry_run(" true "));
        assert!(!is_dry_run("True"));
        assert!(!is_dry_run("1"));
        assert!(!is_dry_run(""));
    }

    /// サマリ行の列が workflow の `Report outcome` env と 1:1 であること。
    /// 片方だけ増やすと「渡しているのに出ない」列が黙って生まれる。
    #[test]
    fn every_field_has_a_distinct_label_and_env_name() {
        let labels: Vec<&str> = OUTCOME_FIELDS.iter().map(|(l, _)| *l).collect();
        let envs: Vec<&str> = OUTCOME_FIELDS.iter().map(|(_, e)| *e).collect();
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len(), "サマリ行の表示名が重複している");
        assert!(envs.iter().all(|e| e.ends_with("_OUTCOME")));
    }

    /// **handoff step の発火条件に、marker を残すべき停止段がすべて挙がっていること。**
    ///
    /// 本 crate の判定は `publish` / `handoff` の 2 つしか見ないため、**どの停止段で handoff が
    /// 発火するかは workflow 側の `if` にしか無い**。その条件を消してもここ以外のテストは
    /// すべて通る (CodeRabbit #449 の指摘。実測で確認した)。条件を実ファイルから読んで
    /// 固定しないと、marker が作られなくなっても誰も気づかない — それは
    /// [ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 19 が防ごうとした
    /// 「失敗した run が先頭を独占する」状態そのものである。
    ///
    /// cwd 依存のため `--ignored` (ADR-041)。
    #[test]
    #[ignore = "cwd 依存: リポジトリルートの .github/workflows/nightly-todo.yml を読む。--test-threads=1 で実行"]
    fn the_handoff_condition_covers_every_stop_that_needs_a_marker() {
        let condition = handoff_step_condition(&read_nightly_workflow());
        for stage in classify::STOP_STAGES {
            let step = stage.workflow_step();
            assert!(
                condition.contains(&format!("steps.{step}.outcome")),
                "handoff の発火条件に {step} が無い — この段で止まると marker が残らず、\
                 同じ順位が翌晩も選ばれる (ADR-072 決定 19)。条件:\n{condition}"
            );
        }
    }

    /// **停止段の列名がサマリ行に在ること。** `stop_stage` はサマリ行と同じ
    /// `(列名, outcome)` の並びを引くので、列名がずれると**段を特定できない側**へ倒れる
    /// (もっともらしい段を出さない設計なので、黙って「特定できませんでした」になる)。
    #[test]
    fn every_stop_stage_has_a_column_in_the_summary_line() {
        let labels: Vec<&str> = OUTCOME_FIELDS.iter().map(|(label, _)| *label).collect();
        for stage in classify::STOP_STAGES {
            assert!(
                labels.contains(&stage.label()),
                "停止段 {:?} の列名 {} がサマリ行に無い",
                stage,
                stage.label()
            );
        }
    }

    fn read_nightly_workflow() -> String {
        let rel = ".github/workflows/nightly-todo.yml";
        std::fs::read_to_string(format!("../../{rel}"))
            .or_else(|_| std::fs::read_to_string(rel))
            .expect("nightly-todo.yml を読めない")
    }

    /// handoff step の `if:` ブロックだけを切り出す (`env:` の手前まで)。
    fn handoff_step_condition(content: &str) -> String {
        content
            .split("- name: Leave a handoff marker")
            .nth(1)
            .expect("handoff step が無い")
            .lines()
            .take_while(|line| !line.trim_start().starts_with("env:"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// **workflow が渡す `*_OUTCOME` env と [`OUTCOME_FIELDS`] が過不足なく一致すること。**
    ///
    /// 片方だけ増減すると「渡しているのに出ない」列や「出そうとして常に `<未実行>` の列」が
    /// 黙って生まれる。実際 CodeRabbit #445 が `LEDGER_COMPLETION_OUTCOME` の欠落を指摘した
    /// ように、この 2 か所の同期は目視では落ちる (memory: 外部 fixture 参照テストは値まで
    /// assert する)。
    ///
    /// cwd 依存のため `--ignored` (ADR-041 / `cargo test -- --ignored --test-threads=1`)。
    #[test]
    #[ignore = "cwd 依存: リポジトリルートの .github/workflows/nightly-todo.yml を読む。--test-threads=1 で実行"]
    fn the_summary_contract_covers_every_outcome_the_workflow_passes() {
        let content = read_nightly_workflow();
        let step = content
            .split("- name: Report outcome")
            .nth(1)
            .expect("Report outcome step が無い");
        let mut passed: Vec<&str> = step
            .lines()
            .take_while(|line| !line.trim_start().starts_with("run:"))
            .filter_map(|line| line.trim().split_once(':').map(|(key, _)| key))
            .filter(|key| key.ends_with("_OUTCOME"))
            .collect();
        passed.sort_unstable();
        let mut declared: Vec<&str> = OUTCOME_FIELDS.iter().map(|(_, env)| *env).collect();
        declared.sort_unstable();
        assert_eq!(
            passed, declared,
            "workflow の Report outcome step が渡す env と OUTCOME_FIELDS が食い違っている"
        );
    }
}
