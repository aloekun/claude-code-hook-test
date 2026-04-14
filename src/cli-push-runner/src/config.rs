use serde::Deserialize;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;
pub(crate) const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) quality_gate: QualityGateConfig,
    pub(crate) diff: Option<DiffConfig>,
    pub(crate) takt: TaktConfig,
    pub(crate) push: PushConfig,
}

#[derive(Deserialize)]
pub(crate) struct QualityGateConfig {
    pub(crate) parallel: Option<bool>,
    pub(crate) step_timeout: Option<u64>,
    pub(crate) groups: Vec<GroupConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct GroupConfig {
    pub(crate) name: String,
    pub(crate) pre: Option<String>,
    pub(crate) commands: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct TaktConfig {
    pub(crate) workflow: String,
    pub(crate) task: String,
    pub(crate) extra_args: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(crate) struct DiffConfig {
    pub(crate) command: String,
    pub(crate) output_path: String,
}

#[derive(Deserialize)]
pub(crate) struct PushConfig {
    pub(crate) command: String,
    pub(crate) timeout: Option<u64>,
}

pub(crate) fn config_path() -> PathBuf {
    let filename = "push-runner-config.toml";

    // 1. カレントディレクトリを優先（pnpm scripts はリポジトリルートで実行される）
    let cwd_path = Path::new(filename).to_path_buf();
    if cwd_path.exists() {
        return cwd_path;
    }

    // 2. exe と同じディレクトリ（.claude/ 配置パターン）
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(filename)
}

pub(crate) fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("設定ファイルの読み込みに失敗: {} ({})", path.display(), e))?;
    let config: Config =
        toml::from_str(&content).map_err(|e| format!("設定ファイルのパースに失敗: {}", e))?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &Config) -> Result<(), String> {
    if config.quality_gate.groups.is_empty() {
        return Err("設定ファイルエラー: quality_gate.groups が空です".into());
    }
    for group in &config.quality_gate.groups {
        if group.commands.is_empty() {
            return Err(format!(
                "設定ファイルエラー: group '{}' の commands が空です",
                group.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_full_without_diff() {
        let toml_str = r#"
[quality_gate]
parallel = true
step_timeout = 60

[[quality_gate.groups]]
name = "lint"
commands = ["pnpm lint"]

[[quality_gate.groups]]
name = "test"
pre = "pnpm install"
commands = ["pnpm test", "pnpm test:e2e"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"
extra_args = ["--pipeline", "--skip-git"]

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.quality_gate.parallel, Some(true));
        assert_eq!(config.quality_gate.step_timeout, Some(60));
        assert_eq!(config.quality_gate.groups.len(), 2);
        assert!(config.diff.is_none());

        assert_eq!(config.takt.workflow, "pre-push-review");
        assert_eq!(config.takt.task, "pre-push review");
        assert_eq!(config.takt.extra_args.as_ref().unwrap().len(), 2);

        assert_eq!(config.push.command, "jj git push");
        assert!(config.push.timeout.is_none());
    }

    #[test]
    fn config_push_timeout_explicit() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "jj git push"
timeout = 600
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.push.timeout, Some(600));
        assert_eq!(
            config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
            600,
        );
    }

    #[test]
    fn config_push_timeout_defaults() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.push.timeout.is_none());
        assert_eq!(
            config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
            DEFAULT_PUSH_TIMEOUT_SECS,
        );
    }

    #[test]
    fn config_parses_with_diff() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        let diff = config.diff.unwrap();
        assert_eq!(diff.command, "jj diff -r @");
        assert_eq!(diff.output_path, ".takt/review-diff.txt");
    }

    #[test]
    fn config_quality_gate_defaults() {
        let toml_str = r#"
[quality_gate]

[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.quality_gate.parallel.unwrap_or(true), true);
        assert_eq!(
            config
                .quality_gate
                .step_timeout
                .unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS,
        );
        assert!(config.takt.extra_args.is_none());
    }

    #[test]
    fn config_pre_field_optional() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "no-pre"
commands = ["echo test"]

[[quality_gate.groups]]
name = "with-pre"
pre = "echo install"
commands = ["echo test"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.quality_gate.groups[0].pre.is_none());
        assert!(config.quality_gate.groups[1].pre.is_some());
    }

    #[test]
    fn validate_rejects_empty_groups() {
        let config = Config {
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![],
            },
            diff: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("groups が空"));
    }

    #[test]
    fn validate_rejects_empty_commands() {
        let config = Config {
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![GroupConfig {
                    name: "empty".into(),
                    pre: None,
                    commands: vec![],
                }],
            },
            diff: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'empty'"));
    }
}
