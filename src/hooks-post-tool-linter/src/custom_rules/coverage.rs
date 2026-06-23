//! `rule_test_coverage_check` cargo test の実装。
//!
//! 順位 137 (PR #163 T1-#1 採用): `.claude/custom-lint-rules.toml` の各 rule に対して、
//! `[rules.test_coverage]` meta field で宣言された対応 test 関数が deploy 済 module 群に
//! 存在し、かつ必須カバレッジ (主要拡張子ごとに 1+ test、非主要専用 rule には
//! `other_ext_tests` 1+) が満たされていることを機械検証する。
//!
//! ## Module split を跨いだ test 関数検索
//!
//! PR-3a の file split (main.rs -> 複数 module) 以降、test 関数は `main.rs` だけでなく
//! 各 sub-module の `#[cfg(test)] mod tests` または `*_tests.rs` 内に散在する。
//! `extract_existing_test_fn_names` は `src/**/*.rs` を再帰的に走査して `fn xxx(` の
//! 全ての関数名を集める。

#[cfg(test)]
use super::types::{CustomRule, CustomRuleTestCoverage, CustomRulesConfig};

#[cfg(test)]
const MAIN_EXTENSIONS: &[&str] = &["rs", "toml", "yaml", "yml"];

#[cfg(test)]
fn load_deployed_custom_rules() -> Vec<CustomRule> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let toml_path = manifest_dir
        .join("..")
        .join("..")
        .join(".claude")
        .join("custom-lint-rules.toml");
    let toml_content = std::fs::read_to_string(&toml_path).unwrap_or_else(|e| {
        panic!(
            "failed to read deployed custom-lint-rules.toml at {}: {e} \
             (false-green guard: this test would silent-pass on missing file)",
            toml_path.display()
        )
    });
    let config: CustomRulesConfig =
        toml::from_str(&toml_content).expect("custom-lint-rules.toml must parse");
    let rules = config.rules.unwrap_or_default();
    assert!(
        !rules.is_empty(),
        "no rules found in deployed custom-lint-rules.toml — false-green guard"
    );
    rules
}

#[cfg(test)]
fn collect_rust_files_recursive(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name == "target" || file_name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_rust_files_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[cfg(test)]
fn extract_existing_test_fn_names() -> std::collections::HashSet<String> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let mut rust_files: Vec<std::path::PathBuf> = Vec::new();
    collect_rust_files_recursive(&src_dir, &mut rust_files);
    assert!(
        !rust_files.is_empty(),
        "false-green guard: no .rs files found under {}",
        src_dir.display()
    );

    let fn_regex = regex::Regex::new(r"(?m)\bfn\s+([a-zA-Z_][a-zA-Z_0-9]*)\s*\(").unwrap();
    let mut existing_fns: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in &rust_files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        for cap in fn_regex.captures_iter(&content) {
            existing_fns.insert(cap[1].to_string());
        }
    }
    assert!(
        existing_fns.contains("rule_test_coverage_check"),
        "false-green guard: fn_regex must find this test itself somewhere in src/. \
         existing_fns count = {}",
        existing_fns.len()
    );
    existing_fns
}

#[cfg(test)]
fn classify_rule_extensions(rule: &CustomRule) -> (Vec<&'static str>, bool) {
    let targets_main: Vec<&'static str> = MAIN_EXTENSIONS
        .iter()
        .filter(|m| rule.extensions.iter().any(|e| e.eq_ignore_ascii_case(m)))
        .copied()
        .collect();
    let has_non_main_ext = rule
        .extensions
        .iter()
        .any(|e| !MAIN_EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)));
    (targets_main, has_non_main_ext)
}

#[cfg(test)]
fn check_main_ext_coverage(
    rule: &CustomRule,
    coverage: &CustomRuleTestCoverage,
    targets_main: &[&str],
    existing_fns: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    for main_ext in targets_main {
        let tests = coverage.main_ext_tests.get(*main_ext);
        let is_empty = tests.map(|v| v.is_empty()).unwrap_or(true);
        if is_empty {
            gaps.push(format!(
                "rule `{}` targets main ext `{}` but `[rules.test_coverage.main_ext_tests].{}` is missing or empty (at least 1 positive test required)",
                rule.id, main_ext, main_ext
            ));
            continue;
        }
        for test_name in tests.unwrap() {
            if !existing_fns.contains(test_name) {
                gaps.push(format!(
                    "rule `{}` declares test `{}` for ext `{}` but no such function exists in src/",
                    rule.id, test_name, main_ext
                ));
            }
        }
    }
    gaps
}

#[cfg(test)]
fn check_other_ext_coverage(
    rule: &CustomRule,
    coverage: &CustomRuleTestCoverage,
    targets_main_empty: bool,
    has_non_main_ext: bool,
    existing_fns: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    if targets_main_empty && has_non_main_ext && coverage.other_ext_tests.is_empty() {
        gaps.push(format!(
            "rule `{}` targets only non-main extensions {:?} but `test_coverage.other_ext_tests` is empty (at least 1 positive test required)",
            rule.id, rule.extensions
        ));
    }
    for test_name in &coverage.other_ext_tests {
        if !existing_fns.contains(test_name) {
            gaps.push(format!(
                "rule `{}` declares other-ext test `{}` but no such function exists in src/",
                rule.id, test_name
            ));
        }
    }
    gaps
}

#[cfg(test)]
fn check_main_ext_keys_sanity(
    rule: &CustomRule,
    coverage: &CustomRuleTestCoverage,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    for declared_ext in coverage.main_ext_tests.keys() {
        if !MAIN_EXTENSIONS.contains(&declared_ext.as_str()) {
            gaps.push(format!(
                "rule `{}` declares `main_ext_tests.{}` but `{}` is not in MAIN_EXTENSIONS ({:?}) — use `other_ext_tests` for non-main extensions",
                rule.id, declared_ext, declared_ext, MAIN_EXTENSIONS
            ));
        }
        if !rule
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(declared_ext))
        {
            gaps.push(format!(
                "rule `{}` declares `main_ext_tests.{}` but `{}` is not in rule.extensions {:?}",
                rule.id, declared_ext, declared_ext, rule.extensions
            ));
        }
    }
    gaps
}

#[cfg(test)]
fn collect_rule_coverage_gaps(
    rule: &CustomRule,
    existing_fns: &std::collections::HashSet<String>,
) -> Vec<String> {
    let coverage = rule.test_coverage.clone().unwrap_or_default();
    let (targets_main, has_non_main_ext) = classify_rule_extensions(rule);
    let mut gaps = check_main_ext_coverage(rule, &coverage, &targets_main, existing_fns);
    gaps.extend(check_other_ext_coverage(
        rule,
        &coverage,
        targets_main.is_empty(),
        has_non_main_ext,
        existing_fns,
    ));
    gaps.extend(check_main_ext_keys_sanity(rule, &coverage));
    gaps
}

#[cfg(test)]
#[test]
fn rule_test_coverage_check() {
    let rules = load_deployed_custom_rules();
    let existing_fns = extract_existing_test_fn_names();
    let rules_with_declared_coverage =
        rules.iter().filter(|r| r.test_coverage.is_some()).count();
    let mut gaps: Vec<String> = Vec::new();
    for rule in &rules {
        gaps.extend(collect_rule_coverage_gaps(rule, &existing_fns));
    }
    assert_eq!(
        rules_with_declared_coverage,
        rules.len(),
        "rules without `[rules.test_coverage]` meta field: {} of {} rules missing — \
         add the meta field to every rule to seal test coverage contract (順位 137)",
        rules.len() - rules_with_declared_coverage,
        rules.len()
    );
    assert!(
        gaps.is_empty(),
        "rule test coverage gaps detected ({} issue(s)):\n  - {}",
        gaps.len(),
        gaps.join("\n  - ")
    );
}
