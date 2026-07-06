//! Engine 自体の挙動 test: rule_matches_ext / paths filter / cap (MAX_CUSTOM_VIOLATIONS) /
//! invalid regex filter / 構造化 JSON 出力 / TOML パース。

use super::engine::{
    compile_rule, find_powershell_rules_missing_case_insensitive_flag, rule_matches_ext,
    rule_matches_path, run_custom_rules,
};
use super::types::{
    CompiledRule, CustomRule, CustomRuleExample, CustomRuleFix, CustomRulesConfig,
};
use crate::violation::MAX_CUSTOM_VIOLATIONS;

pub(super) fn make_test_rule(id: &str, pattern: &str, extensions: &[&str]) -> CustomRule {
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

pub(super) fn make_test_rule_with_paths(
    id: &str,
    pattern: &str,
    extensions: &[&str],
    paths: &[&str],
) -> CustomRule {
    let mut rule = make_test_rule(id, pattern, extensions);
    rule.paths = Some(paths.iter().map(|p| p.to_string()).collect());
    rule
}

pub(super) fn compile_test_rules(rules: Vec<CustomRule>) -> Vec<CompiledRule> {
    rules.into_iter().filter_map(compile_rule).collect()
}

#[test]
fn rule_matches_ts_extension() {
    let rule = make_test_rule("test", "pattern", &["ts", "tsx"]);
    assert!(rule_matches_ext(&rule, "src/app.ts"));
    assert!(rule_matches_ext(&rule, "src/App.tsx"));
}

#[test]
fn rule_does_not_match_other_extension() {
    let rule = make_test_rule("test", "pattern", &["ts"]);
    assert!(!rule_matches_ext(&rule, "main.rs"));
    assert!(!rule_matches_ext(&rule, "style.css"));
}

#[test]
fn rule_matches_case_insensitive() {
    let rule = make_test_rule("test", "pattern", &["ts"]);
    assert!(rule_matches_ext(&rule, "file.TS"));
    assert!(rule_matches_ext(&rule, "file.Ts"));
}

#[test]
fn rule_no_match_for_no_extension() {
    let rule = make_test_rule("test", "pattern", &["ts"]);
    assert!(!rule_matches_ext(&rule, "Makefile"));
    assert!(!rule_matches_ext(&rule, ""));
}

#[test]
fn rule_matches_windows_path() {
    let rule = make_test_rule("test", "pattern", &["ts"]);
    assert!(rule_matches_ext(&rule, r"e:\work\project\src\app.ts"));
}

#[test]
fn paths_filter_none_accepts_any_path() {
    let rule = make_test_rule("test", "x", &["md"]);
    let compiled = compile_rule(rule).expect("rule must compile");
    assert!(rule_matches_path(&compiled, "any/file.md"));
    assert!(rule_matches_path(&compiled, "docs/adr/foo.md"));
    assert!(rule_matches_path(&compiled, "README.md"));
}

#[test]
fn paths_filter_empty_vec_accepts_any_path() {
    let rule = make_test_rule_with_paths("test", "x", &["md"], &[]);
    let compiled = compile_rule(rule).expect("rule must compile");
    assert!(rule_matches_path(&compiled, "any/file.md"));
}

#[test]
fn paths_filter_recursive_glob_matches_docs_only() {
    let rule = make_test_rule_with_paths("test", "x", &["md"], &["docs/**/*.md"]);
    let compiled = compile_rule(rule).expect("rule must compile");
    assert!(rule_matches_path(&compiled, "docs/spec.md"));
    assert!(rule_matches_path(&compiled, "docs/adr/adr-001.md"));
    assert!(rule_matches_path(&compiled, "docs/a/b/c/deep.md"));
    assert!(!rule_matches_path(&compiled, "README.md"));
    assert!(!rule_matches_path(&compiled, "CLAUDE.md"));
}

#[test]
fn paths_filter_normalizes_windows_separators() {
    let rule = make_test_rule_with_paths("test", "x", &["md"], &["docs/**/*.md"]);
    let compiled = compile_rule(rule).expect("rule must compile");
    assert!(rule_matches_path(&compiled, r"docs\adr\adr-001.md"));
}

#[test]
fn paths_filter_multiple_globs_or_semantics() {
    let rule =
        make_test_rule_with_paths("test", "x", &["md"], &["docs/**/*.md", "tests/**/*.md"]);
    let compiled = compile_rule(rule).expect("rule must compile");
    assert!(rule_matches_path(&compiled, "docs/foo.md"));
    assert!(rule_matches_path(&compiled, "tests/integration.md"));
    assert!(!rule_matches_path(&compiled, "src/main.md"));
}

#[test]
fn paths_filter_invalid_glob_drops_rule() {
    let rule = make_test_rule_with_paths("test", "x", &["md"], &["docs/[unclosed"]);
    assert!(
        compile_rule(rule).is_none(),
        "invalid glob in paths should cause compile_rule to drop the rule"
    );
}

#[test]
fn run_custom_rules_extensions_and_paths_are_anded() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let docs_dir = dir.path().join("docs");
    std::fs::create_dir(&docs_dir).unwrap();
    let in_docs = docs_dir.join("foo.md");
    let mut f = std::fs::File::create(&in_docs).unwrap();
    f.write_all(b"FORBIDDEN\n").unwrap();

    let outside = dir.path().join("README.md");
    let mut f2 = std::fs::File::create(&outside).unwrap();
    f2.write_all(b"FORBIDDEN\n").unwrap();

    let rule = make_test_rule_with_paths("test", "FORBIDDEN", &["md"], &["**/docs/**/*.md"]);
    let compiled = compile_test_rules(vec![rule]);

    let in_docs_violations = run_custom_rules(in_docs.to_str().unwrap(), &compiled);
    let outside_violations = run_custom_rules(outside.to_str().unwrap(), &compiled);

    assert_eq!(in_docs_violations.len(), 1);
    assert!(outside_violations.is_empty());
}

#[test]
fn run_custom_rules_detects_console_log() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "const x = 1;").unwrap();
        writeln!(f, "console.log('debug');").unwrap();
        writeln!(f, "const y = 2;").unwrap();
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert_eq!(violations.len(), 1);
    let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
    assert_eq!(v["type"], "NO_CONSOLE_LOG");
    assert_eq!(v["severity"], "error");
    assert_eq!(v["location"]["line"], 2);
    assert_eq!(v["message"], "test message");
}

#[test]
fn run_custom_rules_no_violation_on_clean_file() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clean.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "const x = 1;").unwrap();
        writeln!(f, "logger.info('message');").unwrap();
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert!(violations.is_empty());
}

#[test]
fn run_custom_rules_skips_non_matching_extension() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.rs");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "console.log('should be ignored');").unwrap();
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert!(violations.is_empty());
}

#[test]
fn run_custom_rules_multiple_violations() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("multi.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "console.log('first');").unwrap();
        writeln!(f, "const x = 1;").unwrap();
        writeln!(f, "console.log('second');").unwrap();
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert_eq!(violations.len(), 2);
    let v1: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&violations[1]).unwrap();
    assert_eq!(v1["location"]["line"], 1);
    assert_eq!(v2["location"]["line"], 3);
}

#[test]
fn run_custom_rules_respects_max_violations() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("many.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        for i in 0..30 {
            writeln!(f, "console.log('line {}');", i).unwrap();
        }
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert_eq!(violations.len(), MAX_CUSTOM_VIOLATIONS);
}

#[test]
fn run_custom_rules_line_number_correct_with_multibyte_content() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("multibyte_fixture.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "// 日本語コメント").unwrap();
        writeln!(f, "// 🦀 rust").unwrap();
        writeln!(f, "console.log('after multibyte');").unwrap();
        writeln!(f, "// caf\u{00e9}").unwrap();
        writeln!(f, "console.log('second');").unwrap();
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert_eq!(violations.len(), 2);
    let v1: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&violations[1]).unwrap();
    assert_eq!(v1["location"]["line"], 3);
    assert_eq!(v2["location"]["line"], 5);
}

#[test]
fn run_custom_rules_outer_break_skips_subsequent_rules() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("outer_break.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        for i in 0..21 {
            writeln!(f, "console.log('cl {}');", i).unwrap();
        }
        for i in 0..5 {
            writeln!(f, "alert('al {}');", i).unwrap();
        }
    }

    let rules = compile_test_rules(vec![
        make_test_rule("rule-a", r"console\.log\(", &["ts"]),
        make_test_rule("rule-b", r"alert\(", &["ts"]),
    ]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert_eq!(violations.len(), MAX_CUSTOM_VIOLATIONS);
    for raw in &violations {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(v["type"], "RULE_A");
    }
}

#[test]
fn run_custom_rules_inner_cap_after_partial_first_rule() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("inner_cap.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        for i in 0..19 {
            writeln!(f, "console.log('cl {}');", i).unwrap();
        }
        for i in 0..5 {
            writeln!(f, "alert('al {}');", i).unwrap();
        }
    }

    let rules = compile_test_rules(vec![
        make_test_rule("rule-a", r"console\.log\(", &["ts"]),
        make_test_rule("rule-b", r"alert\(", &["ts"]),
    ]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);

    assert_eq!(violations.len(), MAX_CUSTOM_VIOLATIONS);
    let mut rule_a_count = 0;
    let mut rule_b_count = 0;
    for raw in &violations {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        match v["type"].as_str() {
            Some("RULE_A") => rule_a_count += 1,
            Some("RULE_B") => rule_b_count += 1,
            other => panic!("unexpected violation type: {other:?}"),
        }
    }
    assert_eq!(rule_a_count, 19);
    assert_eq!(rule_b_count, 1);
}

#[test]
fn compile_test_rules_filters_invalid_regex() {
    let rules = vec![
        make_test_rule("bad-rule", r"[invalid(", &["ts"]),
        make_test_rule("good-rule", r"console\.log\(", &["ts"]),
    ];
    let compiled = compile_test_rules(rules);

    assert_eq!(compiled.len(), 1);
    assert_eq!(compiled[0].rule.id, "good-rule");
}

#[test]
fn run_custom_rules_nonexistent_file() {
    let rules = compile_test_rules(vec![make_test_rule("test", r"pattern", &["ts"])]);
    let violations = run_custom_rules("/nonexistent/file.ts", &rules);
    assert!(violations.is_empty());
}

#[test]
fn violation_json_has_all_fields() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ts");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "console.log('x');").unwrap();
    }

    let rules = compile_test_rules(vec![make_test_rule(
        "no-console-log",
        r"console\.log\(",
        &["ts"],
    )]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();

    assert!(v.get("type").is_some());
    assert!(v.get("severity").is_some());
    assert!(v.get("location").is_some());
    assert!(v["location"].get("file").is_some());
    assert!(v["location"].get("line").is_some());
    assert!(v["location"].get("symbol").is_some());
    assert!(v.get("message").is_some());
    assert!(v.get("why").is_some());
    assert!(v.get("fix").is_some());
    assert!(v["fix"].get("strategy").is_some());
    assert!(v["fix"].get("steps").is_some());
    assert!(v.get("example").is_some());
    assert!(v["example"].get("bad").is_some());
    assert!(v["example"].get("good").is_some());
}

#[test]
fn parse_custom_rules_toml() {
    let toml_str = r#"
[[rules]]
id = "no-console-log"
pattern = 'console\.log\('
severity = "error"
message = "console.log は禁止"
why = "デバッグコード残留防止"
extensions = ["ts", "tsx"]

[rules.fix]
strategy = "削除 or logger置換"
steps = ["console.log行を削除する"]

[rules.example]
bad = "console.log('x');"
good = "logger.debug('x');"
"#;

    let config: CustomRulesConfig = toml::from_str(toml_str).unwrap();
    let rules = config.rules.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, "no-console-log");
    assert_eq!(rules[0].severity, "error");
    assert_eq!(rules[0].extensions, vec!["ts", "tsx"]);
    assert!(rules[0].fix.is_some());
    assert!(rules[0].example.is_some());
}

#[test]
fn parse_custom_rules_toml_minimal() {
    let toml_str = r#"
[[rules]]
id = "no-todo"
pattern = "TODO"
severity = "warning"
message = "TODO残留"
extensions = ["ts", "js"]
"#;

    let config: CustomRulesConfig = toml::from_str(toml_str).unwrap();
    let rules = config.rules.unwrap();
    assert_eq!(rules.len(), 1);
    assert!(rules[0].fix.is_none());
    assert!(rules[0].example.is_none());
    assert_eq!(rules[0].why, "");
}

fn ps_rule_with_pattern(id: &str, pattern: &str) -> CustomRule {
    make_test_rule(id, pattern, &["ps1"])
}

#[test]
fn powershell_validation_flags_rule_without_case_insensitive_flag() {
    let rules = vec![ps_rule_with_pattern("ps-bad", r"\bcatch\s*\{\s*\}")];
    let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
    assert_eq!(missing, vec!["ps-bad".to_string()]);
}

#[test]
fn powershell_validation_passes_rule_with_case_insensitive_flag() {
    let rules = vec![ps_rule_with_pattern("ps-good", r"(?i)\bcatch\s*\{\s*\}")];
    let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
    assert!(missing.is_empty());
}

#[test]
fn powershell_validation_ignores_non_ps1_rules() {
    let rule = make_test_rule("rs-rule", r"\bfn\s+main", &["rs"]);
    let missing = find_powershell_rules_missing_case_insensitive_flag(&[rule]);
    assert!(missing.is_empty());
}

#[test]
fn powershell_validation_handles_mixed_extension_list() {
    let rule = make_test_rule("mixed-rule", r"\bcatch\s*\{\s*\}", &["js", "ps1", "ts"]);
    let missing = find_powershell_rules_missing_case_insensitive_flag(&[rule]);
    assert_eq!(missing, vec!["mixed-rule".to_string()]);
}

#[test]
fn powershell_validation_treats_extension_case_insensitively() {
    let rule = make_test_rule("upper-ext", r"\bcatch\s*\{\s*\}", &["PS1"]);
    let missing = find_powershell_rules_missing_case_insensitive_flag(&[rule]);
    assert_eq!(missing, vec!["upper-ext".to_string()]);
}

#[test]
fn powershell_validation_returns_multiple_violators() {
    let rules = vec![
        ps_rule_with_pattern("ps-a", r"\bcatch"),
        ps_rule_with_pattern("ps-b", r"\berroraction"),
        ps_rule_with_pattern("ps-c-ok", r"(?i)\bwrite-host"),
    ];
    let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
    assert_eq!(missing, vec!["ps-a".to_string(), "ps-b".to_string()]);
}
