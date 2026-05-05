use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::log::log_info;

pub(crate) const DEFAULT_MAX_DURATION: u64 = 600;
pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 300;
pub(crate) const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize, Default)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) monitor: MonitorConfig,
    pub(crate) takt: Option<TaktConfig>,
    #[serde(default)]
    pub(crate) fix: FixConfig,
    #[serde(default)]
    pub(crate) rate_limit: RateLimitConfig,
    #[serde(default)]
    pub(crate) review_recheck: ReviewRecheckConfig,
}

#[derive(Deserialize, Clone)]
pub(crate) struct MonitorConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
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

#[derive(Deserialize, Clone)]
pub(crate) struct FixConfig {
    /// "critical" / "major" は自動 re-push。"none" および未知値はユーザー確認。
    #[serde(default = "default_auto_push_severity")]
    pub(crate) auto_push_severity: String,
    /// push コマンド (jj git push / git push)
    #[serde(default = "default_push_command")]
    pub(crate) push_command: String,
}

fn default_auto_push_severity() -> String {
    "critical".into()
}
fn default_push_command() -> String {
    "jj git push".into()
}

impl Default for FixConfig {
    fn default() -> Self {
        Self {
            auto_push_severity: default_auto_push_severity(),
            push_command: default_push_command(),
        }
    }
}

/// rate-limit 自動再 trigger の制御設定 (PR #89 T2-1)
///
/// CodeRabbit のレートリミット発火時、`max_retries` 回まで自動で
/// `@coderabbitai review` を再投稿する。上限超過は `action_required` で抜ける。
#[derive(Deserialize, Clone)]
pub(crate) struct RateLimitConfig {
    /// 自動 retry を行うかどうか。false の場合は rate-limit 検出しても sleep + retrigger しない。
    #[serde(default = "default_rate_limit_enabled")]
    pub(crate) auto_retry_enabled: bool,
    /// 累積 retry 上限。上限到達後は通常 polling 終了 (`action_required`) に抜ける。
    #[serde(default = "default_max_retries")]
    pub(crate) max_retries: u32,
}

fn default_rate_limit_enabled() -> bool {
    true
}
fn default_max_retries() -> u32 {
    3
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            auto_retry_enabled: default_rate_limit_enabled(),
            max_retries: default_max_retries(),
        }
    }
}

/// review 完了待ち park 制御 (Bb-3 順位 55)
///
/// CodeRabbit walkthrough 確認後、review 完了をポーリングする CronCreate 経路の
/// 待機秒数と最大再チェック回数を制御する。
/// 旧 hard-coded const (poll.rs INITIAL_REVIEW_WAIT_SECS / REVIEW_RECHECK_WAIT_SECS /
/// MAX_REVIEW_RECHECKS) を config 化したもの。
#[derive(Deserialize, Clone)]
pub(crate) struct ReviewRecheckConfig {
    /// fresh push 経路 (initial park) の wait 秒数
    #[serde(default = "default_initial_review_wait_secs")]
    pub(crate) initial_review_wait_secs: u64,
    /// wakeup 経路 (continue_monitoring) で次回 wakeup までの wait 秒数
    #[serde(default = "default_review_recheck_wait_secs")]
    pub(crate) review_recheck_wait_secs: u64,
    /// recheck 上限。到達後は action_required で抜ける
    #[serde(default = "default_max_review_rechecks")]
    pub(crate) max_review_rechecks: u32,
}

fn default_initial_review_wait_secs() -> u64 {
    300
}
fn default_review_recheck_wait_secs() -> u64 {
    300
}
fn default_max_review_rechecks() -> u32 {
    3
}

impl Default for ReviewRecheckConfig {
    fn default() -> Self {
        Self {
            initial_review_wait_secs: default_initial_review_wait_secs(),
            review_recheck_wait_secs: default_review_recheck_wait_secs(),
            max_review_rechecks: default_max_review_rechecks(),
        }
    }
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
max_duration_secs = 900
check_ci = true
check_coderabbit = false

[takt]
workflow = "post-pr-review"
task = "post-pr-review"
extra_args = ["--pipeline", "--skip-git"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.monitor.enabled);
        assert_eq!(config.monitor.max_duration_secs, 900);
        assert!(config.monitor.check_ci);
        assert!(!config.monitor.check_coderabbit);

        let takt = config.takt.unwrap();
        assert_eq!(takt.workflow, "post-pr-review");
        assert_eq!(takt.task, "post-pr-review");
        assert_eq!(takt.extra_args.as_ref().unwrap().len(), 2);
    }

    /// Bb-3: 旧 `poll_interval_secs` フィールド (Bb-2 で未使用化、Bb-3 で削除)
    /// が残った既存 config を読み込む際に、unknown field でパースエラーにならず
    /// 無視されることを確認する後方互換テスト。
    #[test]
    fn config_ignores_legacy_poll_interval_secs() {
        let toml_str = r#"
[monitor]
enabled = true
poll_interval_secs = 45
max_duration_secs = 900
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.monitor.enabled);
        assert_eq!(config.monitor.max_duration_secs, 900);
    }

    #[test]
    fn config_monitor_only_no_takt() {
        let toml_str = r#"
[monitor]
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.monitor.enabled);
        assert!(config.takt.is_none());
    }

    #[test]
    fn config_defaults_when_empty_monitor() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.monitor.enabled);
        assert_eq!(config.monitor.max_duration_secs, DEFAULT_MAX_DURATION);
    }

    #[test]
    fn disabled_config() {
        let toml_str = r#"
[monitor]
enabled = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.monitor.enabled);
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

    #[test]
    fn config_fix_defaults() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.fix.auto_push_severity, "critical");
        assert_eq!(config.fix.push_command, "jj git push");
    }

    #[test]
    fn config_fix_custom() {
        let toml_str = r#"
[monitor]

[fix]
auto_push_severity = "major"
push_command = "git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.fix.auto_push_severity, "major");
        assert_eq!(config.fix.push_command, "git push");
    }

    #[test]
    fn config_rate_limit_defaults() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.rate_limit.auto_retry_enabled);
        assert_eq!(config.rate_limit.max_retries, 3);
    }

    #[test]
    fn config_rate_limit_custom() {
        let toml_str = r#"
[monitor]

[rate_limit]
auto_retry_enabled = false
max_retries = 5
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.rate_limit.auto_retry_enabled);
        assert_eq!(config.rate_limit.max_retries, 5);
    }

    #[test]
    fn config_review_recheck_defaults() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.review_recheck.initial_review_wait_secs, 300);
        assert_eq!(config.review_recheck.review_recheck_wait_secs, 300);
        assert_eq!(config.review_recheck.max_review_rechecks, 3);
    }

    #[test]
    fn config_review_recheck_custom() {
        let toml_str = r#"
[monitor]

[review_recheck]
initial_review_wait_secs = 600
review_recheck_wait_secs = 900
max_review_rechecks = 5
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.review_recheck.initial_review_wait_secs, 600);
        assert_eq!(config.review_recheck.review_recheck_wait_secs, 900);
        assert_eq!(config.review_recheck.max_review_rechecks, 5);
    }
}
