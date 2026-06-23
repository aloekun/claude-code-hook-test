//! File size threshold check (Layer 0.5).
//!
//! 順位 177 (PR #197 で Tier 1 (優先実装) 格上げ済):
//! PostToolUse Edit / Write 直後にファイルサイズ閾値超過を検出して分割を促す。
//!
//! 由来: 4 PR 観測 (#133 / #172 / #186 / #197) で systemic risk = Very High frequency。
//! ADR-039 § 3 Bounded lifetime: 3-5 PR の dogfood 後に default-ON 昇格 or 却下を判定。

use crate::config::{Config, FileSizeCheckConfig};
use crate::violation::emit_feedback;
use globset::{Glob, GlobSet, GlobSetBuilder};

/// `FileSizeCheckConfig::paths` から GlobSet を compile する。
///
/// - 空 list → `Ok(None)` (no targets configured = no-op)
/// - 全 glob valid → `Ok(Some(GlobSet))`
/// - 1 つでも glob が invalid → `Err(error message)`
fn compile_size_paths_glob(paths: &[String]) -> Result<Option<GlobSet>, String> {
    if paths.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in paths {
        let glob = Glob::new(pattern).map_err(|e| format!("invalid glob '{}': {}", pattern, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| format!("failed to build GlobSet: {}", e))
}

/// 戻り値:
/// - `Some(message)`: feedback として emit する内容 (size 超過時)
/// - `None`: 無効化 / glob 不一致 / size 閾値内 / ファイル読込失敗のいずれか (no-op)
///
/// touch-trigger ratchet: MVP では `touch_trigger` フィールドは受理のみ、true/false いずれも
/// 「触られたファイルのみチェック」(= true の挙動) に統一。strict mode (= 全 enabled paths を
/// 毎回スキャン) は ADR-039 bounded lifetime dogfood 後に拡張予定。
pub(crate) fn check_file_size_threshold(
    file: &str,
    size_bytes: u64,
    config: &FileSizeCheckConfig,
) -> Option<String> {
    if !config.enabled {
        return None;
    }

    let glob_set = match compile_size_paths_glob(&config.paths) {
        Ok(Some(g)) => g,
        Ok(None) => return None,
        Err(msg) => {
            eprintln!(
                "[post-tool-linter] Warning: file_size_check paths glob compile failed: {}",
                msg
            );
            return None;
        }
    };
    let normalized = file.replace('\\', "/");
    if !glob_set.is_match(&normalized) {
        return None;
    }

    if size_bytes <= config.threshold_bytes {
        return None;
    }

    let recovery_hint = if normalized.contains("docs/todo") && normalized.ends_with(".md") {
        " (docs/todo*.md の場合は新 todo<N+1>.md を新設して entry を移管)"
    } else if normalized.ends_with(".rs") {
        " (Rust source の場合は module 分割を検討)"
    } else {
        ""
    };

    Some(format!(
        "[file-size-check] {}: ファイルサイズ {} bytes が threshold {} bytes (= {:.1} KB) を超過しています。ファイル分割を推奨します{}.",
        file,
        size_bytes,
        config.threshold_bytes,
        config.threshold_bytes as f64 / 1024.0,
        recovery_hint
    ))
}

/// PostToolUse file_size_check layer のエントリ。config 未指定 / metadata 読取失敗時は no-op。
pub(crate) fn run_file_size_layer(file: &str, config: &Config) {
    let Some(size_config) = config
        .post_tool_use
        .as_ref()
        .and_then(|c| c.file_size_check.as_ref())
    else {
        return;
    };
    let Ok(metadata) = std::fs::metadata(file) else {
        return;
    };
    if let Some(message) = check_file_size_threshold(file, metadata.len(), size_config) {
        emit_feedback(&message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_size_check_skips_when_disabled() {
        let config = FileSizeCheckConfig {
            enabled: false,
            threshold_bytes: 1_000,
            paths: vec!["docs/**/*.md".to_string()],
            touch_trigger: true,
        };
        let result = check_file_size_threshold("docs/sample.md", 100_000, &config);
        assert!(
            result.is_none(),
            "enabled=false must short-circuit even when size exceeds threshold"
        );
    }

    #[test]
    fn file_size_check_skips_when_path_does_not_match_glob() {
        let config = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 1_000,
            paths: vec!["docs/**/*.md".to_string(), "src/**/*.rs".to_string()],
            touch_trigger: true,
        };
        let result = check_file_size_threshold("scripts/build.sh", 100_000, &config);
        assert!(
            result.is_none(),
            "path not matching glob must skip even when size exceeds threshold"
        );
    }

    #[test]
    fn file_size_check_skips_when_size_within_threshold() {
        let config = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 1_000,
            paths: vec!["docs/**/*.md".to_string()],
            touch_trigger: true,
        };
        let result = check_file_size_threshold("docs/small.md", 500, &config);
        assert!(
            result.is_none(),
            "size within threshold (500 <= 1000) must skip"
        );
    }

    #[test]
    fn file_size_check_emits_message_when_size_exceeds_threshold() {
        let config = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 1_000,
            paths: vec!["src/**/*.rs".to_string()],
            touch_trigger: true,
        };
        let result = check_file_size_threshold("src/big.rs", 5_000, &config);
        let message = result.expect("size 5000 > threshold 1000 must emit feedback message");
        assert!(message.contains("file-size-check"));
        assert!(message.contains("5000"));
        assert!(message.contains("1000"));
        assert!(message.contains("module 分割"));
    }

    #[test]
    fn file_size_check_emits_todo_recovery_hint_for_docs_todo_files() {
        let config = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 51_200,
            paths: vec!["docs/**/*.md".to_string()],
            touch_trigger: true,
        };
        let result = check_file_size_threshold("docs/todoXYZ.md", 60_000, &config);
        let message = result.expect("60KB > 50KB threshold must emit");
        assert!(
            message.contains("todo<N+1>.md"),
            "docs/todo* prefix path should get the todo split hint, got: {}",
            message
        );
    }

    #[test]
    fn file_size_check_returns_none_when_paths_glob_is_empty() {
        let config = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 1_000,
            paths: vec![],
            touch_trigger: true,
        };
        let result = check_file_size_threshold("docs/anything.md", 100_000, &config);
        assert!(
            result.is_none(),
            "empty paths glob must skip (no targets configured)"
        );
    }

    #[test]
    fn file_size_check_treats_touch_trigger_false_same_as_true_in_mvp() {
        let mut cfg_strict = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 51_200,
            paths: vec!["docs/**/*.md".to_string()],
            touch_trigger: false,
        };
        let result_strict = check_file_size_threshold("docs/oversized.md", 60_000, &cfg_strict);
        cfg_strict.touch_trigger = true;
        let result_ratchet = check_file_size_threshold("docs/oversized.md", 60_000, &cfg_strict);
        assert!(
            result_strict.is_some(),
            "touch_trigger=false (MVP) must still emit for touched file"
        );
        assert!(
            result_ratchet.is_some(),
            "touch_trigger=true must emit for touched file"
        );
        assert_eq!(
            result_strict, result_ratchet,
            "MVP: touch_trigger=false behaves identically to true (strict mode = future work)"
        );
    }

    #[test]
    fn file_size_check_normalizes_windows_backslash_path() {
        let config = FileSizeCheckConfig {
            enabled: true,
            threshold_bytes: 1_000,
            paths: vec!["docs/**/*.md".to_string()],
            touch_trigger: true,
        };
        let result = check_file_size_threshold(r"docs\win.md", 60_000, &config);
        assert!(
            result.is_some(),
            "Windows backslash path must be normalized to forward slash for glob match"
        );
    }
}
