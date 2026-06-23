//! `BlockedPattern` 型定義と pattern build / validate ロジック。

use crate::config::Config;
use crate::presets::{default_preset_names, resolve_preset_or_custom};
use regex::Regex;

pub(crate) struct BlockedPattern {
    pub(crate) pattern: Regex,
    /// 順位 144 (PR #171 T3-#8 採用): pattern match 後にこの regex が hit する場合は allow。
    /// Rust 標準 regex crate は negative lookahead 非対応のため 2 段判定で「pattern match
    /// AND exception 不一致」の semantic を実現する。`None` の場合は従来通り pattern match で block。
    pub(crate) exception: Option<Regex>,
    pub(crate) message: &'static str,
}

pub(crate) fn build_blocked_patterns(config: &Config) -> Vec<BlockedPattern> {
    let preset_names: Vec<String> = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.blocked_patterns.as_ref())
        .cloned()
        .unwrap_or_else(default_preset_names);
    preset_names
        .iter()
        .flat_map(|name| resolve_preset_or_custom(name.as_str()))
        .collect()
}

pub(crate) fn validate_command(command: &str, patterns: &[BlockedPattern]) -> Option<&'static str> {
    for pattern in patterns {
        if pattern.pattern.is_match(command) {
            if let Some(exc) = &pattern.exception {
                if exc.is_match(command) {
                    continue;
                }
            }
            return Some(pattern.message);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, PreToolValidateConfig};

    fn patterns_with_presets(presets: &[&str]) -> Vec<BlockedPattern> {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(presets.iter().map(|s| s.to_string()).collect()),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        build_blocked_patterns(&config)
    }

    fn is_blocked(command: &str) -> bool {
        validate_command(command, &build_blocked_patterns(&Config::default())).is_some()
    }

    fn is_blocked_with(command: &str, presets: &[&str]) -> bool {
        validate_command(command, &patterns_with_presets(presets)).is_some()
    }

    #[test]
    fn default_config_enables_all_presets() {
        assert!(is_blocked("git push"));
        assert!(is_blocked("rm -rf /tmp"));
        assert!(is_blocked("jj --ignore-immutable rebase"));
        assert!(is_blocked("jj new main"));
        assert!(is_blocked("electron ."));
    }

    #[test]
    fn empty_presets_blocks_nothing() {
        let patterns = patterns_with_presets(&[]);
        assert!(validate_command("git push", &patterns).is_none());
        assert!(validate_command("rm -rf /tmp", &patterns).is_none());
    }

    #[test]
    fn custom_regex_pattern() {
        assert!(is_blocked_with("docker rm -f container", &[r"docker\s+rm"]));
        assert!(!is_blocked_with("docker ps", &[r"docker\s+rm"]));
    }
}
