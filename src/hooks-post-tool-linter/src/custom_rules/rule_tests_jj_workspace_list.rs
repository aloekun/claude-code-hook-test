//! rule⑱ (jj-workspace-list-without-ignore-working-copy) の positive / negative test。
//!
//! 同 crate の他 test module と同様、test helper は memory `feedback_test_dry_antipattern`
//! に従って per-module で複製している。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRulesConfig};

const RULE_ID: &str = "jj-workspace-list-without-ignore-working-copy";

/// **pattern / exception を写経しない。** リポジトリの `config/custom-lint-rules.toml` から
/// rule を丸ごと読む。テスト側にコピーを置くと、config だけを直したときに古い定義を
/// 検証し続けて green のままになる (ADR-081)。見つからなければ panic する (fail-closed)。
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

fn compiled_rule() -> Vec<CompiledRule> {
    let compiled: Vec<CompiledRule> = vec![rule_from_repo_config(RULE_ID)]
        .into_iter()
        .filter_map(compile_rule)
        .collect();
    assert_eq!(compiled.len(), 1, "rule '{RULE_ID}' must compile");
    compiled
}

/// `content` を `.rs` として lint し、検出行 (1-indexed) の list を返す。
fn violation_lines(content: &str) -> Vec<u64> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("jj_call.rs");
    std::fs::write(&file, content).unwrap();
    run_custom_rules(file.to_str().unwrap(), &compiled_rule())
        .iter()
        .map(|json| {
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            v["location"]["line"].as_u64().unwrap()
        })
        .collect()
}

#[test]
fn jj_workspace_list_detects_args_array_without_flag() {
    let src = "let out = Command::new(\"jj\").args([\"workspace\", \"list\", \"-T\", template]).output();\n";
    assert_eq!(violation_lines(src), vec![1]);
}

#[test]
fn jj_workspace_list_detects_multiline_args_array_without_flag() {
    let src = "let out = std::process::Command::new(\"jj\")\n    .args([\n        \"workspace\",\n        \"list\",\n        \"-T\",\n        \"self.root()\",\n    ])\n    .output();\n";
    assert_eq!(violation_lines(src), vec![2]);
}

#[test]
fn jj_workspace_list_detects_arg_chain_without_flag() {
    let src = "let out = Command::new(\"jj\").arg(\"workspace\").arg(\"list\").arg(\"-T\").output();\n";
    assert_eq!(violation_lines(src), vec![1]);
}

/// exception は match 範囲だけを見る。同じファイルに準拠した呼び出しがあっても、
/// 違反側の呼び出しは逃がさない。
#[test]
fn jj_workspace_list_detects_violation_next_to_compliant_call() {
    let src = "let a = Command::new(\"jj\").args([\"workspace\", \"list\", \"--ignore-working-copy\"]).output();\nlet b = Command::new(\"jj\").args([\"workspace\", \"list\"]).output();\n";
    assert_eq!(violation_lines(src), vec![2]);
}

#[test]
fn jj_workspace_list_skips_flag_after_subcommand() {
    let src = "let out = Command::new(\"jj\")\n    .args([\"workspace\", \"list\", \"--ignore-working-copy\", \"-T\", template])\n    .output();\n";
    assert!(violation_lines(src).is_empty());
}

/// global option なので subcommand の前に置いても有効。範囲が配列全体なので逃がす。
#[test]
fn jj_workspace_list_skips_flag_before_subcommand() {
    let src = "let out = Command::new(\"jj\")\n    .args([\"--ignore-working-copy\", \"workspace\", \"list\", \"-T\", template])\n    .output();\n";
    assert!(violation_lines(src).is_empty());
}

#[test]
fn jj_workspace_list_skips_flag_in_arg_chain() {
    let src = "let out = Command::new(\"jj\").arg(\"workspace\").arg(\"list\").arg(\"--ignore-working-copy\").output();\n";
    assert!(violation_lines(src).is_empty());
}

/// 連鎖形でも global option を subcommand の前に置ける。連鎖の範囲は先頭の `.arg` から
/// 始まるので、前置したフラグも範囲に入る。
#[test]
fn jj_workspace_list_skips_flag_before_subcommand_in_arg_chain() {
    let src = "let out = Command::new(\"jj\").arg(\"--ignore-working-copy\").arg(\"workspace\").arg(\"list\").output();\n";
    assert!(violation_lines(src).is_empty());
}

/// `workspace add` / `update-stale` は working copy を触る操作で、本ルールの対象外。
#[test]
fn jj_workspace_list_skips_other_workspace_subcommands() {
    let src = "let a = Command::new(\"jj\").args([\"workspace\", \"add\", path]).status();\nlet b = Command::new(\"jj\").args([\"workspace\", \"update-stale\"]).status();\n";
    assert!(violation_lines(src).is_empty());
}

/// deploy 済み rule が exception を持つこと。exception が消えると、準拠した呼び出し
/// まで全件 error になる (逆に pattern 側を緩めて逃がす改変を防ぐ目的もある)。
#[test]
fn jj_workspace_list_deployed_rule_has_exception() {
    let rule = rule_from_repo_config(RULE_ID);
    assert_eq!(rule.exception.as_deref(), Some(r#""--ignore-working-copy""#));
}
