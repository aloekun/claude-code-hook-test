//! Deployed rule (rules⑥〜) ごとの positive / negative test (part 2)。
//!
//! Test helper (`make_test_rule` 等) は memory `feedback_test_dry_antipattern` に従って
//! per-module で複製している (sibling module 間で共有しない)。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRuleExample, CustomRuleFix};

fn make_test_rule(id: &str, pattern: &str, extensions: &[&str]) -> CustomRule {
    CustomRule {
        id: id.into(),
        pattern: pattern.into(),
        severity: "error".into(),
        message: "test message".into(),
        why: "test reason".into(),
        extensions: extensions.iter().map(|e| e.to_string()).collect(),
        paths: None,
        fix: Some(CustomRuleFix {
            strategy: "test strategy".into(),
            steps: vec!["step1".into()],
        }),
        example: Some(CustomRuleExample {
            bad: "bad code".into(),
            good: "good code".into(),
        }),
        test_coverage: None,
        incident: None,
    }
}

fn compile_test_rules(rules: Vec<CustomRule>) -> Vec<CompiledRule> {
    rules.into_iter().filter_map(compile_rule).collect()
}

fn write_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    use std::io::Write;
    let file = dir.join(name);
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    file
}

fn no_ephemeral_todo_reference_rule() -> CustomRule {
    let stem = "todo";
    let pattern = format!(r"(?i)docs/{stem}[0-9]*\.md");
    make_test_rule(
        "no-ephemeral-todo-reference",
        &pattern,
        &[
            "rs", "toml", "jsonc", "json", "yaml", "yml", "ts", "tsx", "js", "jsx", "py", "ps1",
        ],
    )
}

fn build_concrete_digit_fixture(digit: u32) -> String {
    let stem = "todo";
    format!("const MSG: &str = \"see docs/{stem}{digit}.md\";\n")
}

fn build_zero_digit_fixture() -> String {
    let stem = "todo";
    format!("pub const NOTE: &str = \"linked from docs/{stem}.md baseline\";\n")
}

fn build_letter_placeholder_fixture() -> String {
    let stem = "todo";
    let placeholder = "N";
    format!(
        "/// example: \"docs/{stem}{placeholder}.md\" ({placeholder} = digit) is the placeholder form\n"
    )
}

fn build_asterisk_literal_fixture() -> String {
    let stem = "todo";
    let glob = "*";
    format!("pub const GLOB: &str = \"docs/{stem}{glob}.md\";\n")
}

#[test]
fn no_ephemeral_todo_detects_concrete_digit_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "config.rs", &build_concrete_digit_fixture(3));
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_ephemeral_todo_detects_zero_digit_form() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "lib.rs", &build_zero_digit_fixture());
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_ephemeral_todo_skips_letter_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "explainer.rs",
        &build_letter_placeholder_fixture(),
    );
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_ephemeral_todo_skips_asterisk_literal() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "doc_glob.rs", &build_asterisk_literal_fixture());
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_ephemeral_todo_only_targets_listed_extensions_md_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "note.md", &build_concrete_digit_fixture(3));
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_ephemeral_todo_detects_toml_ephemeral_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "config.toml", &build_concrete_digit_fixture(3));
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_ephemeral_todo_toml_skips_permanent_adr_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "config.toml",
        "doc_link = \"see docs/adr/adr-007-foo.md for context\"\n",
    );
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_ephemeral_todo_detects_yaml_ephemeral_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "workflow.yaml", &build_concrete_digit_fixture(3));
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_ephemeral_todo_yaml_skips_permanent_adr_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "workflow.yaml",
        "description: see docs/adr/adr-007-foo.md for context\n",
    );
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_ephemeral_todo_detects_yml_ephemeral_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "config.yml", &build_concrete_digit_fixture(7));
    let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

fn takt_workflow_persona_without_model_rule() -> CustomRule {
    make_test_rule(
        "takt-workflow-persona-without-model",
        r"(?m)^[ \t]+persona:[ \t]+[\w-]+[ \t]*\r?\n[ \t]+(?:policy|instruction|edit|provider_options|knowledge|condition|rules|inputs|outputs|allowed_tools|disallowed_tools|name|type|cmd|when|description|tool|tools|output_contracts|pass_previous_response|required_permission_mode|parallel):",
        &["yaml"],
    )
}

#[test]
fn takt_workflow_persona_detects_judge_block_violation() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "loop_monitors:\n  - cycle:\n      - analyze\n      - fix\n    judge:\n      persona: supervisor\n      instruction: loop-monitor-reviewers-fix\n";
    let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn takt_workflow_persona_detects_supervise_step_violation() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "steps:\n  - name: supervise\n    edit: false\n    persona: supervisor\n    policy: review\n";
    let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn takt_workflow_persona_skips_when_model_directly_follows() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "steps:\n  - name: supervise\n    edit: false\n    persona: supervisor\n    model: sonnet\n    policy: review\n";
    let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn takt_workflow_persona_detects_multiple_violations_in_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "loop_monitors:\n  - cycle:\n      - analyze\n    judge:\n      persona: supervisor\n      instruction: monitor\nsteps:\n  - name: supervise\n    persona: supervisor\n    policy: review\n";
    let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 2);
}

#[test]
fn takt_workflow_persona_detects_required_permission_mode_violation() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "steps:\n  - name: fix\n    persona: coder\n    required_permission_mode: edit\n";
    let file = write_file(dir.path(), "pre-push-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn takt_workflow_persona_detects_pass_previous_response_violation() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "steps:\n  - name: review\n    persona: code-reviewer\n    pass_previous_response: false\n";
    let file = write_file(dir.path(), "pre-push-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn takt_workflow_persona_detects_output_contracts_violation() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "steps:\n  - name: review\n    persona: simplicity-reviewer\n    output_contracts:\n      - approve\n";
    let file = write_file(dir.path(), "pre-push-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn takt_workflow_persona_detects_parallel_violation() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "steps:\n  - name: review\n    persona: code-reviewer\n    parallel: true\n";
    let file = write_file(dir.path(), "pre-push-review.yaml", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn takt_workflow_persona_skips_non_yaml_extension() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "persona: supervisor\ninstruction: loop\n";
    let file = write_file(dir.path(), "fake.md", fixture);
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

fn no_write_result_discard_rule() -> CustomRule {
    make_test_rule("no-write-result-discard", r"let\s+_\s*=\s+write_\w+\(", &["rs"])
}

fn build_write_discard_fixture(callee: &str) -> String {
    format!("fn run() {{ let _ = {}(arg); }}\n", callee)
}

fn build_drop_write_discard_fixture(callee: &str) -> String {
    format!(
        "impl Drop for G {{ fn drop(&mut self) {{ let _ = {}(self.path); }} }}\n",
        callee
    )
}

fn build_if_let_err_fixture(callee: &str) -> String {
    format!(
        "fn run() {{ if let Err(e) = {}(arg) {{ log_warn(&e.to_string()); }} }}\n",
        callee
    )
}

fn build_non_write_prefix_fixture() -> String {
    let prefix = "let _";
    format!("fn run() {{ {prefix} = stream.flush(); {prefix} = drop(handle); {prefix} = sender.send(msg); }}\n")
}

fn build_named_binding_fixture(callee: &str) -> String {
    format!(
        "fn run() {{ let _result = {}(arg); println!(\"{{:?}}\", _result); }}\n",
        callee
    )
}

#[test]
fn no_write_result_discard_detects_simple_let_underscore() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "stage.rs",
        &build_write_discard_fixture("write_state"),
    );
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_write_result_discard_detects_write_skip_report_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "stage.rs",
        &build_write_discard_fixture("write_skip_report"),
    );
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_write_result_discard_detects_write_failed_marker_in_drop() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "guard.rs",
        &build_drop_write_discard_fixture("write_failed_marker"),
    );
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_write_result_discard_skips_proper_if_let_err_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "stage.rs",
        &build_if_let_err_fixture("write_state"),
    );
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_write_result_discard_skips_non_write_prefix_calls() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "stage.rs", &build_non_write_prefix_fixture());
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_write_result_discard_skips_named_binding_starting_with_underscore() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "stage.rs",
        &build_named_binding_fixture("write_state"),
    );
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_write_result_discard_only_targets_rust_extension() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "doc.md",
        &build_write_discard_fixture("write_state"),
    );
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

fn no_jj_template_first_line_rule() -> CustomRule {
    make_test_rule(
        "no-jj-template-first-line",
        r"description\.first_line\(\)",
        &["toml", "yaml", "md"],
    )
}

fn build_first_line_fixture(label: &str) -> String {
    let bad_method = format!("description{}{}", ".", "first_line()");
    format!("{} = \"jj log -T 'change_id ++ {}'\"\n", label, bad_method)
}

fn build_empty_keyword_fixture(label: &str) -> String {
    format!(
        "{} = \"jj log -T 'change_id ++ if(empty, EMPTY, CONTENT)'\"\n",
        label
    )
}

#[test]
fn no_jj_template_first_line_detects_toml_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "rule.toml", &build_first_line_fixture("command"));
    let rules = compile_test_rules(vec![no_jj_template_first_line_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_jj_template_first_line_toml_skips_empty_keyword() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "rule.toml", &build_empty_keyword_fixture("command"));
    let rules = compile_test_rules(vec![no_jj_template_first_line_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_jj_template_first_line_detects_yaml_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "workflow.yaml",
        &build_first_line_fixture("template"),
    );
    let rules = compile_test_rules(vec![no_jj_template_first_line_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_jj_template_first_line_yaml_skips_empty_keyword() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "workflow.yaml",
        &build_empty_keyword_fixture("template"),
    );
    let rules = compile_test_rules(vec![no_jj_template_first_line_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_jj_template_first_line_detects_md_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "doc.md", &build_first_line_fixture("snippet"));
    let rules = compile_test_rules(vec![no_jj_template_first_line_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

fn no_hardcoded_jj_revset_range_rule() -> CustomRule {
    make_test_rule("no-hardcoded-jj-revset-range", r"master\.\.@", &["rs"])
}

fn build_hardcoded_revset_fixture(branch: &str) -> String {
    format!(
        "fn count() {{ let revset = \"{}..@\"; let _ = revset; }}\n",
        branch
    )
}

fn build_empty_filter_revset_fixture(branch: &str) -> String {
    format!(
        "fn count() {{ let revset = \"empty() & ({}..@)\"; let _ = revset; }}\n",
        branch
    )
}

fn build_parameterized_revset_fixture() -> String {
    "fn count(default_branch: &str) { let revset = format!(\"{}..@\", default_branch); let _ = revset; }\n"
        .to_string()
}

#[test]
fn no_hardcoded_jj_revset_range_detects_simple_hardcode() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "fix_commit.rs",
        &build_hardcoded_revset_fixture("master"),
    );
    let rules = compile_test_rules(vec![no_hardcoded_jj_revset_range_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_hardcoded_jj_revset_range_detects_within_empty_filter() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "fix_commit.rs",
        &build_empty_filter_revset_fixture("master"),
    );
    let rules = compile_test_rules(vec![no_hardcoded_jj_revset_range_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_hardcoded_jj_revset_range_skips_parameterized_format() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "fix_commit.rs", &build_parameterized_revset_fixture());
    let rules = compile_test_rules(vec![no_hardcoded_jj_revset_range_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn no_hardcoded_jj_revset_range_skips_other_branch_literal() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "fix_commit.rs",
        &build_hardcoded_revset_fixture("main"),
    );
    let rules = compile_test_rules(vec![no_hardcoded_jj_revset_range_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}
