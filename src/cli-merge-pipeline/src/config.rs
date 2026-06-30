//! hooks-config.toml の `[merge_pipeline]` セクション読み込みと定数。

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// hooks-config.toml のトップレベル構造
#[derive(Deserialize, Default)]
pub(crate) struct Config {
    pub(crate) merge_pipeline: Option<MergePipelineConfig>,
}

/// `[merge_pipeline]` セクションの設定
#[derive(Deserialize, Default)]
pub(crate) struct MergePipelineConfig {
    pub(crate) step_timeout: Option<u64>,
    pub(crate) default_branch: Option<String>,
    pub(crate) pre_steps: Option<Vec<PipelineStepConfig>>,
    pub(crate) post_steps: Option<Vec<PipelineStepConfig>>,
}

/// パイプラインの個別ステップ定義
#[derive(Deserialize, Clone)]
pub(crate) struct PipelineStepConfig {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) step_type: String,
    pub(crate) cmd: Option<String>,
    /// 旧 ADR-029 で参照されていた hint。ADR-030 では takt workflow が固定なので未使用だが、
    /// hooks-config.toml の既存エントリと互換を保つため deserialize 対象として残す。
    #[allow(dead_code)]
    pub(crate) prompt: Option<String>,
}

/// デフォルトのブランチ名
pub(crate) const DEFAULT_BRANCH: &str = "master";

/// デフォルトのステップタイムアウト（秒）
pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;

/// マージコマンドのタイムアウト（秒）
pub(crate) const DEFAULT_MERGE_TIMEOUT_SECS: u64 = 300;

/// サブプロセス出力の最大収集行数（メモリ保護）
pub(crate) const MAX_LINES: usize = 200;

fn config_path() -> PathBuf {
    config_dir().join("hooks-config.toml")
}

/// exe と設定・pending file を配置するディレクトリ (`.claude/`)。
fn config_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

pub(crate) fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "hooks-config.toml の読み込みに失敗: {} ({})",
            path.display(),
            e
        )
    })?;
    toml::from_str(&content).map_err(|e| format!("hooks-config.toml のパースに失敗: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_merge_pipeline_with_pre_and_post_steps() {
        let toml_str = r#"
[merge_pipeline]
step_timeout = 60
default_branch = "main"

[[merge_pipeline.pre_steps]]
name = "ci_check"
type = "command"
cmd = "gh pr checks --required"

[[merge_pipeline.post_steps]]
name = "post_merge_learnings"
type = "ai"
prompt = "analyze_pr_learnings"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.merge_pipeline.unwrap();
        assert_eq!(pipeline.step_timeout.unwrap(), 60);
        assert_eq!(pipeline.default_branch.as_deref(), Some("main"));

        let pre = pipeline.pre_steps.unwrap();
        assert_eq!(pre.len(), 1);
        assert_eq!(pre[0].name, "ci_check");
        assert_eq!(pre[0].step_type, "command");

        let post = pipeline.post_steps.unwrap();
        assert_eq!(post.len(), 1);
        assert_eq!(post[0].name, "post_merge_learnings");
        assert_eq!(post[0].step_type, "ai");
        assert_eq!(post[0].prompt.as_deref(), Some("analyze_pr_learnings"));
    }

    #[test]
    fn config_defaults_when_empty() {
        let toml_str = r#"
[merge_pipeline]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.merge_pipeline.unwrap();
        assert_eq!(
            pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS
        );
        assert!(pipeline.pre_steps.unwrap_or_default().is_empty());
        assert!(pipeline.post_steps.unwrap_or_default().is_empty());
        assert_eq!(
            pipeline
                .default_branch
                .unwrap_or_else(|| DEFAULT_BRANCH.to_string()),
            DEFAULT_BRANCH
        );
    }

    #[test]
    fn config_missing_merge_pipeline_section() {
        let toml_str = r#"
[push_pipeline]
step_timeout = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.merge_pipeline.is_none());
    }
}
