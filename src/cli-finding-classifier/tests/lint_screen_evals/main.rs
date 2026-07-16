//! Phase a evals integration test
//!
//! `evals/lint-screen-evals.json` を読み込み、各 fixture の Claude Code baseline と
//! mistral:7b 出力を突合する。ADR-038 Phase a evals infrastructure の
//! deliverable D5 に対応 (旧 docs/local-llm-offload-analysis.md §11.6、retire 済)。
//!
//! 構成:
//! - JSON / fixture の構造を検証する schema test (常時実行) — 本ファイル
//! - `agreement_metrics` の pure function を検証する unit test (常時実行) — 本ファイル
//! - 実 Ollama 呼出を伴う end-to-end test (`#[ignore]` + env opt-in、ローカル限定) — `e2e.rs`
//!
//! end-to-end test の起動 (`LINT_SCREEN_EVALS=1` が必須):
//!   LINT_SCREEN_EVALS=1 cargo test -p cli-finding-classifier --test lint_screen_evals \
//!     -- --ignored --nocapture run_lint_screen_against_all_fixtures

mod e2e;

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

/// fixture の `#` で始まる leading コメントヘッダ (ADR-038 SYNTHETIC FIXTURE block) を skip し、
/// `diff --git` 以降の純粋な diff body を返す。LLM 入力にメタ情報を混入させないため。
fn read_diff_body(path: &Path) -> String {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    content
        .lines()
        .skip_while(|line| line.starts_with('#') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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
    decision_pair: (String, String),
    baseline_finding_count: usize,
    llm_finding_count: usize,
    true_positive_count: usize,
    false_positive_count: usize,
    false_negative_count: usize,
    true_positive_normalized_count: usize,
}

impl AgreementMetrics {
    fn precision(&self) -> f32 {
        ratio_or_default(
            self.true_positive_count,
            self.true_positive_count + self.false_positive_count,
            self.llm_finding_count == 0 && self.baseline_finding_count == 0,
        )
    }

    fn recall(&self) -> f32 {
        ratio_or_default(
            self.true_positive_count,
            self.true_positive_count + self.false_negative_count,
            self.baseline_finding_count == 0 && self.llm_finding_count == 0,
        )
    }

    fn f1(&self) -> f32 {
        let p = self.precision();
        let r = self.recall();
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    fn precision_normalized(&self) -> f32 {
        ratio_or_default(
            self.true_positive_normalized_count,
            self.llm_finding_count,
            self.llm_finding_count == 0 && self.baseline_finding_count == 0,
        )
    }

    fn recall_normalized(&self) -> f32 {
        ratio_or_default(
            self.true_positive_normalized_count,
            self.baseline_finding_count,
            self.baseline_finding_count == 0 && self.llm_finding_count == 0,
        )
    }
}

fn ratio_or_default(numerator: usize, denominator: usize, both_empty: bool) -> f32 {
    if denominator == 0 {
        if both_empty {
            1.0
        } else {
            0.0
        }
    } else {
        numerator as f32 / denominator as f32
    }
}

/// rule 名を canonical form に正規化 (大小文字・記号揺れ・oxlint/biome シノニムを吸収)。
///
/// LLM は同じ概念に対して `no-var` / `var-keyword` / `unused-variable` 等のバリアントを
/// 出力する。Phase b の eval6 で 25% 一致まで agreement が落ちた主因。
fn normalize_rule_name(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "no-var" | "var-keyword" | "no-vars" | "var" => "no-var",
        "no-unused-vars" | "unused-vars" | "unused-variable" | "unused-variables" => {
            "no-unused-vars"
        }
        "unused-import" | "unused-imports" | "no-unused-imports" => "unused-import",
        "magic-number" | "magic-numbers" | "magic-num" | "no-magic-number" | "no-magic-numbers" => {
            "magic-number"
        }
        "deep-nesting" | "max-depth" | "deep-nest" | "nested-conditions" | "max-nesting" => {
            "deep-nesting"
        }
        "dead-code" | "dead_code" | "unused-code" | "no-dead-code" => "dead-code",
        "complexity" | "cognitive-complexity" | "cyclomatic" | "max-complexity" => "complexity",
        _ => return lower,
    }
    .to_string()
}

fn agreement_metrics(baseline: &Baseline, llm: &LintScreenResult) -> AgreementMetrics {
    let decision_match = baseline.screen_decision == llm.screen_decision;
    let mut tp = 0usize;
    let mut tp_norm = 0usize;
    for b in &baseline.lint_findings {
        if llm.lint_findings.iter().any(|l| finding_matches(b, l)) {
            tp += 1;
        }
        if llm
            .lint_findings
            .iter()
            .any(|l| finding_matches_normalized(b, l))
        {
            tp_norm += 1;
        }
    }
    let baseline_count = baseline.lint_findings.len();
    let llm_count = llm.lint_findings.len();
    let fp = llm_count.saturating_sub(tp);
    let fn_ = baseline_count.saturating_sub(tp);

    AgreementMetrics {
        decision_match,
        decision_pair: (
            baseline.screen_decision.clone(),
            llm.screen_decision.clone(),
        ),
        baseline_finding_count: baseline_count,
        llm_finding_count: llm_count,
        true_positive_count: tp,
        false_positive_count: fp,
        false_negative_count: fn_,
        true_positive_normalized_count: tp_norm,
    }
}

fn finding_matches(b: &BaselineFinding, l: &LintFinding) -> bool {
    b.rule == l.rule && b.file == l.file
}

fn finding_matches_normalized(b: &BaselineFinding, l: &LintFinding) -> bool {
    b.file == l.file && normalize_rule_name(&b.rule) == normalize_rule_name(&l.rule)
}

const DECISION_LABELS: &[&str] = &["auto_fix", "human_review", "informational"];

fn decision_index(d: &str) -> Option<usize> {
    DECISION_LABELS.iter().position(|&label| label == d)
}

fn build_confusion_matrix(pairs: &[(String, String)]) -> [[u32; 3]; 3] {
    let mut matrix = [[0u32; 3]; 3];
    for (baseline_d, llm_d) in pairs {
        if let (Some(r), Some(c)) = (decision_index(baseline_d), decision_index(llm_d)) {
            matrix[r][c] += 1;
        }
    }
    matrix
}

#[test]
fn eval_set_loads_and_has_at_least_phase_b_prime_baseline_count() {
    let set = load_eval_set();
    assert_eq!(set.schema_version, 1);
    assert!(set.agreement_threshold >= 0.5 && set.agreement_threshold <= 1.0);
    assert!(
        set.evals.len() >= 15,
        "Bundle i baseline は 15 fixtures 以上を維持する必要があります (現状 {})",
        set.evals.len()
    );
}

/// Bundle i (Phase d 着手前必須) で eval13/14/15 を追加し 15 件に到達したことを検証。
///
/// ADR-038 Phase c+ で要求された scale-aware fixture (200+ 行 / 3 件、Bundle i 由来)
/// が実体として存在することを最低限の重複スモークでガードする。
#[test]
fn eval_set_includes_bundle_i_scale_aware_fixtures() {
    let set = load_eval_set();
    let names: Vec<&str> = set.evals.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names
            .iter()
            .any(|n| n.contains("large-refactor-real")),
        "eval13 (large-refactor-real-context-stress) が必要 (現状: {names:?})"
    );
    assert!(
        names.iter().any(|n| n.contains("mid-mixed")),
        "eval14 (mid-mixed-recall-stability) が必要 (現状: {names:?})"
    );
    assert!(
        names.iter().any(|n| n.contains("syntax-stress")),
        "eval15 (syntax-stress-single-file) が必要 (現状: {names:?})"
    );
}

#[test]
fn eval_set_screen_decision_distribution_covers_all_three_lanes() {
    let set = load_eval_set();
    let mut counts = std::collections::HashMap::new();
    for entry in &set.evals {
        *counts
            .entry(entry.claude_code_baseline.screen_decision.clone())
            .or_insert(0u32) += 1;
    }
    assert!(
        counts.get("auto_fix").copied().unwrap_or(0) >= 2,
        "auto_fix lane に複数の eval が必要 (現状: {:?})",
        counts
    );
    assert!(
        counts.get("human_review").copied().unwrap_or(0) >= 1,
        "human_review lane を必ず 1 件以上カバー (現状: {:?})",
        counts
    );
    assert!(
        counts.get("informational").copied().unwrap_or(0) >= 3,
        "informational lane (FP 検知 + boundary + test-only 等) 3 件以上必要 (現状: {:?})",
        counts
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
        let body = read_diff_body(&diff_path);
        assert!(
            body.starts_with("diff --git "),
            "eval {}: {} does not look like a unified diff (after skipping `#` header)",
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
    assert_eq!(m.true_positive_count, 1);
    assert_eq!(m.recall(), 1.0);
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
fn agreement_metrics_match_ignores_line_position() {
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
    assert_eq!(m.true_positive_count, 1);
    assert_eq!(m.recall(), 1.0);
}

#[test]
fn agreement_metrics_match_requires_rule_and_file() {
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
            rule: "unused-import".into(),
            file: "src/x.rs".into(),
            line: 10,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "auto_fix".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert_eq!(m.true_positive_count, 0);
    assert_eq!(m.recall(), 0.0);
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
    assert_eq!(m.recall(), 1.0, "both empty = perfect overlap");
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
    assert_eq!(m.recall(), 0.0, "LLM-side false positive");
    assert_eq!(m.false_positive_count, 1);
    assert_eq!(m.false_negative_count, 0);
    assert_eq!(m.precision(), 0.0);
}

#[test]
fn normalize_rule_name_maps_known_synonyms_to_canonical() {
    assert_eq!(normalize_rule_name("var-keyword"), "no-var");
    assert_eq!(normalize_rule_name("No-Var"), "no-var");
    assert_eq!(normalize_rule_name("unused-variable"), "no-unused-vars");
    assert_eq!(normalize_rule_name("unused-imports"), "unused-import");
    assert_eq!(normalize_rule_name("magic-numbers"), "magic-number");
    assert_eq!(normalize_rule_name("max-depth"), "deep-nesting");
    assert_eq!(normalize_rule_name("cognitive-complexity"), "complexity");
}

#[test]
fn normalize_rule_name_passes_unknown_through_lowercased() {
    assert_eq!(normalize_rule_name("BogusRule"), "bogusrule");
    assert_eq!(normalize_rule_name("bespoke-pattern"), "bespoke-pattern");
}

#[test]
fn finding_matches_normalized_recovers_synonym_mismatch() {
    let baseline = BaselineFinding {
        severity: "minor".into(),
        rule: "no-var".into(),
        file: "x.ts".into(),
        line: 1,
        issue: "i".into(),
        suggestion: "s".into(),
    };
    let llm = LintFinding {
        severity: "minor".into(),
        rule: "var-keyword".into(),
        file: "x.ts".into(),
        line: 99,
        issue: "i".into(),
        suggestion: "s".into(),
    };
    assert!(!finding_matches(&baseline, &llm));
    assert!(finding_matches_normalized(&baseline, &llm));
}

#[test]
fn agreement_metrics_separates_strict_and_normalized_tp() {
    let baseline = Baseline {
        lint_findings: vec![BaselineFinding {
            severity: "minor".into(),
            rule: "no-var".into(),
            file: "x.ts".into(),
            line: 1,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "informational".into(),
    };
    let llm = LintScreenResult {
        lint_findings: vec![LintFinding {
            severity: "minor".into(),
            rule: "var-keyword".into(),
            file: "x.ts".into(),
            line: 1,
            issue: "i".into(),
            suggestion: "s".into(),
        }],
        screen_decision: "informational".into(),
        fallback_reason: None,
    };
    let m = agreement_metrics(&baseline, &llm);
    assert_eq!(m.true_positive_count, 0);
    assert_eq!(m.true_positive_normalized_count, 1);
    assert!(m.recall_normalized() > m.recall());
}

#[test]
fn build_confusion_matrix_counts_decision_pairs() {
    let pairs = vec![
        ("auto_fix".to_string(), "auto_fix".to_string()),
        ("human_review".to_string(), "auto_fix".to_string()),
        ("informational".to_string(), "informational".to_string()),
        ("auto_fix".to_string(), "auto_fix".to_string()),
    ];
    let matrix = build_confusion_matrix(&pairs);
    assert_eq!(matrix[0][0], 2);
    assert_eq!(matrix[1][0], 1);
    assert_eq!(matrix[2][2], 1);
    assert_eq!(matrix[2][0], 0);
}

