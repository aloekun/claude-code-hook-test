//! rule㉓ (read-dir-entry-error-dropped) の positive / negative test。
//!
//! 同 crate の他 test module と同様、test helper は memory `feedback_test_dry_antipattern`
//! に従って per-module で複製している。
//!
//! **入力は `concat!` で分割して書く。** 本ファイルは rule㉓ の `paths`
//! (`src/hooks-post-tool-linter/**`) の中にあるので、違反例をそのまま書くと自身が検出される。

use super::engine::{compile_rule, rule_matches_path, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRulesConfig};

const RULE_ID: &str = "read-dir-entry-error-dropped";

/// **pattern / exception / paths を写経しない。** `config/custom-lint-rules.toml` から rule を
/// 丸ごと読む (ADR-081)。見つからなければ panic する (fail-closed)。
fn rule_from_repo_config() -> CustomRule {
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
        .find(|r| r.id == RULE_ID)
        .unwrap_or_else(|| panic!("rule '{RULE_ID}' not found in {}", path.display()))
}

/// pattern / exception だけを検証する (temp file はリポジトリ外なので `paths` を外す)。
/// `paths` 自体は [`read_dir_drop_ignores_paths_outside_scope`] で検証する。
fn violation_lines(content: &str) -> Vec<u64> {
    let mut rule = rule_from_repo_config();
    rule.paths = None;
    let compiled: Vec<CompiledRule> = vec![rule].into_iter().filter_map(compile_rule).collect();
    assert_eq!(compiled.len(), 1, "rule must compile");
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
fn read_dir_drop_detects_chained_flatten() {
    let src = concat!("for e in std::fs::read_dir(p)?.flat", "ten() { use_it(e); }\n");
    assert_eq!(violation_lines(src), vec![1]);
}

#[test]
fn read_dir_drop_detects_entries_variable_flatten() {
    let src = concat!(
        "let Ok(entries) = std::fs::read_dir(p) else { return; };\n",
        "for entry in entries.flat", "ten() { use_it(entry); }\n",
    );
    assert_eq!(violation_lines(src), vec![2]);
}

/// run_registry の修正前と同じ、改行を挟んだ形。
#[test]
fn read_dir_drop_detects_multiline_entries_flatten() {
    let src = concat!(
        "let dirs: Vec<PathBuf> = entries\n",
        "    .flat", "ten()\n",
        "    .map(|e| e.path())\n",
        "    .collect();\n",
    );
    assert_eq!(violation_lines(src), vec![1]);
}

/// lib-telemetry/tests/reason_field.rs の修正前と同じ形。
#[test]
fn read_dir_drop_detects_filter_map_result_ok() {
    let src = concat!("let path = entries\n", "    .filter_map(Result::", "ok)\n", "    .next();\n");
    assert_eq!(violation_lines(src), vec![1]);
}

#[test]
fn read_dir_drop_detects_filter_map_closure_ok() {
    let src = concat!("let v: Vec<_> = std::fs::read_dir(p)?.filter_map(|e| e.", "ok()).collect();\n");
    assert_eq!(violation_lines(src), vec![1]);
}

/// 読めない entry をどちらへ倒すかを明示した書き方は検出しない。
#[test]
fn read_dir_drop_skips_explicit_handling() {
    let src = concat!(
        "for entry in entries {\n",
        "    let Ok(entry) = entry else { continue; };\n",
        "    use_it(entry);\n",
        "}\n",
        "for entry in entries { let path = entry.expect(\"entry\").path(); }\n",
    );
    assert!(violation_lines(src).is_empty());
}

/// cli-docs-lint の docs_files.rs にある「旧実装の説明」のようなコメントは検出しない。
#[test]
fn read_dir_drop_skips_comment_line() {
    let src = concat!("//! 旧実装は `read_dir(..).flat", "ten()` で entry エラーを捨てていた\n", "fn f() {}\n");
    assert!(violation_lines(src).is_empty());
}

/// `Option` の flatten など、read_dir と無関係な flatten は検出しない。
#[test]
fn read_dir_drop_skips_unrelated_flatten() {
    let src = concat!("let names: Vec<_> = maybe_names.into_iter().flat", "ten().collect();\n");
    assert!(violation_lines(src).is_empty());
}

/// 集計・補助の crate は対象外 (2026-10-02 ユーザー判断)。テストは crate を問わず対象。
#[test]
fn read_dir_drop_ignores_paths_outside_scope() {
    let compiled = compile_rule(rule_from_repo_config()).expect("rule must compile");
    assert!(rule_matches_path(&compiled, "src/cli-push-runner/src/stages/takt_verdict/mod.rs"));
    assert!(rule_matches_path(&compiled, "src/cli-merge-pipeline/src/feedback/run_registry.rs"));
    assert!(rule_matches_path(&compiled, "src/lib-telemetry/tests/reason_field.rs"));
    assert!(!rule_matches_path(&compiled, "src/cli-takt-timings/src/main.rs"));
    assert!(!rule_matches_path(&compiled, "src/cli-merge-pipeline/src/feedback/context.rs"));
}
