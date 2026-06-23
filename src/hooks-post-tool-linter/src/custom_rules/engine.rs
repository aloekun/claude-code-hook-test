//! Custom lint rule の compile / matching engine。
//!
//! `.claude/custom-lint-rules.toml` から `CustomRule` を読み込み、regex + paths glob を
//! pre-compile して `CompiledRule` を作る。`run_custom_rules` が file の content に対して
//! 全 rule を順次評価し、`LintViolation` の JSON 文字列の Vec を返す。

use super::types::{CompiledRule, CustomRule, CustomRulesConfig};
use crate::violation::{
    emit_feedback, LintViolation, ViolationExample, ViolationFix, ViolationLocation,
    MAX_CUSTOM_VIOLATIONS,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use std::path::{Path, PathBuf};

/// カスタムルール設定ファイルのパス解決
fn custom_rules_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("custom-lint-rules.toml")
}

/// `CustomRule::paths` を GlobSet に compile する。
///
/// - `None` または `Some(empty Vec)` -> `Ok(None)` (filter なし)
/// - `Some(non-empty)` で全 glob valid -> `Ok(Some(GlobSet))`
/// - 1 つでも glob が invalid -> `Err(error message)` (rule 全体を破棄)
pub(crate) fn compile_paths_glob(paths: &Option<Vec<String>>) -> Result<Option<GlobSet>, String> {
    let Some(pattern_list) = paths else {
        return Ok(None);
    };
    if pattern_list.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in pattern_list {
        let glob = Glob::new(pattern).map_err(|e| format!("invalid glob '{}': {}", pattern, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| format!("failed to build GlobSet: {}", e))
}

/// `CustomRule` 単体を compile し、`CompiledRule` を返す。失敗時は warn log + None。
pub(crate) fn compile_rule(rule: CustomRule) -> Option<CompiledRule> {
    let regex = match Regex::new(&rule.pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "[post-tool-linter] Warning: Invalid regex in rule '{}': {}",
                rule.id, e
            );
            return None;
        }
    };
    let paths_glob = match compile_paths_glob(&rule.paths) {
        Ok(g) => g,
        Err(msg) => {
            eprintln!(
                "[post-tool-linter] Warning: rule '{}' paths filter compile failed, dropping rule: {}",
                rule.id, msg
            );
            return None;
        }
    };
    Some(CompiledRule {
        rule,
        regex,
        paths_glob,
    })
}

/// カスタムルール設定を読み込み、正規表現をプリコンパイルする
pub(crate) fn load_custom_rules() -> Vec<CompiledRule> {
    let path = custom_rules_path();
    let rules = match std::fs::read_to_string(&path) {
        Ok(content) => {
            let config: CustomRulesConfig = toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "[post-tool-linter] Warning: Failed to parse {}: {}",
                    path.display(),
                    e
                );
                CustomRulesConfig::default()
            });
            config.rules.unwrap_or_default()
        }
        Err(_) => return Vec::new(),
    };

    for missing_id in find_powershell_rules_missing_case_insensitive_flag(&rules) {
        eprintln!(
            "[post-tool-linter] Warning: rule '{}' targets ps1 but lacks (?i) flag (PowerShell is case-insensitive — see ~/.claude/rules/common/code-review.md)",
            missing_id
        );
    }

    rules.into_iter().filter_map(compile_rule).collect()
}

pub(crate) fn find_powershell_rules_missing_case_insensitive_flag(
    rules: &[CustomRule],
) -> Vec<String> {
    rules
        .iter()
        .filter(|r| r.extensions.iter().any(|e| e.eq_ignore_ascii_case("ps1")))
        .filter(|r| !r.pattern.contains("(?i)"))
        .map(|r| r.id.clone())
        .collect()
}

/// ファイル拡張子がルールの対象かチェック
pub(crate) fn rule_matches_ext(rule: &CustomRule, file: &str) -> bool {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext {
        Some(ext) => rule.extensions.iter().any(|e| e.to_lowercase() == ext),
        None => false,
    }
}

/// `compiled.paths_glob` が `None` (filter なし) または `Some(GlobSet)` で file path がマッチする場合 true。
pub(crate) fn rule_matches_path(compiled: &CompiledRule, file: &str) -> bool {
    let Some(globset) = compiled.paths_glob.as_ref() else {
        return true;
    };
    let normalized = file.replace('\\', "/");
    globset.is_match(&normalized)
}

/// 1 件の regex match と rule 定義から `LintViolation` の JSON 文字列を構築する。
fn build_violation_json(
    file: &str,
    rule: &CustomRule,
    m: regex::Match,
    content: &str,
) -> Option<String> {
    let line_no = content[..m.start()].bytes().filter(|b| *b == b'\n').count() + 1;
    let violation = LintViolation {
        r#type: rule.id.to_uppercase().replace('-', "_"),
        severity: rule.severity.clone(),
        location: ViolationLocation {
            file: file.to_string(),
            line: line_no,
            symbol: m.as_str().to_string(),
        },
        message: rule.message.clone(),
        why: rule.why.clone(),
        fix: ViolationFix {
            strategy: rule
                .fix
                .as_ref()
                .map_or_else(String::new, |f| f.strategy.clone()),
            steps: rule.fix.as_ref().map_or_else(Vec::new, |f| f.steps.clone()),
        },
        example: ViolationExample {
            bad: rule
                .example
                .as_ref()
                .map_or_else(String::new, |e| e.bad.clone()),
            good: rule
                .example
                .as_ref()
                .map_or_else(String::new, |e| e.good.clone()),
        },
    };
    serde_json::to_string(&violation).ok()
}

fn collect_violations_for_rule(
    file: &str,
    content: &str,
    compiled: &CompiledRule,
    violations: &mut Vec<String>,
) {
    for m in compiled.regex.find_iter(content) {
        if violations.len() >= MAX_CUSTOM_VIOLATIONS {
            return;
        }
        if let Some(json) = build_violation_json(file, &compiled.rule, m, content) {
            violations.push(json);
        }
    }
}

pub(crate) fn run_custom_rules(file: &str, rules: &[CompiledRule]) -> Vec<String> {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut violations = Vec::new();

    for compiled in rules {
        if !rule_matches_ext(&compiled.rule, file) {
            continue;
        }
        if !rule_matches_path(compiled, file) {
            continue;
        }
        collect_violations_for_rule(file, &content, compiled, &mut violations);
        if violations.len() >= MAX_CUSTOM_VIOLATIONS {
            break;
        }
    }

    violations
}

/// PostToolUse custom-rules layer のエントリ。violation があれば feedback を emit する。
pub(crate) fn run_custom_rules_layer(file: &str) {
    let compiled_rules = load_custom_rules();
    let violations = run_custom_rules(file, &compiled_rules);
    if violations.is_empty() {
        return;
    }
    let feedback = format!(
        "[custom-lint] {} violation(s) found:\n{}",
        violations.len(),
        violations.join("\n")
    );
    emit_feedback(&feedback);
}
