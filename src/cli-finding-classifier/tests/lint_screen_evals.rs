//! Phase a evals integration test
//!
//! `evals/lint-screen-evals.json` を読み込み、各 fixture の Claude Code baseline と
//! mistral:7b 出力を突合する。docs/local-llm-offload-analysis.md §11.6 Phase a の
//! deliverable D5 に対応。
//!
//! 構成:
//! - JSON / fixture の構造を検証する schema test (常時実行)
//! - `agreement_metrics` の pure function を検証する unit test (常時実行)
//! - 実 Ollama 呼出を伴う end-to-end test (`#[ignore]` 付き、ローカル限定)
//!
//! end-to-end test の起動:
//!   cargo test -p cli-finding-classifier --test lint_screen_evals \
//!     -- --ignored --nocapture run_lint_screen_against_all_fixtures

use cli_finding_classifier::{LintFinding, LintScreenResult};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug)]
struct EvalSet {
    schema_version: u32,
    agreement_threshold: f32,
    evals: Vec<Eval>,
}

#[derive(Deserialize, Debug)]
struct Eval {
    id: u32,
    name: String,
    input_diff: String,
    claude_code_baseline: Baseline,
    expectations: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Baseline {
    lint_findings: Vec<BaselineFinding>,
    screen_decision: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
struct BaselineFinding {
    severity: String,
    rule: String,
    file: String,
    line: u32,
    #[allow(dead_code)]
    issue: String,
    #[allow(dead_code)]
    suggestion: String,
}

const VALID_SCREEN_DECISIONS: &[&str] = &["auto_fix", "human_review", "informational"];
const VALID_SEVERITIES: &[&str] = &["minor", "major", "critical"];

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load_eval_set() -> EvalSet {
    let path = manifest_root().join("evals/lint-screen-evals.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("invalid eval JSON {}: {e}", path.display()))
}

#[derive(Debug, PartialEq)]
struct AgreementMetrics {
    decision_match: bool,
    finding_overlap_count: usize,
    baseline_finding_count: usize,
    llm_finding_count: usize,
}

impl AgreementMetrics {
    fn overlap_ratio(&self) -> f32 {
        if self.baseline_finding_count == 0 {
            if self.llm_finding_count == 0 {
                1.0
            } else {
                0.0
            }
        } else {
            self.finding_overlap_count as f32 / self.baseline_finding_count as f32
        }
    }
}

/// baseline と LLM 出力の突合 metrics を計算 (pure function、CI で常時実行可能)
///
/// finding overlap は (rule, file) 一致 + line が ±2 行以内で同一視。
fn agreement_metrics(baseline: &Baseline, llm: &LintScreenResult) -> AgreementMetrics {
    let decision_match = baseline.screen_decision == llm.screen_decision;
    let mut overlap = 0;
    for b in &baseline.lint_findings {
        if llm.lint_findings.iter().any(|l| finding_matches(b, l)) {
            overlap += 1;
        }
    }
    AgreementMetrics {
        decision_match,
        finding_overlap_count: overlap,
        baseline_finding_count: baseline.lint_findings.len(),
        llm_finding_count: llm.lint_findings.len(),
    }
}

fn finding_matches(b: &BaselineFinding, l: &LintFinding) -> bool {
    b.rule == l.rule && b.file == l.file && (b.line as i64 - l.line as i64).abs() <= 2
}

#[test]
fn eval_set_loads_and_has_initial_six_entries() {
    let set = load_eval_set();
    assert_eq!(set.schema_version, 1);
    assert!(set.agreement_threshold >= 0.5 && set.agreement_threshold <= 1.0);
    assert_eq!(
        set.evals.len(),
        6,
        "Phase a initial scope is 6 fixtures (§11.6)"
    );
}

#[test]
fn eval_ids_are_unique_and_sequential() {
    let set = load_eval_set();
    let ids: Vec<u32> = set.evals.iter().map(|e| e.id).collect();
    let expected: Vec<u32> = (1..=ids.len() as u32).collect();
    assert_eq!(ids, expected, "eval ids must be 1..=N sequential");
}

#[test]
fn each_eval_references_existing_diff_file() {
    let set = load_eval_set();
    let root = manifest_root();
    for entry in &set.evals {
        let diff_path = root.join(&entry.input_diff);
        assert!(
            diff_path.exists(),
            "eval {} ({}): diff file not found at {}",
            entry.id,
            entry.name,
            diff_path.display()
        );
        let content = std::fs::read_to_string(&diff_path).unwrap();
        assert!(
            content.starts_with("diff --git "),
            "eval {}: {} does not look like a unified diff",
            entry.id,
            entry.input_diff
        );
    }
}

#[test]
fn each_baseline_uses_valid_screen_decision_and_severities() {
    let set = load_eval_set();
    for entry in &set.evals {
        let dec = &entry.claude_code_baseline.screen_decision;
        assert!(
            VALID_SCREEN_DECISIONS.contains(&dec.as_str()),
            "eval {}: invalid screen_decision '{dec}'",
            entry.id
        );
        for f in &entry.claude_code_baseline.lint_findings {
            assert!(
                VALID_SEVERITIES.contains(&f.severity.as_str()),
                "eval {}: invalid severity '{}' in finding rule={}",
                entry.id,
                f.severity,
                f.rule
            );
        }
    }
}

#[test]
fn each_eval_has_at_least_one_expectation() {
    let set = load_eval_set();
    for entry in &set.evals {
        assert!(
            !entry.expectations.is_empty(),
            "eval {} ({}) has no expectations",
            entry.id,
            entry.name
        );
    }
}

#[test]
fn clean_diff_baseline_has_zero_findings() {
    let set = load_eval_set();
    let clean = set
        .evals
        .iter()
        .find(|e| e.name == "clean-diff-no-false-positive")
        .expect("eval4 (clean-diff-no-false-positive) is required for false-positive detection");
    assert_eq!(
        clean.claude_code_baseline.lint_findings.len(),
        0,
        "clean fixture must have zero baseline findings (it is the FP-detection axis)"
    );
}

#[test]
fn agreement_metrics_perfect_match() {
    let baseline = Baseline {
        lint_findings: vec![BaselineFinding {
            severity: "minor".into(),
            rule: "unused-import".into(),
            file: "src/x.rs".into(),
            line: 1,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "auto_fix".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![LintFinding {
            severity: "minor".into(),
            rule: "unused-import".into(),
            file: "src/x.rs".into(),
            line: 1,
            issue: "x".into(),
            suggestion: "y".into(),
        }],
        screen_decision: "auto_fix".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert!(m.decision_match);
    assert_eq!(m.finding_overlap_count, 1);
    assert_eq!(m.overlap_ratio(), 1.0);
}

#[test]
fn agreement_metrics_decision_mismatch() {
    let baseline = Baseline {
        lint_findings: vec![],
        screen_decision: "informational".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![],
        screen_decision: "auto_fix".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert!(!m.decision_match);
}

#[test]
fn agreement_metrics_finding_line_within_two_rows_matches() {
    let baseline = Baseline {
        lint_findings: vec![BaselineFinding {
            severity: "minor".into(),
            rule: "magic-number".into(),
            file: "src/x.rs".into(),
            line: 10,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "auto_fix".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![LintFinding {
            severity: "minor".into(),
            rule: "magic-number".into(),
            file: "src/x.rs".into(),
            line: 12,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "auto_fix".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert_eq!(m.finding_overlap_count, 1);
}

#[test]
fn agreement_metrics_finding_line_far_off_does_not_match() {
    let baseline = Baseline {
        lint_findings: vec![BaselineFinding {
            severity: "minor".into(),
            rule: "magic-number".into(),
            file: "src/x.rs".into(),
            line: 10,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "auto_fix".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![LintFinding {
            severity: "minor".into(),
            rule: "magic-number".into(),
            file: "src/x.rs".into(),
            line: 50,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "auto_fix".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert_eq!(m.finding_overlap_count, 0);
    assert_eq!(m.overlap_ratio(), 0.0);
}

#[test]
fn agreement_metrics_empty_both_sides_overlap_one() {
    let baseline = Baseline {
        lint_findings: vec![],
        screen_decision: "informational".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![],
        screen_decision: "informational".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert!(m.decision_match);
    assert_eq!(m.overlap_ratio(), 1.0, "both empty = perfect overlap");
}

#[test]
fn agreement_metrics_baseline_empty_llm_nonempty_is_zero_overlap() {
    let baseline = Baseline {
        lint_findings: vec![],
        screen_decision: "informational".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![LintFinding {
            severity: "minor".into(),
            rule: "x".into(),
            file: "f".into(),
            line: 1,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "informational".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert_eq!(m.overlap_ratio(), 0.0, "LLM-side false positive");
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

    let diff = std::fs::read_to_string(manifest_root().join(&entry.input_diff)).unwrap();
    let started = Instant::now();
    let result = screen_diff(client, template, &diff);
    let latency_ms = started.elapsed().as_millis();
    let metrics = agreement_metrics(&entry.claude_code_baseline, &result);

    println!(
        "eval {} ({}): decision_match={} overlap={:.0}% baseline={} llm={} latency={}ms fallback={:?}",
        entry.id,
        entry.name,
        metrics.decision_match,
        metrics.overlap_ratio() * 100.0,
        metrics.baseline_finding_count,
        metrics.llm_finding_count,
        latency_ms,
        result.fallback_reason,
    );
    EvalRunOutcome {
        metrics,
        latency_ms,
    }
}

fn report_summary(set: &EvalSet, decision_matches: u32, mut latencies_ms: Vec<u128>) {
    latencies_ms.sort_unstable();
    let p50 = latencies_ms[latencies_ms.len() / 2];
    let p95_idx = (latencies_ms.len() as f32 * 0.95) as usize;
    let p95 = latencies_ms[p95_idx.min(latencies_ms.len() - 1)];
    let agreement = decision_matches as f32 / set.evals.len() as f32;

    println!("---");
    println!(
        "agreement rate = {decision_matches}/{} = {:.1}% (threshold {:.0}%)",
        set.evals.len(),
        agreement * 100.0,
        set.agreement_threshold * 100.0
    );
    println!("latency p50={p50}ms p95={p95}ms");
    println!(
        "Phase b GO/NO-GO: {}",
        if agreement >= set.agreement_threshold {
            "GO (§8.E 着手)"
        } else {
            "NO-GO (§8.D prompt v2 先行)"
        }
    );
}

/// Phase b 判定用の end-to-end test (実 Ollama 呼出)。
///
/// 起動方法:
///   cargo test -p cli-finding-classifier --test lint_screen_evals \
///     -- --ignored --nocapture run_lint_screen_against_all_fixtures
///
/// 前提: Ollama がローカルで起動 + mistral:7b モデル pull 済。
#[test]
#[ignore]
fn run_lint_screen_against_all_fixtures() {
    use lib_ollama_client::OllamaClient;
    use std::time::Duration;

    let set = load_eval_set();
    let client = OllamaClient::new("http://localhost:11434", "mistral:7b")
        .with_timeout(Duration::from_secs(60));
    let template = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/lint-screen.txt"),
    )
    .unwrap();

    let mut decision_matches = 0u32;
    let mut latencies_ms: Vec<u128> = Vec::new();

    println!("\n=== Phase a evals: lint-screen end-to-end ===");
    for entry in &set.evals {
        let outcome = run_single_eval(entry, &client, &template);
        if outcome.metrics.decision_match {
            decision_matches += 1;
        }
        latencies_ms.push(outcome.latency_ms);
    }

    report_summary(&set, decision_matches, latencies_ms);
}
