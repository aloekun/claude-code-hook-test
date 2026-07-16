//! 実 Ollama 呼出を伴う end-to-end eval と、その結果レポート。
//!
//! 自動 gate から外れているのは実 Ollama を呼ぶ `run_lint_screen_against_all_fixtures`
//! だけ (`#[ignore]` + `LINT_SCREEN_EVALS` env opt-in)。同居する pure function の
//! unit test (`evals_are_opt_in_by_default` / `verdict_label_thresholds_match_phase_b_table`)
//! は軽量なので常時実行する。**このファイルに実 Ollama を呼ぶテストを追加するときは
//! 同じ opt-in ガードを通すこと** — `#[ignore]` だけでは gate の `--ignored` で走る。
//!
//! schema / metrics の常時実行テストは `main.rs` 側にある。

use crate::{
    agreement_metrics, build_confusion_matrix, load_eval_set, manifest_root, ratio_or_default,
    read_diff_body, AgreementMetrics, Eval, EvalSet, DECISION_LABELS,
};
use std::path::Path;

/// eval の実行を opt-in する env 変数。`#[ignore]` だけでは塞げない:
/// 呼出箇所 (quality_gate / takt fix step / 手動) がいずれも `--ignored` を
/// 無条件で付けるため素通りする。
const EVALS_ENV_VAR: &str = "LINT_SCREEN_EVALS";

/// truthy 語彙は push-runner の `parse_override_env` (`stages/pr_size_check.rs`)
/// に揃える。env を直接読まず `Option<&str>` を取るのは、env がプロセス全域の
/// 可変状態で並列テストと相性が悪いため (ADR-041)。
fn evals_opt_in(raw: Option<&str>) -> bool {
    raw.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// T1: eval が自動 gate から外れていること (env 未設定 = skip) を固定する。
#[test]
fn evals_are_opt_in_by_default() {
    assert!(!evals_opt_in(None), "env 未設定なら skip");
    assert!(!evals_opt_in(Some("0")), "falsy なら skip");
    assert!(evals_opt_in(Some("1")));
    assert!(evals_opt_in(Some(" TRUE ")), "trim + 大小無視");
}

/// Phase b 判定用の end-to-end test (実 Ollama 呼出)。
///
/// 起動方法 (`LINT_SCREEN_EVALS=1` が必須):
///   LINT_SCREEN_EVALS=1 cargo test -p cli-finding-classifier --test lint_screen_evals \
///     -- --ignored --nocapture run_lint_screen_against_all_fixtures
///
/// 前提: Ollama がローカルで起動 + mistral:7b モデル pull 済。
///
/// env opt-in なのは assert を持たない計測専用テストだから (判定は人間が出力を
/// 読んで行う)。自動 gate では実呼出の時間だけ払って何も検証しない。ADR-038 参照。
#[test]
#[ignore]
fn run_lint_screen_against_all_fixtures() {
    use lib_ollama_client::OllamaClient;
    use std::time::Duration;

    if !evals_opt_in(std::env::var(EVALS_ENV_VAR).ok().as_deref()) {
        println!(
            "skip: {EVALS_ENV_VAR} が未設定のため eval を実行しません。\n\
             実行するには {EVALS_ENV_VAR}=1 を設定してください (Ollama 起動 + mistral:7b pull 済が前提)。"
        );
        return;
    }

    let set = load_eval_set();
    let client = OllamaClient::new("http://localhost:11434", "mistral:7b")
        .with_timeout(Duration::from_secs(60))
        .with_temperature(0.0);
    let template = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/lint-screen.txt"),
    )
    .unwrap();

    println!("\n=== Phase b'/Bundle i evals: lint-screen end-to-end ===");
    let outcomes: Vec<EvalRunOutcome> = set
        .evals
        .iter()
        .map(|entry| run_single_eval(entry, &client, &template))
        .collect();

    report_summary(&set, &outcomes);
}

struct EvalRunOutcome {
    metrics: AgreementMetrics,
    latency_ms: u128,
}

fn run_single_eval(
    entry: &Eval,
    client: &lib_ollama_client::OllamaClient,
    template: &str,
) -> EvalRunOutcome {
    use cli_finding_classifier::screen_diff;
    use std::time::Instant;

    let diff = read_diff_body(&manifest_root().join(&entry.input_diff));
    let started = Instant::now();
    let result = screen_diff(client, template, &diff);
    let latency_ms = started.elapsed().as_millis();
    let metrics = agreement_metrics(&entry.claude_code_baseline, &result);

    println!(
        "eval {} ({}): decision={}->{} match={} P={:.0}%/{:.0}% R={:.0}%/{:.0}% F1={:.2} TP={}(norm {}) FP={} FN={} latency={}ms fallback={:?}",
        entry.id,
        entry.name,
        metrics.decision_pair.0,
        metrics.decision_pair.1,
        metrics.decision_match,
        metrics.precision() * 100.0,
        metrics.precision_normalized() * 100.0,
        metrics.recall() * 100.0,
        metrics.recall_normalized() * 100.0,
        metrics.f1(),
        metrics.true_positive_count,
        metrics.true_positive_normalized_count,
        metrics.false_positive_count,
        metrics.false_negative_count,
        latency_ms,
        result.fallback_reason,
    );
    EvalRunOutcome {
        metrics,
        latency_ms,
    }
}

fn print_confusion_matrix(matrix: &[[u32; 3]; 3]) {
    println!("decision confusion matrix (rows=baseline, cols=LLM):");
    println!("            auto_fix  human_review  informational");
    for (i, label) in DECISION_LABELS.iter().enumerate() {
        println!(
            "{:<14}{:>3}           {:>3}            {:>3}",
            label, matrix[i][0], matrix[i][1], matrix[i][2]
        );
    }
}

fn aggregate_finding_counts(outcomes: &[EvalRunOutcome]) -> (usize, usize, usize, usize) {
    let mut tp = 0usize;
    let mut tp_norm = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;
    for o in outcomes {
        tp += o.metrics.true_positive_count;
        tp_norm += o.metrics.true_positive_normalized_count;
        fp += o.metrics.false_positive_count;
        fn_ += o.metrics.false_negative_count;
    }
    (tp, tp_norm, fp, fn_)
}

fn verdict_label(agreement: f32, threshold: f32) -> &'static str {
    if agreement >= threshold {
        "GO (§8.E 着手)"
    } else if agreement >= 0.70 {
        "CONDITIONAL-GO (§8.E auto_fix lane に限定)"
    } else if agreement >= 0.60 {
        "LOOP-V3 (§8.D v3 ループ)"
    } else {
        "NO-GO (§8.E 却下判断)"
    }
}

#[test]
fn verdict_label_thresholds_match_phase_b_table() {
    assert_eq!(verdict_label(0.85, 0.80), "GO (§8.E 着手)");
    assert!(verdict_label(0.75, 0.80).contains("CONDITIONAL-GO"));
    assert!(verdict_label(0.65, 0.80).contains("LOOP-V3"));
    assert!(verdict_label(0.50, 0.80).contains("NO-GO"));
}

fn report_summary(set: &EvalSet, outcomes: &[EvalRunOutcome]) {
    let mut latencies_ms: Vec<u128> = outcomes.iter().map(|o| o.latency_ms).collect();
    latencies_ms.sort_unstable();
    let p50 = latencies_ms[latencies_ms.len() / 2];
    let p95_idx = (latencies_ms.len() as f32 * 0.95) as usize;
    let p95 = latencies_ms[p95_idx.min(latencies_ms.len() - 1)];
    let decision_matches = outcomes.iter().filter(|o| o.metrics.decision_match).count() as u32;
    let agreement = decision_matches as f32 / set.evals.len() as f32;
    let (tp, tp_norm, fp, fn_) = aggregate_finding_counts(outcomes);
    let agg_precision = ratio_or_default(tp, tp + fp, tp == 0 && fp == 0 && fn_ == 0);
    let agg_recall = ratio_or_default(tp, tp + fn_, tp == 0 && fp == 0 && fn_ == 0);
    let agg_precision_norm = ratio_or_default(tp_norm, tp + fp, tp == 0 && fp == 0 && fn_ == 0);
    let agg_recall_norm = ratio_or_default(tp_norm, tp + fn_, tp == 0 && fp == 0 && fn_ == 0);
    let pairs: Vec<(String, String)> = outcomes
        .iter()
        .map(|o| o.metrics.decision_pair.clone())
        .collect();
    let matrix = build_confusion_matrix(&pairs);

    println!("---");
    println!(
        "decision agreement rate = {decision_matches}/{} = {:.1}% (threshold {:.0}%)",
        set.evals.len(),
        agreement * 100.0,
        set.agreement_threshold * 100.0
    );
    println!(
        "aggregate precision={:.1}% recall={:.1}%  (normalized: P={:.1}% R={:.1}%)",
        agg_precision * 100.0,
        agg_recall * 100.0,
        agg_precision_norm * 100.0,
        agg_recall_norm * 100.0,
    );
    println!("latency p50={p50}ms p95={p95}ms");
    print_confusion_matrix(&matrix);
    println!(
        "Phase b verdict: {}",
        verdict_label(agreement, set.agreement_threshold)
    );
}
