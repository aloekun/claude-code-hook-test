use serde::Deserialize;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_POLL_INTERVAL: u64 = 120;
pub(crate) const DEFAULT_MAX_DURATION: u64 = 600;
pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 300;
pub(crate) const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize, Default)]
pub(crate) struct Config {
    pub(crate) post_pr_monitor: Option<PostPrMonitorConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct PostPrMonitorConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) poll_interval_secs: Option<u64>,
    pub(crate) max_duration_secs: Option<u64>,
    pub(crate) check_ci: Option<bool>,
    pub(crate) check_coderabbit: Option<bool>,
}

impl Default for PostPrMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            poll_interval_secs: Some(DEFAULT_POLL_INTERVAL),
            max_duration_secs: Some(DEFAULT_MAX_DURATION),
            check_ci: Some(true),
            check_coderabbit: Some(true),
        }
    }
}

pub(crate) fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

pub(crate) fn load_config() -> Config {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };
    toml::from_str(&content).unwrap_or_else(|e| {
        eprintln!(
            "[post-pr-monitor] hooks-config.toml パースエラー (デフォルト使用): {}",
            e
        );
        Config::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_post_pr_monitor() {
        let toml_str = r#"
[post_pr_monitor]
enabled = true
poll_interval_secs = 45
max_duration_secs = 900
check_ci = true
check_coderabbit = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, Some(true));
        assert_eq!(m.poll_interval_secs, Some(45));
        assert_eq!(m.max_duration_secs, Some(900));
        assert_eq!(m.check_ci, Some(true));
        assert_eq!(m.check_coderabbit, Some(false));
    }

    #[test]
    fn config_defaults_when_empty() {
        let toml_str = "[post_pr_monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, None);
        assert_eq!(m.poll_interval_secs, None);
    }

    #[test]
    fn config_missing_section() {
        let toml_str = "[stop_quality]\nstep_timeout = 60\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.post_pr_monitor.is_none());
    }

    #[test]
    fn disabled_config() {
        let toml_str = r#"
[post_pr_monitor]
enabled = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let m = config.post_pr_monitor.unwrap();
        assert_eq!(m.enabled, Some(false));
    }
}
