use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::log::log_info;

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
    pub(crate) classifier: ClassifierConfig,
}

#[derive(Deserialize, Clone)]
pub(crate) struct MonitorConfig {
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "default_check_ci")]
    pub(crate) check_ci: bool,
    #[serde(default = "default_check_coderabbit")]
    pub(crate) check_coderabbit: bool,
}

fn default_enabled() -> bool {
    true
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
    #[serde(default)]
    pub(crate) sweep: SweepConfig,
    #[serde(default)]
    pub(crate) gate: crate::stages::gate::GateConfig,
    #[serde(default)]
    pub(crate) scope_guard: crate::stages::scope_guard::ScopeGuardConfig,
    /// auto-push 成功後に `@coderabbitai review` を投稿して再レビューを明示発火するか。
    ///
    /// `.coderabbit.yaml` の `reviews.auto_review.auto_incremental_review = false`
    /// と結合する (WP-03 / ADR-019 amendment)。増分レビュー抑止時は push だけでは
    /// CodeRabbit が再レビューしないため、監視側から明示トリガーする必要がある。
    /// デフォルト false (opt-in): auto_incremental_review が有効な環境で有効化すると
    /// 二重レビューになるため、`.coderabbit.yaml` 側と揃えて明示的に true にする。
    #[serde(default)]
    pub(crate) trigger_review_after_push: bool,
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
            sweep: SweepConfig::default(),
            gate: crate::stages::gate::GateConfig::default(),
            scope_guard: crate::stages::scope_guard::ScopeGuardConfig::default(),
            trigger_review_after_push: false,
        }
    }
}

/// PR 範囲内の `fix(review):` 空 commit を自動 abandon する sweep 設定。
///
/// デフォルトは `enabled = false` (opt-in)。有効化後は `default_branch..@` 範囲の
/// `fix(review):` 空 commit を `execute_repush_flow` 末尾で自動 abandon する。
#[derive(Deserialize, Clone)]
pub(crate) struct SweepConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default = "default_sweep_branch")]
    pub(crate) default_branch: String,
}

fn default_sweep_branch() -> String {
    "master".into()
}

impl Default for SweepConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_branch: default_sweep_branch(),
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

/// CodeRabbit findings をローカル LLM (Ollama) で classify する設定 (ADR-038、Phase 5)。
///
/// `cli-finding-classifier.exe` を subprocess invoke し、`Vec<Finding>` を
/// `Vec<ClassifiedFinding>` に enrich する。デフォルトは無効 (`enabled = false`、
/// 試験運用)。Ollama 不在 / 失敗時は classifier 側 fallback で全件 `human_review`
/// に倒れるため、有効化しても polling が block しない。
#[derive(Deserialize, Clone)]
pub(crate) struct ClassifierConfig {
    /// classifier を invoke するかどうか
    #[serde(default = "default_classifier_enabled")]
    pub(crate) enabled: bool,
    /// Ollama モデル名 (`--model` に渡す)
    #[serde(default = "default_classifier_model")]
    pub(crate) model: String,
    /// Ollama HTTP endpoint (`--endpoint` に渡す)
    #[serde(default = "default_classifier_endpoint")]
    pub(crate) endpoint: String,
    /// 1 リクエストあたりタイムアウト秒 (`--timeout-secs` に渡す)
    #[serde(default = "default_classifier_timeout_secs")]
    pub(crate) timeout_secs: u64,
}

fn default_classifier_enabled() -> bool {
    false
}
fn default_classifier_model() -> String {
    "mistral:7b".into()
}
fn default_classifier_endpoint() -> String {
    "http://localhost:11434".into()
}
fn default_classifier_timeout_secs() -> u64 {
    30
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            enabled: default_classifier_enabled(),
            model: default_classifier_model(),
            endpoint: default_classifier_endpoint(),
            timeout_secs: default_classifier_timeout_secs(),
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
    match toml::from_str::<Config>(&content) {
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
        assert!(config.monitor.check_ci);
        assert!(!config.monitor.check_coderabbit);

        let takt = config.takt.unwrap();
        assert_eq!(takt.workflow, "post-pr-review");
        assert_eq!(takt.task, "post-pr-review");
        assert_eq!(takt.extra_args.as_ref().unwrap().len(), 2);
    }

    /// 旧フィールドが残った既存 config を読み込む際に、unknown field でパース
    /// エラーにならず無視されることを確認する後方互換テスト (poll_interval_secs は
    /// Bb-3 で、max_duration_secs は WP-17 PR 3 で削除)。
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
        assert!(!config.fix.sweep.enabled, "sweep はデフォルト無効 (opt-in)");
        assert_eq!(config.fix.sweep.default_branch, "master");
        assert!(
            config.fix.gate.enabled,
            "gate はデフォルト有効 (fail-closed、ADR-043)"
        );
        assert_eq!(config.fix.gate.group, "rust-lint-test");
        assert!(
            !config.fix.trigger_review_after_push,
            "trigger_review_after_push はデフォルト無効 (opt-in、.coderabbit.yaml と結合)"
        );
    }

    #[test]
    fn config_fix_gate_override() {
        let toml_str = r#"
[monitor]

[fix.gate]
enabled = false
group = "custom-group"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.fix.gate.enabled);
        assert_eq!(config.fix.gate.group, "custom-group");
    }

    #[test]
    fn config_fix_custom() {
        let toml_str = r#"
[monitor]

[fix]
auto_push_severity = "major"
push_command = "git push"
trigger_review_after_push = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.fix.auto_push_severity, "major");
        assert_eq!(config.fix.push_command, "git push");
        assert!(
            config.fix.trigger_review_after_push,
            "trigger_review_after_push = true が parse されること"
        );
    }

    #[test]
    fn config_fix_sweep_defaults() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.fix.sweep.enabled, "sweep はデフォルト無効 (opt-in)");
        assert_eq!(config.fix.sweep.default_branch, "master");
    }

    #[test]
    fn config_fix_sweep_custom() {
        let toml_str = r#"
[monitor]

[fix.sweep]
enabled = true
default_branch = "main"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.fix.sweep.enabled);
        assert_eq!(config.fix.sweep.default_branch, "main");
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

    /// PR 3 (wakeup 廃止): 旧 config file に残る `[review_recheck]` セクションは
    /// unknown field として無視され、パースが失敗しないこと (park モデル時代の
    /// pr-monitor-config.toml との前方互換)。
    #[test]
    fn config_with_removed_review_recheck_section_still_parses() {
        let toml_str = r#"
[monitor]

[review_recheck]
initial_review_wait_secs = 600
review_recheck_wait_secs = 900
max_review_rechecks = 5
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.monitor.enabled);
    }

    #[test]
    fn config_classifier_defaults() {
        let toml_str = "[monitor]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.classifier.enabled, "デフォルトは無効 (試験運用)");
        assert_eq!(config.classifier.model, "mistral:7b");
        assert_eq!(config.classifier.endpoint, "http://localhost:11434");
        assert_eq!(config.classifier.timeout_secs, 30);
    }

    #[test]
    fn config_classifier_custom() {
        let toml_str = r#"
[monitor]

[classifier]
enabled = true
model = "llama2:13b"
endpoint = "http://192.168.1.10:11434"
timeout_secs = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.classifier.enabled);
        assert_eq!(config.classifier.model, "llama2:13b");
        assert_eq!(config.classifier.endpoint, "http://192.168.1.10:11434");
        assert_eq!(config.classifier.timeout_secs, 60);
    }

}
