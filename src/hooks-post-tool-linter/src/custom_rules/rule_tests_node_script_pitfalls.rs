//! rule㉑ (gh-without-repo-in-node-script) / rule㉒ (network-spawn-without-timeout) の
//! positive / negative test。
//!
//! 同 crate の他 test module と同様、test helper は memory `feedback_test_dry_antipattern`
//! に従って per-module で複製している。両 rule は mjs / js / ts だけが対象なので、本ファイル
//! (rs) に違反例を書いても自身は検出されない。

use super::engine::{compile_rule, run_custom_rules};
use super::types::{CompiledRule, CustomRule, CustomRulesConfig};

const GH_REPO: &str = "gh-without-repo-in-node-script";
const SPAWN_TIMEOUT: &str = "network-spawn-without-timeout";

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

/// `content` を拡張子 `ext` のファイルとして `id` の rule だけで lint し、検出行を返す。
fn violation_lines(id: &str, ext: &str, content: &str) -> Vec<u64> {
    let compiled: Vec<CompiledRule> = vec![rule_from_repo_config(id)]
        .into_iter()
        .filter_map(compile_rule)
        .collect();
    assert_eq!(compiled.len(), 1, "rule '{id}' must compile");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(format!("input.{ext}"));
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
fn gh_without_repo_detects_spawn_sync_pr_view() {
    let src = "const r = spawnSync(\"gh\", [\"pr\", \"view\", pr, \"--json\", \"state\"], { timeout: T });\n";
    assert_eq!(violation_lines(GH_REPO, "mjs", src), vec![1]);
}

/// scripts/ledger-residue-scan.mjs と同じ複数行の形。
#[test]
fn gh_without_repo_detects_multiline_args() {
    let src = "const r = spawnSync(\n  \"gh\",\n  [\n    \"pr\",\n    \"list\",\n    \"--state\",\n    \"merged\",\n  ],\n  { timeout: T },\n);\n";
    assert_eq!(violation_lines(GH_REPO, "mjs", src), vec![2]);
}

/// scripts/rebase-nightly-pr.mjs の修正前と同じ、ラッパー経由の形。
#[test]
fn gh_without_repo_detects_wrapper_call() {
    let src = "const result = runOrFail(\"gh\", [\"pr\", \"view\", pr, \"--json\", \"headRefName\"]);\n";
    assert_eq!(violation_lines(GH_REPO, "mjs", src), vec![1]);
}

#[test]
fn gh_without_repo_skips_repo_flag() {
    let src = "const r = spawnSync(\"gh\", [\"pr\", \"view\", pr, \"--repo\", REPO], { timeout: T });\n";
    assert!(violation_lines(GH_REPO, "mjs", src).is_empty());
}

#[test]
fn gh_without_repo_skips_short_repo_flag() {
    let src = "const r = spawnSync(\"gh\", [\"issue\", \"list\", \"-R\", REPO], { timeout: T });\n";
    assert!(violation_lines(GH_REPO, "ts", src).is_empty());
}

/// `gh api` は URL にリポジトリを書くので対象外。
#[test]
fn gh_without_repo_skips_gh_api() {
    let src = "const r = spawnSync(\"gh\", [\"api\", `repos/${REPO}/pulls`], { timeout: T });\n";
    assert!(violation_lines(GH_REPO, "mjs", src).is_empty());
}

#[test]
fn gh_without_repo_skips_comment_line() {
    let src = "// 旧実装: runOrFail(\"gh\", [\"pr\", \"view\", pr]) は副 workspace で失敗した\nconst x = 1;\n";
    assert!(violation_lines(GH_REPO, "js", src).is_empty());
}

/// Rust の exe は GIT_DIR 注入で守られているので対象外 (旧順位 509 の検証)。
#[test]
fn gh_without_repo_ignores_rust_files() {
    let src = "let o = Command::new(\"gh\").args([\"pr\", \"view\", &n]).output();\nlet a = (\"gh\", [\"pr\", \"view\"]);\n";
    assert!(violation_lines(GH_REPO, "rs", src).is_empty());
}

#[test]
fn network_spawn_detects_gh_without_timeout() {
    let src = "const r = spawnSync(\"gh\", [\"pr\", \"list\", \"--repo\", REPO], { encoding: \"utf8\" });\n";
    assert_eq!(violation_lines(SPAWN_TIMEOUT, "mjs", src), vec![1]);
}

#[test]
fn network_spawn_detects_git_fetch_without_timeout() {
    let src = "spawnSync(\"git\", [\"fetch\", \"origin\"], { cwd: ROOT });\n";
    assert_eq!(violation_lines(SPAWN_TIMEOUT, "mjs", src), vec![1]);
}

#[test]
fn network_spawn_detects_git_dash_c_ls_remote() {
    let src = "spawnSync(\"git\", [\"-C\", dir, \"ls-remote\", \"origin\"], { cwd: ROOT });\n";
    assert_eq!(violation_lines(SPAWN_TIMEOUT, "mjs", src), vec![1]);
}

#[test]
fn network_spawn_detects_exec_file_sync() {
    let src = "const out = execFileSync(\"gh\", [\"api\", path], { encoding: \"utf8\" });\n";
    assert_eq!(violation_lines(SPAWN_TIMEOUT, "ts", src), vec![1]);
}

#[test]
fn network_spawn_detects_multiline_call() {
    let src = "const r = spawnSync(\n  \"gh\",\n  [\"pr\", \"list\"],\n  { cwd: ROOT, encoding: \"utf8\" },\n);\n";
    assert_eq!(violation_lines(SPAWN_TIMEOUT, "mjs", src), vec![1]);
}

/// jj も `git fetch` / `git push` では通信する (pre-push simplicity review の指摘)。
#[test]
fn network_spawn_detects_jj_git_fetch() {
    let src = "spawnSync(\"jj\", [\"git\", \"fetch\"], { cwd: ROOT });\n";
    assert_eq!(violation_lines(SPAWN_TIMEOUT, "mjs", src), vec![1]);
}

#[test]
fn network_spawn_skips_timeout_option() {
    let src = "const r = spawnSync(\n  \"gh\",\n  [\"pr\", \"list\"],\n  { cwd: ROOT, timeout: GH_TIMEOUT_MS },\n);\n";
    assert!(violation_lines(SPAWN_TIMEOUT, "mjs", src).is_empty());
}

/// cargo と、jj / git のローカル操作はネットワークを待たないので対象外。
#[test]
fn network_spawn_skips_local_commands() {
    let src = "spawnSync(\"cargo\", [\"check\"], {});\nspawnSync(\"jj\", [\"file\", \"list\"], {});\nspawnSync(\"git\", [\"status\"], {});\n";
    assert!(violation_lines(SPAWN_TIMEOUT, "mjs", src).is_empty());
}

#[test]
fn network_spawn_skips_comment_line() {
    let src = "// spawnSync(\"gh\", [\"pr\", \"list\"]) は timeout 無しだと止まる\nconst x = 1;\n";
    assert!(violation_lines(SPAWN_TIMEOUT, "mjs", src).is_empty());
}
