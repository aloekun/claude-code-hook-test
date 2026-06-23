//! Deployed artifact (workspace `.claude/custom-lint-rules.toml` /
//! `.takt/workflows/*.yaml` / `src/**/*.rs`) に対する regression seal tests。

use super::engine::{
    compile_rule, find_powershell_rules_missing_case_insensitive_flag, run_custom_rules,
};
use super::types::{CompiledRule, CustomRule, CustomRuleExample, CustomRuleFix, CustomRulesConfig};

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
    }
}

fn compile_test_rules(rules: Vec<CustomRule>) -> Vec<CompiledRule> {
    rules.into_iter().filter_map(compile_rule).collect()
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

fn no_write_result_discard_rule() -> CustomRule {
    make_test_rule("no-write-result-discard", r"let\s+_\s*=\s+write_\w+\(", &["rs"])
}

fn takt_workflow_persona_without_model_rule() -> CustomRule {
    make_test_rule(
        "takt-workflow-persona-without-model",
        r"(?m)^[ \t]+persona:[ \t]+[\w-]+[ \t]*\r?\n[ \t]+(?:policy|instruction|edit|provider_options|knowledge|condition|rules|inputs|outputs|allowed_tools|disallowed_tools|name|type|cmd|when|description|tool|tools|output_contracts|pass_previous_response|required_permission_mode|parallel):",
        &["yaml"],
    )
}

#[test]
fn no_ephemeral_todo_self_exclusion_invariant_holds_on_deployed_toml() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".claude")
        .join("custom-lint-rules.toml");

    assert!(
        path.exists(),
        "deployed custom-lint-rules.toml not found at {:?} — \
         self-exclusion invariant test would silent-pass on missing file",
        path
    );

    let rule = no_ephemeral_todo_reference_rule();
    assert!(
        rule.extensions.iter().any(|e| e == "toml"),
        "rule⑥ extensions list does not contain \"toml\" — \
         self-exclusion invariant test would silent-pass on rule scope change. \
         extensions actual: {:?}",
        rule.extensions
    );

    let rules = compile_test_rules(vec![rule]);
    let violations = run_custom_rules(path.to_str().unwrap(), &rules);
    assert!(
        violations.is_empty(),
        "self-exclusion invariant broken: rule⑥ self-triggered on deployed custom-lint-rules.toml"
    );
}

#[test]
fn deployed_custom_rules_pass_powershell_case_insensitive_validation() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".claude")
        .join("custom-lint-rules.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read deployed custom-lint-rules.toml: {e}"));
    let config: CustomRulesConfig = toml::from_str(&content).unwrap();
    let rules = config.rules.unwrap_or_default();
    let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
    assert!(
        missing.is_empty(),
        "PowerShell rules without (?i) flag detected: {:?}",
        missing
    );
}

fn collect_rust_files(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name == "target" || file_name == "node_modules" || file_name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn deployed_src_rust_passes_no_write_result_discard_rule() {
    let src_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let rules = compile_test_rules(vec![no_write_result_discard_rule()]);
    let mut rust_files: Vec<std::path::PathBuf> = Vec::new();
    collect_rust_files(&src_root, &mut rust_files);
    assert!(
        !rust_files.is_empty(),
        "src/ 配下の .rs file が 0 件 — false-green guard (path resolution mistake?). \
         searched: {}",
        src_root.display()
    );
    let mut total_violations: Vec<String> = Vec::new();
    for path in &rust_files {
        let violations = run_custom_rules(path.to_str().unwrap(), &rules);
        for v in violations {
            total_violations.push(format!("{}: {}", path.display(), v));
        }
    }
    assert!(
        total_violations.is_empty(),
        "src/**/*.rs に let _ = write_*(...) swallowed error が残存。\
         if let Err(e) = ... {{ log_*(...) }} 形式に書き換えてください。違反内容: {:#?}",
        total_violations
    );
}

#[test]
fn deployed_takt_workflows_have_clean_baseline_for_persona_model_rule() {
    let workflows_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".takt")
        .join("workflows");
    let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
    let mut total_violations: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&workflows_dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", workflows_dir.display()))
    {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let violations = run_custom_rules(path.to_str().unwrap(), &rules);
            for v in violations {
                total_violations.push(format!("{}: {}", path.display(), v));
            }
        }
    }
    assert!(
        total_violations.is_empty(),
        ".takt/workflows/*.yaml で persona: → model: 不在 violation が検出されました。`model:` 行を追加してください。違反内容: {:?}",
        total_violations
    );
}
