//! rule㉔ (gh-output-without-timeout) の positive / negative test。
//!
//! 同 crate の他 test module と同様、test helper は memory `feedback_test_dry_antipattern`
//! に従って per-module で複製している。本ファイルの違反例は文字列リテラルの中で `\"gh\"` と
//! エスケープされるため、本ファイル自身は検出されない (それ自体を
//! `gh_output_without_timeout_skips_escaped_string_literal` が固定している)。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRulesConfig};

const RULE: &str = "gh-output-without-timeout";

/// **pattern / exception を写経しない。** `config/custom-lint-rules.toml` から rule を丸ごと
/// 読む (ADR-081)。見つからなければ panic する (fail-closed)。
fn rule_from_repo_config(id: &str) -> CustomRule {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("config")
        .join("custom-lint-rules.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let config: CustomRulesConfig = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display()));
    config
        .rules
        .unwrap_or_default()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule '{id}' not found in {}", path.display()))
}

/// `content` を `.rs` ファイルとして本 rule だけで lint し、検出行を返す。
fn violation_lines(content: &str) -> Vec<u64> {
    let compiled: Vec<CompiledRule> = vec![rule_from_repo_config(RULE)]
        .into_iter()
        .filter_map(compile_rule)
        .collect();
    assert_eq!(compiled.len(), 1, "rule '{RULE}' must compile");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("input.rs");
    std::fs::write(&file, content).unwrap();
    run_custom_rules(file.to_str().unwrap(), &compiled)
        .iter()
        .map(|json| {
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            v["location"]["line"].as_u64().unwrap()
        })
        .collect()
}

#[test]
fn gh_output_without_timeout_detects_single_line_output() {
    let src = "let out = Command::new(\"gh\").args([\"pr\", \"view\"]).output()?;\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// PR #230 の対象 (cli-merge-pipeline の pr_metadata.rs) と同じ、builder を複数行に分けた形。
#[test]
fn gh_output_without_timeout_detects_multiline_builder() {
    let src = "fn fetch() -> Result<(), String> {\n\
               \x20   let output = Command::new(\"gh\")\n\
               \x20       .args([\"pr\", \"view\", &pr, \"--json\", \"commits\"])\n\
               \x20       .stdout(Stdio::piped())\n\
               \x20       .output()\n\
               \x20       .map_err(|e| format!(\"gh 起動失敗: {}\", e))?;\n\
               \x20   Ok(())\n\
               }\n";
    assert_eq!(violation_lines(src), vec![2]);
}

/// `.status()` も子プロセスの終了を無期限に待つ。
#[test]
fn gh_output_without_timeout_detects_status() {
    let src = "let st = Command::new(\"gh\").args([\"auth\", \"status\"]).status();\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// spawn した直後に同じ文で `wait_with_output()` するのは `.output()` と同じ。
#[test]
fn gh_output_without_timeout_detects_spawn_then_wait_with_output() {
    let src = "let out = Command::new(\"gh\").args(args).spawn()?.wait_with_output()?;\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// check-ci-coderabbit の `run_gh` と同じ形。spawn で文を終え、自前の timeout killer を付けて
/// から待つので bounded。文の区切り `;` で検出範囲が切れることを固定する。
#[test]
fn gh_output_without_timeout_skips_spawn_with_own_timeout() {
    let src = "fn run_gh(args: &[&str]) -> Result<String, String> {\n\
               \x20   let child = Command::new(\"gh\")\n\
               \x20       .args(args)\n\
               \x20       .stdout(Stdio::piped())\n\
               \x20       .spawn()\n\
               \x20       .map_err(|e| format!(\"gh の起動に失敗: {}\", e))?;\n\
               \x20   let (timeout_flag, done_flag) = spawn_timeout_killer(child.id());\n\
               \x20   let wait_result = child.wait_with_output();\n\
               \x20   todo!()\n\
               }\n";
    assert!(violation_lines(src).is_empty());
}

#[test]
fn gh_output_without_timeout_skips_run_cmd_direct_capture() {
    let src = "let cap = lib_subprocess::run_cmd_direct_capture(\"gh\", &[\"pr\", \"view\"], 120);\n\
               let cap = run_gh(&[\"api\", &endpoint]);\n";
    assert!(violation_lines(src).is_empty());
}

#[test]
fn gh_output_without_timeout_skips_comment_line() {
    let src = "// 旧実装: Command::new(\"gh\").args(args).output() は timeout なし\n\
               /// 例: `Command::new(\"gh\").output()` は使わない\n";
    assert!(violation_lines(src).is_empty());
}

/// 文字列リテラルの中の例は `\"gh\"` とエスケープされるので一致しない。本ファイルが自身の
/// 違反例で検出されないのはこの性質による。
#[test]
fn gh_output_without_timeout_skips_escaped_string_literal() {
    let src = "let bad = \"let o = Command::new(\\\"gh\\\").output()?;\";\n";
    assert!(violation_lines(src).is_empty());
}

#[test]
fn gh_output_without_timeout_skips_other_programs() {
    let src = "let out = Command::new(\"jj\").args([\"log\"]).output()?;\n\
               let out = Command::new(\"ghq\").arg(\"list\").output()?;\n";
    assert!(violation_lines(src).is_empty());
}
