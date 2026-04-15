use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::log::log_info;

pub(crate) const DEFAULT_POLL_INTERVAL: u64 = 120;
pub(crate) const DEFAULT_MAX_DURATION: u64 = 600;
pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 300;
pub(crate) const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize, Default)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) monitor: MonitorConfig,
    pub(crate) takt: Option<TaktConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct MonitorConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub(crate) poll_interval_secs: u64,
    #[serde(default = "default_max_duration")]
    pub(crate) max_duration_secs: u64,
    #[serde(default = "default_check_ci")]
    pub(crate) check_ci: bool,
    #[serde(default = "default_check_coderabbit")]
    pub(crate) check_coderabbit: bool,
}

fn default_enabled() -> bool {
    true
}
fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL
}
fn default_max_duration() -> u64 {
    DEFAULT_MAX_DURATION
}
fn default_check_ci() -> bool {
    true
}
fn default_check_coderabbit() -> bool {
    true
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            poll_interval_secs: default_poll_interval(),
            max_duration_secs: default_max_duration(),
            check_ci: default_check_ci(),
            check_coderabbit: default_check_coderabbit(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub(crate) struct TaktConfig {
    pub(crate) workflow: String,
    pub(crate) task: String,
    pub(crate) extra_args: Option<Vec<String>>,
}

fn config_path() -> PathBuf {
    let filename = "pr-monitor-config.toml";

    // 1. CWD を優先 (pnpm scripts はリポジトリルートで実行される)
    let cwd_path = Path::new(filename).to_path_buf();
    if cwd_path.exists() {
        return cwd_path;
    }

    // 2. exe が .claude/ 配下にある場合は repo ルートも見る
    let exe_dir = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    if exe_dir.file_name().and_then(|n| n.to_str()) == Some(".claude") {
        let repo_root_candidate = exe_dir.parent().unwrap_or(Path::new(".")).join(filename);
        if repo_root_candidate.exists() {
            return repo_root_candidate;
        }
    }

    exe_dir.join(filename)
}

pub(crate) fn load_config() -> Config {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log_info("pr-monitor-config.toml が見つかりません (デフォルト使用)");
            return Config::default();
        }
        Err(e) => {
            log_info(&format!(
                "pr-monitor-config.toml 読み込み失敗 (デフォルト使用): {}",
                e
            ));
            return Config::default();
        }
    };
    match toml::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            log_info(&format!(
                "pr-monitor-config.toml パースエラー (デフォルト使用): {}",
                e
            ));
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_full() {
        let toml_str = r#"
[monitor]
enabled = true
poll_interval_secs = 45
max_duration_secs = 900
check_ci = true
check_coderabbit = false

[takt]
workflow = "post-pr-review"
task = "analyze PR review comments"
extra_args = ["--pipeline", "--skip-git"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monitor.enabled, true);
        assert_eq!(config.monitor.poll_interval_secs, 45);
        assert_eq!(config.monitor.max_duration_secs, 900);
        assert_eq!(config.monitor.check_ci, true);
        assert_eq!(config.monitor.check_coderabbit, false);

        let takt = config.takt.unwrap();
        assert_eq!(takt.workflow, "post-pr-review");
        assert_eq!(takt.task, "analyze PR review comments");
        assert_eq!(takt.extra_args.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn config_monitor_only_no_takt() {
        let toml_str = r#"
[monitor]
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monitor.enabled, true);
        assert!(config.takt.is_none());
    }

    #[test]
    fn config_defaults_when_empty_monitor() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        // serde(default) により空の [monitor] でも MonitorConfig::default() と同じ値
        assert_eq!(config.monitor.enabled, true);
        assert_eq!(config.monitor.poll_interval_secs, DEFAULT_POLL_INTERVAL);
    }

    #[test]
    fn disabled_config() {
        let toml_str = r#"
[monitor]
enabled = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monitor.enabled, false);
    }

    #[test]
    fn config_takt_extra_args_optional() {
        let toml_str = r#"
[monitor]

[takt]
workflow = "w"
task = "t"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let takt = config.takt.unwrap();
        assert!(takt.extra_args.is_none());
    }
}
