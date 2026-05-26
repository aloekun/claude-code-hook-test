//! Scratch file warning stage — 順位 1 (PR #85 T1-4)
//!
//! `@` commit に scratch-pattern ファイル (default pattern: `__*`) が含まれていないか
//! 検査し、検出時は warning + block で push を停止する。jj は auto-snapshot で
//! working tree を即 commit に取り込むため、`.gitignore` 漏れがあると scratch
//! ファイルが PR に意図せず混入する (PR #85 で `__parse_transcripts.ps1` 実例)。
//!
//! ADR-039 (Experimental feature 標準パターン) 準拠の 3 点セット:
//! - **Config opt-in**: 試験運用のため default `enabled = false`、`[scratch_file_warning]`
//!   section で明示的に `enabled = true` にしないと検査は走らない。section 不在 /
//!   enabled 未指定の場合も skip (= 完全 no-op)。
//! - **Kill-switch**: `enabled = false` (TOML) または env override
//!   `SCRATCH_FILE_WARNING_OVERRIDE=1` で意図的バイパス可能。
//! - **Bounded lifetime**: 3-5 PR の dogfood で false positive / 検出効果を観測後、
//!   default-ON 昇格 or 却下を判定 (詳細は push-runner-config.toml の
//!   `[scratch_file_warning]` section コメント参照)。
//!
//! Stage 配置: `run_pipeline` の最早期 (quality_gate より前)。検出時は quality_gate
//! や takt review を無駄に走らせず即停止する。
//!
//! Config-driven pattern: `[scratch_file_warning]` section で `patterns` を拡張可能。
//! 順位 5 (AI 生成一時スクリプト pattern の pre-push 検出) は本 stage の patterns
//! 拡張 (例: `_tmp_*`) + ADR-007 連携で補完的に実装する。

use std::process::Command;

use crate::config::ScratchFileWarningConfig;
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;
const OVERRIDE_ENV_VAR: &str = "SCRATCH_FILE_WARNING_OVERRIDE";
const DEFAULT_PATTERN: &str = "__*";

/// `[scratch_file_warning]` config の有無に応じて検査を実行し、
/// push を続行してよいか (= violation なし or override active) を返す。
///
/// ADR-039 § 1 Config opt-in 準拠: default `enabled = false` (試験運用)。
/// section 不在 / `c.enabled = None` / `c.enabled = Some(false)` のいずれも skip。
/// 明示的に `c.enabled = Some(true)` のときのみ検査を実行。
///
/// fail-open: jj 不調 (timeout / 起動失敗) 時は warning ログのみで true を返し、
/// push 自体は止めない。
pub(crate) fn run_scratch_file_warning(config: Option<&ScratchFileWarningConfig>) -> bool {
    let enabled = config.and_then(|c| c.enabled).unwrap_or(false);
    if !enabled {
        return true;
    }
    let patterns = effective_patterns(config);
    let files = match list_files_in_at() {
        Ok(f) => f,
        Err(e) => {
            log_info(&format!(
                "scratch_file_warning: jj file list 失敗、検査を skip して push を続行します: {}",
                e
            ));
            return true;
        }
    };
    let violations = find_violations(&files, &patterns);
    if violations.is_empty() {
        log_stage("scratch", "scratch ファイル検出なし");
        return true;
    }
    log_stage(
        "scratch",
        &format!(
            "scratch ファイル候補 ({} 件) が @ commit に含まれます:",
            violations.len()
        ),
    );
    for v in &violations {
        log_info(&format!("  - {}", v));
    }
    let raw = std::env::var(OVERRIDE_ENV_VAR).ok();
    if parse_override_env(raw.as_deref()) {
        log_info(&format!(
            "  {}={} により続行します (意図的バイパス)",
            OVERRIDE_ENV_VAR,
            raw.as_deref().unwrap_or("")
        ));
        true
    } else {
        log_info(&format!(
            "  対処:\n  \
             (a) `.gitignore` に該当 pattern を追加 + `jj abandon @ && jj new` で再記述\n  \
             (b) ファイル自体を削除\n  \
             (c) 意図的 commit なら env {}=1 を設定して再実行",
            OVERRIDE_ENV_VAR
        ));
        false
    }
}

fn effective_patterns(config: Option<&ScratchFileWarningConfig>) -> Vec<String> {
    config
        .and_then(|c| c.patterns.as_ref())
        .map(|patterns| {
            patterns
                .iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|patterns| !patterns.is_empty())
        .unwrap_or_else(|| vec![DEFAULT_PATTERN.to_string()])
}

fn list_files_in_at() -> Result<Vec<String>, String> {
    let output = run_jj_file_list_at()?;
    Ok(parse_file_list_output(&output))
}

fn parse_file_list_output(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|line| line.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn extract_basename(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(idx) => &path[idx + 1..],
        None => path,
    }
}

/// 簡易 glob: `*` (任意長文字列、空マッチ含む) のみサポート。`?` 等は未対応。
/// パターンに `*` が含まれない場合は完全一致。
fn matches_glob(name: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return name == pattern;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    match_pattern_parts(name, &parts)
}

fn match_pattern_parts(name: &str, parts: &[&str]) -> bool {
    let Some(after_prefix) = consume_prefix(name, parts.first().copied().unwrap_or("")) else {
        return false;
    };
    let middle_parts = pattern_middle_slice(parts);
    let Some(after_middle) = consume_middle(after_prefix, middle_parts) else {
        return false;
    };
    if parts.len() > 1 {
        let suffix = parts.last().copied().unwrap_or("");
        check_suffix(after_middle, suffix)
    } else {
        true
    }
}

fn pattern_middle_slice<'a>(parts: &'a [&'a str]) -> &'a [&'a str] {
    if parts.len() > 2 {
        &parts[1..parts.len() - 1]
    } else {
        &[]
    }
}

fn consume_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        Some(name)
    } else if name.starts_with(prefix) {
        Some(&name[prefix.len()..])
    } else {
        None
    }
}

fn consume_middle<'a>(name: &'a str, middle_parts: &[&str]) -> Option<&'a str> {
    let mut remaining = name;
    for part in middle_parts {
        if part.is_empty() {
            continue;
        }
        let idx = remaining.find(part)?;
        remaining = &remaining[idx + part.len()..];
    }
    Some(remaining)
}

fn check_suffix(name: &str, suffix: &str) -> bool {
    suffix.is_empty() || name.ends_with(suffix)
}

fn find_violations(files: &[String], patterns: &[String]) -> Vec<String> {
    let mut violations = Vec::new();
    for file in files {
        let name = extract_basename(file);
        for pattern in patterns {
            if matches_glob(name, pattern) {
                violations.push(file.clone());
                break;
            }
        }
    }
    violations
}

fn parse_override_env(raw: Option<&str>) -> bool {
    let Some(value) = raw else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn run_jj_file_list_at() -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(["file", "list", "-r", "@"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj file list 起動失敗: {}", e))?;

    let stdout_handle =
        crate::runner::drain_pipe(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        crate::runner::drain_pipe(child.stderr.take().expect("stderr must be piped"));

    let status = crate::runner::wait_with_timeout("jj file list", &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("jj file list wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj file list タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_list_basic() {
        let raw = "src/main.rs\nsrc/lib.rs\n";
        assert_eq!(
            parse_file_list_output(raw),
            vec!["src/main.rs", "src/lib.rs"]
        );
    }

    #[test]
    fn parse_file_list_skips_empty_lines() {
        let raw = "src/main.rs\n\n\nsrc/lib.rs\n";
        assert_eq!(
            parse_file_list_output(raw),
            vec!["src/main.rs", "src/lib.rs"]
        );
    }

    #[test]
    fn parse_file_list_trims_whitespace() {
        let raw = "  src/main.rs  \n\tsrc/lib.rs\t\n";
        assert_eq!(
            parse_file_list_output(raw),
            vec!["src/main.rs", "src/lib.rs"]
        );
    }

    #[test]
    fn parse_file_list_empty_returns_empty() {
        assert_eq!(parse_file_list_output(""), Vec::<String>::new());
    }

    #[test]
    fn extract_basename_forward_slash() {
        assert_eq!(extract_basename("src/foo/bar.rs"), "bar.rs");
    }

    #[test]
    fn extract_basename_backslash() {
        assert_eq!(extract_basename(r"src\foo\bar.rs"), "bar.rs");
    }

    #[test]
    fn extract_basename_no_separator() {
        assert_eq!(extract_basename("foo.rs"), "foo.rs");
    }

    #[test]
    fn extract_basename_mixed_separators() {
        assert_eq!(extract_basename(r"src/foo\bar.rs"), "bar.rs");
        assert_eq!(extract_basename(r"src\foo/bar.rs"), "bar.rs");
    }

    #[test]
    fn extract_basename_trailing_separator_returns_empty() {
        assert_eq!(extract_basename("src/foo/"), "");
    }

    #[test]
    fn matches_glob_prefix_wildcard() {
        assert!(matches_glob("__foo", "__*"));
        assert!(matches_glob("__", "__*"));
        assert!(!matches_glob("foo__", "__*"));
        assert!(!matches_glob("_foo", "__*"));
    }

    #[test]
    fn matches_glob_suffix_wildcard() {
        assert!(matches_glob("foo.tmp", "*.tmp"));
        assert!(matches_glob(".tmp", "*.tmp"));
        assert!(!matches_glob("foo.tmpx", "*.tmp"));
    }

    #[test]
    fn matches_glob_prefix_and_suffix_wildcards() {
        assert!(matches_glob("_tmp_file.ps1", "_tmp_*"));
        assert!(matches_glob("__file.py", "__*.py"));
        assert!(!matches_glob("__file.ps1", "__*.py"));
    }

    #[test]
    fn matches_glob_single_middle_wildcard() {
        assert!(matches_glob("foobazbar", "foo*bar"));
        assert!(matches_glob("foobar", "foo*bar"));
        assert!(!matches_glob("fooXY", "foo*bar"));
    }

    #[test]
    fn matches_glob_three_part_pattern() {
        assert!(matches_glob("mytest_x.ps1", "*test*.ps1"));
        assert!(matches_glob("test.ps1", "*test*.ps1"));
        assert!(!matches_glob("foo.ps1", "*test*.ps1"));
    }

    #[test]
    fn matches_glob_no_wildcard_exact() {
        assert!(matches_glob("foo", "foo"));
        assert!(!matches_glob("foo.bar", "foo"));
        assert!(!matches_glob("foo", "bar"));
    }

    #[test]
    fn matches_glob_only_wildcard_matches_anything() {
        assert!(matches_glob("anything", "*"));
        assert!(matches_glob("", "*"));
    }

    #[test]
    fn matches_glob_empty_pattern_exact() {
        assert!(matches_glob("", ""));
        assert!(!matches_glob("foo", ""));
    }

    #[test]
    fn find_violations_detects_default_pattern() {
        let files = vec![
            "src/main.rs".to_string(),
            "__test.ps1".to_string(),
            "docs/__draft.md".to_string(),
            "src/__scratch.rs".to_string(),
        ];
        let patterns = vec!["__*".to_string()];
        let violations = find_violations(&files, &patterns);
        assert_eq!(
            violations,
            vec![
                "__test.ps1".to_string(),
                "docs/__draft.md".to_string(),
                "src/__scratch.rs".to_string()
            ]
        );
    }

    #[test]
    fn find_violations_empty_when_no_match() {
        let files = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
        let patterns = vec!["__*".to_string()];
        assert!(find_violations(&files, &patterns).is_empty());
    }

    #[test]
    fn find_violations_multiple_patterns() {
        let files = vec![
            "__test.ps1".to_string(),
            "_tmp_log.txt".to_string(),
            "src/main.rs".to_string(),
        ];
        let patterns = vec!["__*".to_string(), "_tmp_*".to_string()];
        let violations = find_violations(&files, &patterns);
        assert_eq!(violations.len(), 2);
        assert!(violations.contains(&"__test.ps1".to_string()));
        assert!(violations.contains(&"_tmp_log.txt".to_string()));
    }

    #[test]
    fn find_violations_reports_file_only_once_when_matching_multiple_patterns() {
        let files = vec!["__test.tmp".to_string()];
        let patterns = vec!["__*".to_string(), "*.tmp".to_string()];
        let violations = find_violations(&files, &patterns);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn find_violations_matches_basename_in_any_subdirectory() {
        let files = vec![
            "subdir/__hidden.txt".to_string(),
            r"win\path\__hidden.txt".to_string(),
            "__top.txt".to_string(),
        ];
        let patterns = vec!["__*".to_string()];
        assert_eq!(find_violations(&files, &patterns).len(), 3);
    }

    #[test]
    fn find_violations_ignores_dirname_prefix_match_when_basename_does_not_match() {
        let files = vec!["__src/main.rs".to_string()];
        let patterns = vec!["__*".to_string()];
        assert!(find_violations(&files, &patterns).is_empty());
    }

    #[test]
    fn parse_override_env_truthy() {
        for v in [
            "1", "true", "TRUE", "yes", "YES", "on", "On", " true ", "\tyes\n",
        ] {
            assert!(parse_override_env(Some(v)), "'{}' should be truthy", v);
        }
    }

    #[test]
    fn parse_override_env_falsy() {
        for v in ["0", "false", "no", "off", "", "   ", "maybe", "enable"] {
            assert!(!parse_override_env(Some(v)), "'{}' should be falsy", v);
        }
    }

    #[test]
    fn parse_override_env_none_is_false() {
        assert!(!parse_override_env(None));
    }

    #[test]
    fn effective_patterns_default_when_none() {
        let p = effective_patterns(None);
        assert_eq!(p, vec!["__*".to_string()]);
    }

    #[test]
    fn effective_patterns_default_when_no_patterns_field() {
        let config = ScratchFileWarningConfig {
            enabled: Some(true),
            patterns: None,
        };
        assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
    }

    #[test]
    fn effective_patterns_default_when_empty_list() {
        let config = ScratchFileWarningConfig {
            enabled: Some(true),
            patterns: Some(vec![]),
        };
        assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
    }

    #[test]
    fn effective_patterns_uses_config_when_provided() {
        let config = ScratchFileWarningConfig {
            enabled: Some(true),
            patterns: Some(vec!["__*".to_string(), "_tmp_*".to_string()]),
        };
        assert_eq!(
            effective_patterns(Some(&config)),
            vec!["__*".to_string(), "_tmp_*".to_string()]
        );
    }

    #[test]
    fn effective_patterns_default_when_only_blank_entries() {
        let config = ScratchFileWarningConfig {
            enabled: Some(true),
            patterns: Some(vec!["".to_string(), "   ".to_string()]),
        };
        assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
    }

    #[test]
    fn effective_patterns_filters_blank_entries_and_keeps_valid_ones() {
        let config = ScratchFileWarningConfig {
            enabled: Some(true),
            patterns: Some(vec![
                "".to_string(),
                "__*".to_string(),
                "   ".to_string(),
                "_tmp_*".to_string(),
            ]),
        };
        assert_eq!(
            effective_patterns(Some(&config)),
            vec!["__*".to_string(), "_tmp_*".to_string()]
        );
    }

    #[test]
    fn effective_patterns_trims_whitespace_in_pattern_values() {
        let config = ScratchFileWarningConfig {
            enabled: Some(true),
            patterns: Some(vec!["  __*  ".to_string()]),
        };
        assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
    }
}
