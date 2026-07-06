//! Deployed rule (rules①〜) ごとの positive / negative test (part 1)。
//!
//! 命名規約: `<rule_id_short>_<scenario>` (例: `no_personal_paths_detects_windows_user_path_in_md`)。
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

fn no_personal_paths_rule() -> CustomRule {
    make_test_rule(
        "no-personal-paths",
        r"C:\\Users\\[A-Za-z][A-Za-z0-9_-]+\\|/home/[a-z][a-z0-9_-]+/",
        &["md", "txt"],
    )
}

#[test]
fn no_personal_paths_detects_windows_user_path_in_md() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "guide.md",
        "Path: `C:\\Users\\alice\\.claude\\projects\\foo` is the location\n",
    );
    let rules = compile_test_rules(vec![no_personal_paths_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_personal_paths_detects_unix_home_path_in_txt() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "notes.txt",
        "Run from /home/bob/projects/foo to start\n",
    );
    let rules = compile_test_rules(vec![no_personal_paths_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn no_personal_paths_skips_placeholder_paths() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "doc.md",
        "Use `%USERPROFILE%\\.claude\\` or `<USER_HOME>/.claude/` or `~/.claude/` paths\n",
    );
    let rules = compile_test_rules(vec![no_personal_paths_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

fn ps_empty_catch_rule() -> CustomRule {
    make_test_rule("no-empty-powershell-catch", r"(?i)catch\s*\{\s*\}", &["ps1"])
}

#[test]
fn ps_empty_catch_detects_violation() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "swallow.ps1", "try { Get-Item $p } catch {}\n");
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn ps_empty_catch_detects_with_internal_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "ws.ps1", "try { ... } catch {  }\n");
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn ps_empty_catch_skips_non_empty_block() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "ok.ps1",
        "try { ... } catch { Write-Error $_ }\n",
    );
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn ps_empty_catch_only_targets_ps1() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "elsewhere.ts", "try { x() } catch {}\n");
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn ps_empty_catch_detects_capitalized_keyword() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "cap.ps1", "try { Get-Item $p } Catch {}\n");
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn ps_empty_catch_detects_uppercase_keyword() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "upper.ps1", "try { Get-Item $p } CATCH {}\n");
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn ps_empty_catch_detects_multiline_block() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "multi.ps1",
        "try {\n    Get-Item $p\n} catch {\n}\n",
    );
    let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
    let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
    assert_eq!(v["location"]["line"], 3);
}

fn ps_silent_error_rule() -> CustomRule {
    make_test_rule(
        "no-silent-error-action",
        r"(?i)-ErrorAction\s+SilentlyContinue",
        &["ps1"],
    )
}

#[test]
fn ps_silent_error_detects_basic_form() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "silent.ps1",
        "$d = ConvertFrom-Json $r -ErrorAction SilentlyContinue\n",
    );
    let rules = compile_test_rules(vec![ps_silent_error_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn ps_silent_error_skips_stop_action() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "stop.ps1",
        "ConvertFrom-Json $r -ErrorAction Stop\n",
    );
    let rules = compile_test_rules(vec![ps_silent_error_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn ps_silent_error_skips_ignore_action() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "ignore.ps1",
        "Get-Item $p -ErrorAction Ignore\n",
    );
    let rules = compile_test_rules(vec![ps_silent_error_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn ps_silent_error_detects_lowercase_param() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "lc.ps1",
        "Get-Item $p -erroraction silentlycontinue\n",
    );
    let rules = compile_test_rules(vec![ps_silent_error_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn ps_silent_error_detects_mixed_case() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "mixed.ps1",
        "ConvertFrom-Json $r -ErrorAction SILENTLYCONTINUE\n",
    );
    let rules = compile_test_rules(vec![ps_silent_error_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

fn md_mutable_anchor_rule() -> CustomRule {
    make_test_rule("no-mutable-anchor", r"\]\([^)#:]*#[^\x00-\x7F)]+", &["md"])
}

#[test]
fn md_mutable_anchor_detects_inline_fragment() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "frag.md", "See [section](#推奨実行順序)\n");
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn md_mutable_anchor_detects_path_with_fragment() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "cross.md",
        "See [other](other.md#日本語見出し)\n",
    );
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn md_mutable_anchor_skips_ascii_fragment() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "ascii.md", "See [section](#stable-ascii-id)\n");
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_mutable_anchor_skips_link_without_fragment() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "url.md",
        "Visit [example](https://example.com)\n",
    );
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_mutable_anchor_skips_path_only_link() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "path.md", "See [other](other.md)\n");
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_mutable_anchor_only_targets_md() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "other.txt", "See [section](#日本語)\n");
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_mutable_anchor_skips_external_url_with_fragment() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "external.md",
        "See [spec](https://example.com/#日本語)\n",
    );
    let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

fn rs_time_field_strict_greater_rule() -> CustomRule {
    make_test_rule(
        "no-time-field-strict-greater",
        r"\b(created_at|submitted_at|updated_at|comment_event_time|event_time|comment_created_at|published_at|posted_at|commented_at)\s*>\s*[a-zA-Z_]",
        &["rs"],
    )
}

fn build_rs_source_with_op(field_lhs: &str, op: &str, rhs: &str) -> String {
    format!("fn f() {{ items.iter().filter(|c| c.{field_lhs} {op} {rhs}); }}\n")
}

fn build_doc_comment_source(field_lhs: &str, op: &str, rhs: &str) -> String {
    format!("/// `{field_lhs} {op} {rhs}` (epoch 0 で実質全件)\nfn f() {{}}\n")
}

fn build_toml_with_field(field_lhs: &str, op: &str, rhs: &str) -> String {
    format!("comment = \"{field_lhs} {op} {rhs}\"\n")
}

#[test]
fn rs_time_field_strict_greater_detects_created_at_gt_push_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("created_at", ">", "push_time"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn rs_time_field_strict_greater_detects_submitted_at_gt_since() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("submitted_at", ">", "since"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn rs_time_field_strict_greater_detects_updated_at_gt_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("updated_at", ">", "threshold"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn rs_time_field_strict_greater_detects_comment_event_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("comment_event_time", ">", "now"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn rs_time_field_strict_greater_skips_inclusive_comparison() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("created_at", ">=", "push_time"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn rs_time_field_strict_greater_skips_strict_less_than() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "stale.rs",
        &build_rs_source_with_op("created_at", "<", "threshold"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn rs_time_field_strict_greater_skips_le_inclusive() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("created_at", "<=", "cutoff"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn rs_time_field_strict_greater_skips_numeric_rhs() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("created_at", ">", "0"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn rs_time_field_strict_greater_skips_doc_comment_with_inclusive() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "doc.rs",
        &build_doc_comment_source("created_at", ">=", "push_time"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn rs_time_field_strict_greater_skips_unrelated_field() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "parse.rs",
        &build_rs_source_with_op("count", ">", "limit"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn rs_time_field_strict_greater_only_targets_rs() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "config.toml",
        &build_toml_with_field("created_at", ">", "push_time"),
    );
    let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

fn md_no_docs_relative_back_to_docs_rule() -> CustomRule {
    make_test_rule("no-docs-relative-back-to-docs", r"(?i)\]\(\.\./docs/", &["md"])
}

#[test]
fn md_no_docs_relative_detects_pr133_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "todo7.md",
        "See [ADR-036](../docs/adr/adr-036-bundle-z-three-layer-review.md) for details.\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn md_no_docs_relative_detects_uppercase_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "note.md",
        "Reference [Spec](../DOCS/feature.md).\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn md_no_docs_relative_skips_same_directory_link() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "todo7.md",
        "See [ADR-036](adr/adr-036-bundle-z-three-layer-review.md) for details.\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_no_docs_relative_skips_parent_to_other_dir() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "page.md",
        "See [README](../README.md) and [src](../src/main.rs).\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_no_docs_relative_only_targets_md() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "config.toml",
        "doc = \"](../docs/adr/foo.md)\"\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert!(violations.is_empty());
}

#[test]
fn md_no_docs_relative_detects_root_level_back_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "CLAUDE.md",
        "See [TODO summary](../docs/todo-summary.md) for context.\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}

#[test]
fn md_no_docs_relative_detects_root_readme_back_reference() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(
        dir.path(),
        "README.md",
        "Project setup guide: [setup](../docs/setup.md)\n",
    );
    let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
    let violations = run_custom_rules(file.to_str().unwrap(), &rules);
    assert_eq!(violations.len(), 1);
}
