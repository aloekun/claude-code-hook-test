//! Deployed artifact (workspace `config/custom-lint-rules.toml` /
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
        incident: None,
        exception: None,
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
        .join(lib_config_path::CONFIG_DIR)
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
        .join(lib_config_path::CONFIG_DIR)
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
    for entry in entries {
        let path = entry.expect("ディレクトリの entry を読めません (検査対象が欠けたまま通さない)").path();
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

fn is_excluded_scan_dir(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name == "target" || name == "node_modules" {
        return true;
    }
    if name.starts_with('.') {
        return true;
    }
    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if name == "fixtures" && parent_name == "tests" {
        return true;
    }
    if name == "runs" && parent_name == ".takt" {
        return true;
    }
    false
}

fn collect_files_with_extensions(
    root: &std::path::Path,
    extensions: &[String],
    out: &mut Vec<std::path::PathBuf>,
) {
    let entries = std::fs::read_dir(root).unwrap_or_else(|e| {
        panic!(
            "ディレクトリを読めません (検査対象が欠けたまま通さない): {}: {e}",
            root.display()
        )
    });
    for entry in entries {
        let path = entry
            .expect("ディレクトリの entry を読めません (検査対象が欠けたまま通さない)")
            .path();
        if path.is_dir() {
            if is_excluded_scan_dir(&path) {
                continue;
            }
            collect_files_with_extensions(&path, extensions, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                out.push(path);
            }
        }
    }
}

fn load_deployed_rule_by_id(id: &str) -> CustomRule {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(lib_config_path::CONFIG_DIR)
        .join("custom-lint-rules.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read deployed custom-lint-rules.toml: {e}"));
    let config: CustomRulesConfig = toml::from_str(&content).unwrap();
    config
        .rules
        .unwrap_or_default()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule '{id}' not found in deployed custom-lint-rules.toml"))
}

/// rule⑲ / rule⑳ 共用: deployed rule を `src` / `scripts` / `.github` / `.takt` 全体に当て、
/// 違反が 0 件であることを固定する (`tests/fixtures` / `.takt/runs` / `target` /
/// `node_modules` / その他ドット始まりディレクトリは除外)。
fn assert_clean_baseline_for_rule(rule_id: &str) {
    let rule = load_deployed_rule_by_id(rule_id);
    let extensions = rule.extensions.clone();
    let compiled = compile_test_rules(vec![rule]);
    assert_eq!(
        compiled.len(),
        1,
        "rule '{rule_id}' のコンパイルに失敗しました (invalid regex / exception / paths glob) — \
         clean-baseline test が 0 rules のまま silent-pass します"
    );

    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in ["src", "scripts", ".github", ".takt"] {
        collect_files_with_extensions(&repo_root.join(dir), &extensions, &mut files);
    }
    assert!(
        !files.is_empty(),
        "rule '{rule_id}' の走査対象 (src/scripts/.github/.takt) で対象拡張子のファイルが 0 件 \
         — false-green guard (path resolution mistake?). searched root: {}",
        repo_root.display()
    );

    let mut total_violations: Vec<String> = Vec::new();
    for path in &files {
        if let Err(e) = std::fs::read_to_string(path) {
            panic!(
                "ファイルを読めません (検査対象が欠けたまま通さない): {}: {e}",
                path.display()
            );
        }
        let violations = run_custom_rules(path.to_str().unwrap(), &compiled);
        for v in violations {
            total_violations.push(format!("{}: {}", path.display(), v));
        }
    }
    assert!(
        total_violations.is_empty(),
        "rule '{rule_id}' の既存違反が検出されました。clean baseline が崩れています: {:#?}",
        total_violations
    );
}

#[test]
fn deployed_tree_has_clean_baseline_for_gh_json_files_truncated_rule() {
    assert_clean_baseline_for_rule("gh-json-files-truncated");
}

#[test]
fn deployed_tree_has_clean_baseline_for_ref_destroying_push_without_lease_rule() {
    assert_clean_baseline_for_rule("ref-destroying-push-without-lease");
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
