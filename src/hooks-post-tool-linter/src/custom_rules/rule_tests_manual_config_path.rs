//! rule⑯ (no-manual-hooks-config-path) の positive / negative test (part 3)。
//!
//! `rule_tests_extras.rs` から分離した理由は行数のみ (ADR-080 の mechanical split)。
//! 同 crate の他 test module と同様、test helper (`make_test_rule` 等) は memory
//! `feedback_test_dry_antipattern` に従って per-module で複製している。

use super::engine::{compile_rule, rule_matches_path, run_custom_rules};
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
// ─── rule⑯: no-manual-hooks-config-path ───

fn no_manual_hooks_config_path_rule() -> CustomRule {
    let mut rule = make_test_rule(
        "no-manual-hooks-config-path",
        r#"current_(?:dir|exe)\(\)[^;{}]{0,400}?\.join\(\s*"[\w./-]*config\.toml"\s*\)"#,
        &["rs"],
    );
    rule.paths = Some(vec!["src/hooks-*/**/*.rs".to_string()]);
    rule
}

/// paths filter を外した rule。regex 単体の positive / negative は tempdir の相対パスで
/// 回すため (repo 配下でない tempdir は glob に一致しない)。filter 自体は
/// `no_manual_hooks_config_path_paths_match_hooks_crates_only` が固定する。
fn no_manual_hooks_config_path_rule_without_paths() -> CustomRule {
    let mut rule = no_manual_hooks_config_path_rule();
    rule.paths = None;
    rule
}

fn run_manual_hooks_config_path_rule_on(source: &str) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let file = write_file(dir.path(), "main.rs", source);
    let rules = compile_test_rules(vec![no_manual_hooks_config_path_rule_without_paths()]);
    run_custom_rules(file.to_str().unwrap(), &rules).len()
}

/// PR #267 incident と同型: `current_dir()` から `.join` を重ねて `hooks-config.toml` を組む。
#[test]
fn no_manual_hooks_config_path_detects_current_dir_join_chain() {
    let source = "fn config_path() -> std::path::PathBuf {\n    std::env::current_dir().unwrap_or_default().join(\".claude\").join(\"hooks-config.toml\")\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 1);
}

/// PR #267 の fix 直後の形 (`current_exe().parent()`) も、`lib_config_path` 導入後は
/// 同じ bug class の再発として検出対象になる (解決口の 1 本化、ADR-006 § 改訂 2026-09-15)。
#[test]
fn no_manual_hooks_config_path_detects_current_exe_join_chain() {
    let source = "fn config_path() -> std::path::PathBuf {\n    std::env::current_exe().unwrap().parent().unwrap().join(\"hooks-config.toml\").to_path_buf()\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 1);
}

/// `lib_config_path::resolve_config` は関数呼び出し構文 (`resolve_config(` に文字列を渡す) で
/// `.join(` を経由しないため、正しい経路は fire しない。
#[test]
fn no_manual_hooks_config_path_skips_resolve_config_call() {
    let source = "fn config_path() -> std::path::PathBuf {\n    lib_config_path::resolve_config(\"hooks-config.toml\")\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 0);
}

/// `current_dir()` / `current_exe()` を伴わない `.join(\"hooks-config.toml\")` は、
/// test fixture の staging 等の正当な用途であり fire してはならない
/// (`hooks-session-start/src/hooks_config.rs` の test module が実例)。
#[test]
fn no_manual_hooks_config_path_skips_join_without_current_dir_or_exe() {
    let source = "fn stage_fixture(dir: &std::path::Path) -> std::path::PathBuf {\n    dir.join(\".claude\").join(\"hooks-config.toml\")\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 0);
}

/// `current_exe()` が config 以外の用途 (`.session-id` 等) に使われている場合は fire しない
/// (`hooks-session-start/src/main.rs` の `session_id_file_path` が実例)。
#[test]
fn no_manual_hooks_config_path_skips_unrelated_current_exe_usage() {
    let source = "fn session_id_file_path() -> std::path::PathBuf {\n    std::env::current_exe().unwrap_or_default().parent().unwrap_or(std::path::Path::new(\".\")).join(\".session-id\")\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 0);
}

/// 起点取得 (`current_exe()`) と config 名の `.join(...)` が**別の関数**にある場合は fire しない。
/// 間隙を `[^;{}]` に絞ったことで関数境界 (`}` / `{`) と文の終わり (`;`) を跨げない
/// (PR #504 CodeRabbit 指摘の false positive — regex が 400 文字窓で無関係な 2 箇所を
/// ペアにしていた)。
#[test]
fn no_manual_hooks_config_path_skips_separate_functions() {
    let source = "fn exe_dir() -> std::path::PathBuf {\n    std::env::current_exe().unwrap().parent().unwrap().to_path_buf()\n}\n\nfn stage_fixture(dir: &std::path::Path) -> std::path::PathBuf {\n    dir.join(\"hooks-config.toml\")\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 0);
}

/// 相対パス 1 つを直接渡す単一 join 形 (`.claude/` 込みの config 名) も同じ bug class。パス文字クラスに
/// `/` を含めて取りこぼさない (PR #504 CodeRabbit 指摘の false negative)。
#[test]
fn no_manual_hooks_config_path_detects_single_join_with_slash_path() {
    let source = "fn config_path() -> std::path::PathBuf {\n    std::env::current_dir().unwrap_or_default().join(\".claude/hooks-config.toml\")\n}\n";
    assert_eq!(run_manual_hooks_config_path_rule_on(source), 1);
}

/// paths filter は `hooks-*` crate だけに当たる。`lib_config_path` 自身の実装
/// (`current_exe()` を正当に使う) や他 crate を巻き込まないため。
#[test]
fn no_manual_hooks_config_path_paths_match_hooks_crates_only() {
    let compiled = compile_rule(no_manual_hooks_config_path_rule()).expect("rule must compile");
    let cwd = std::env::current_dir().expect("cwd");
    let root = cwd.to_string_lossy().replace('\\', "/");
    let abs = |rel: &str| format!("{root}/{rel}");
    for rel in [
        "src/hooks-session-start/src/main.rs",
        "src/hooks-post-tool-jj-op-verify/src/main.rs",
    ] {
        assert!(rule_matches_path(&compiled, &abs(rel)), "should match: {rel}");
    }
    for rel in [
        "src/lib-config-path/src/lib.rs",
        "src/cli-pr-monitor/src/main.rs",
    ] {
        assert!(!rule_matches_path(&compiled, &abs(rel)), "should not match: {rel}");
    }
}
