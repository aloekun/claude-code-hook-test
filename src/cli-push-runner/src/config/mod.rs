use serde::Deserialize;
use std::path::{Path, PathBuf};

mod lint_screen;
mod pr_size_check;
mod scratch_file_warning;

pub(crate) use lint_screen::{
    LintScreenConfig, DEFAULT_LINT_SCREEN_ENDPOINT, DEFAULT_LINT_SCREEN_EXE_PATH,
    DEFAULT_LINT_SCREEN_MAX_DIFF_LINES, DEFAULT_LINT_SCREEN_MODEL, DEFAULT_LINT_SCREEN_OUTPUT_PATH,
    DEFAULT_LINT_SCREEN_TIMEOUT_SECS,
};
pub(crate) use pr_size_check::{
    PrSizeCheckConfig, DEFAULT_PR_SIZE_BASE_BRANCH, DEFAULT_PR_SIZE_BLOCK_THRESHOLD,
    DEFAULT_PR_SIZE_WARNING_THRESHOLD,
};
pub(crate) use scratch_file_warning::ScratchFileWarningConfig;

use lint_screen::{apply_lint_screen_env_override, ENV_LINT_SCREEN_ENABLED};

pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;
pub(crate) const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

/// diff stage の既定 timeout (T6)。
///
/// 他の jj 系呼び出し (`bookmark_check` の `JJ_TIMEOUT_SECS = 30`) より長く取るのは、
/// diff が working copy の snapshot + 大 diff の書き出しを伴い、読み取りのみの
/// `jj bookmark list` より重いため。timeout の目的は**ハング検知**であって latency
/// 制限ではなく、誤 timeout は diff 失敗 = pipeline 全体の中断 (exit 5) を招くので
/// 余裕側に倒す。詰まる環境では `[diff] timeout` で上書きする。
pub(crate) const DEFAULT_DIFF_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) quality_gate: QualityGateConfig,
    pub(crate) diff: Option<DiffConfig>,
    pub(crate) lint_screen: Option<LintScreenConfig>,
    pub(crate) takt: TaktConfig,
    pub(crate) push: PushConfig,
    pub(crate) scratch_file_warning: Option<ScratchFileWarningConfig>,
    pub(crate) pr_size_check: Option<PrSizeCheckConfig>,
    pub(crate) pre_push_review: Option<PrePushReviewConfig>,
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

/// pre-push review の refute variant 制御 (WP-06 / ADR-047, 試験運用)。
///
/// ADR-039 (config opt-in): section 不在 / `refute_enabled != Some(true)` /
/// `refute_workflow` 未指定 のいずれでも現行 `[takt] workflow` を使う (default OFF)。
/// 明示的に `refute_enabled = true` かつ `refute_workflow` 指定時のみ refute
/// variant workflow に切り替わる。派生プロジェクトの templates は section を
/// 置かない or `refute_enabled = false` で default OFF を継承する。
#[derive(Deserialize)]
pub(crate) struct PrePushReviewConfig {
    pub(crate) refute_enabled: Option<bool>,
    pub(crate) refute_workflow: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DiffConfig {
    pub(crate) command: String,
    pub(crate) output_path: String,
    /// 未指定時は `DEFAULT_DIFF_TIMEOUT_SECS` (T6)。`[push] timeout` と同形。
    pub(crate) timeout: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct PushConfig {
    pub(crate) command: String,
    pub(crate) timeout: Option<u64>,
}

/// `push-runner-config.toml` の探索順序: カレントディレクトリ (pnpm scripts は
/// リポジトリルートで実行される) を優先し、無ければ exe 隣接パスに fallback する。
pub(crate) fn config_path() -> PathBuf {
    let filename = "push-runner-config.toml";
    let cwd_path = Path::new(filename).to_path_buf();
    if cwd_path.exists() {
        return cwd_path;
    }
    exe_adjacent_config_path(filename)
}

/// exe と同じディレクトリ (`.claude/` 配置パターン) 上の config path を返す。
/// `config_path` が cwd に見つからなかった場合の fallback。
fn exe_adjacent_config_path(filename: &str) -> PathBuf {
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
    let mut config: Config =
        toml::from_str(&content).map_err(|e| format!("設定ファイルのパースに失敗: {}", e))?;
    apply_lint_screen_env_override(&mut config, std::env::var(ENV_LINT_SCREEN_ENABLED).ok());
    validate_config(&config)?;
    Ok(config)
}

/// takt に渡す workflow 名を解決する (WP-06 / ADR-047)。
///
/// 切替判定を本関数 1 箇所に集約する (ADR-039 §設計6点 #5: 3 段 gate の単一化)。
/// `[pre_push_review] refute_enabled = true` かつ `refute_workflow` 指定時のみ
/// refute variant を返し、それ以外は現行 `[takt] workflow` を返す (fail-safe で
/// 現行フロー)。
pub(crate) fn resolve_takt_workflow(config: &Config) -> String {
    if let Some(pre_push) = &config.pre_push_review {
        if pre_push.refute_enabled == Some(true) {
            if let Some(workflow) = &pre_push.refute_workflow {
                return workflow.clone();
            }
        }
    }
    config.takt.workflow.clone()
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
        assert!(diff.timeout.is_none());
    }

    /// T6: `[diff] timeout` 未指定時は既定値に落ちる (本リポジトリの config は未指定)。
    #[test]
    fn config_diff_timeout_defaults() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let diff = config.diff.unwrap();
        assert!(diff.timeout.is_none());
        assert_eq!(
            diff.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS),
            DEFAULT_DIFF_TIMEOUT_SECS,
        );
    }

    /// T6: 大 diff / 低速環境向けの escape hatch (既定 60s では足りない場合)。
    #[test]
    fn config_diff_timeout_explicit() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"
timeout = 180

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.diff.unwrap().timeout, Some(180));
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
        assert!(config.quality_gate.parallel.unwrap_or(true));
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
            lint_screen: None,
            scratch_file_warning: None,
            pr_size_check: None,
            pre_push_review: None,
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
            lint_screen: None,
            scratch_file_warning: None,
            pr_size_check: None,
            pre_push_review: None,
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

    /// resolve_takt_workflow テスト用に base config + 任意の [pre_push_review]
    /// section を組み立てる。base workflow は "pre-push-review"。
    fn config_with_optional_pre_push(pre_push_section: &str) -> Config {
        let toml_str = format!(
            r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "echo push"
{pre_push_section}
"#
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn resolve_workflow_base_when_section_absent() {
        let config = config_with_optional_pre_push("");
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
    }

    #[test]
    fn resolve_workflow_base_when_refute_disabled() {
        let config = config_with_optional_pre_push(
            "[pre_push_review]\nrefute_enabled = false\nrefute_workflow = \"pre-push-review-refute\"",
        );
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
    }

    #[test]
    fn resolve_workflow_refute_when_enabled() {
        let config = config_with_optional_pre_push(
            "[pre_push_review]\nrefute_enabled = true\nrefute_workflow = \"pre-push-review-refute\"",
        );
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review-refute");
    }

    #[test]
    fn resolve_workflow_base_when_enabled_but_no_refute_workflow() {
        let config = config_with_optional_pre_push("[pre_push_review]\nrefute_enabled = true");
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
    }
}
