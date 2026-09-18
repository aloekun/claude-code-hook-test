//! rule⑰ (no-pid-timestamp-temp-naming) の positive / negative test (part 4)。
//!
//! `rule_tests_extras.rs` から独立させた理由は行数のみ (ADR-080 の mechanical split)。
//! 同 crate の他 test module と同様、test helper (`make_test_rule` 等) は memory
//! `feedback_test_dry_antipattern` に従って per-module で複製している。

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

// ─── rule⑰: no-pid-timestamp-temp-naming ───

fn no_pid_timestamp_temp_naming_rule() -> CustomRule {
    make_test_rule(
        "no-pid-timestamp-temp-naming",
        r#"temp_dir\(\)\s*\.join\(\s*format!\([^;]{0,400}?process::id\(\)[^;]{0,400}?as_(?:millis|nanos)\(\)"#,
        &["rs"],
    )
}

fn run_pid_timestamp_temp_naming_rule_on(source: &str) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "snapshot.rs", source);
    let rules = compile_test_rules(vec![no_pid_timestamp_temp_naming_rule()]);
    run_custom_rules(file.to_str().unwrap(), &rules).len()
}

/// PR #229 incident と同型: PID + `as_nanos()` の併用。
#[test]
fn no_pid_timestamp_temp_naming_detects_pid_and_nanos_combo() {
    let source = "let path = std::env::temp_dir().join(format!(\"nightly-outcome-e2e-{}-{}.json\", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));\n";
    assert_eq!(run_pid_timestamp_temp_naming_rule_on(source), 1);
}

/// `as_millis()` variant も同じ bug class として検出する。
#[test]
fn no_pid_timestamp_temp_naming_detects_pid_and_millis_combo() {
    let source = "let path = std::env::temp_dir().join(format!(\"push-runner-{}-{}.json\", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));\n";
    assert_eq!(run_pid_timestamp_temp_naming_rule_on(source), 1);
}

/// PID 単独 (rule⑮ の `[rules.fix]` が推奨する形そのもの) は対象外。`src/` に
/// `process::id()` は 64 箇所あり、ここを検出対象に含めると rule⑮ が推奨して直した箇所を
/// 後から違反にしてしまう。
#[test]
fn no_pid_timestamp_temp_naming_skips_pid_only() {
    let source =
        "let path = std::env::temp_dir().join(format!(\"push-runner-snapshot-{}.json\", std::process::id()));\n";
    assert_eq!(run_pid_timestamp_temp_naming_rule_on(source), 0);
}

/// `tempfile::NamedTempFile` 等への切替後は `temp_dir().join(format!(...))` 自体が
/// 出現しないため fire しない (fix 後の正常形)。
#[test]
fn no_pid_timestamp_temp_naming_skips_tempfile_usage() {
    let source = "let temp_file = tempfile::NamedTempFile::new().expect(\"tempfile\");\nlet path = temp_file.path();\n";
    assert_eq!(run_pid_timestamp_temp_naming_rule_on(source), 0);
}

/// `temp_dir()` を経由しない PID+時刻併用の `format!` (temp file 用途でない一意 ID 生成等) は
/// 本 rule のスコープ外。
#[test]
fn no_pid_timestamp_temp_naming_skips_non_temp_dir_join() {
    let source = "let id = format!(\"{}-{}\", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));\n";
    assert_eq!(run_pid_timestamp_temp_naming_rule_on(source), 0);
}

#[test]
fn no_pid_timestamp_temp_naming_only_targets_rust_extension() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "notes.md",
        "let path = std::env::temp_dir().join(format!(\"nightly-outcome-e2e-{}-{}.json\", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));\n",
    );
    let rules = compile_test_rules(vec![no_pid_timestamp_temp_naming_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}
