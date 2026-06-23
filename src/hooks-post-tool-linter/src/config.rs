//! `hooks-config.toml` の `[post_tool_linter]` / `[post_tool_use]` セクションを読み込む
//! 設定構造体と default factory 群。
//!
//! ## レイヤ概念
//!
//! - **Layer 0.5** (`[post_tool_use.file_size_check]`): PostToolUse Edit / Write 直後の
//!   ファイルサイズ閾値検出。default OFF (ADR-039 opt-in pattern)。
//! - **Layer 1** (`[post_tool_linter]` 配下の custom rules): `.claude/custom-lint-rules.toml`
//!   経由の regex ベースのカスタムリンタ。
//! - **Layer 2** (`[post_tool_linter].pipelines`): 拡張子ごとの biome / oxlint / ruff /
//!   markdownlint パイプライン実行。

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Default)]
pub(crate) struct Config {
    pub(crate) post_tool_linter: Option<PostToolLinterConfig>,
    pub(crate) post_tool_use: Option<PostToolUseConfig>,
}

#[derive(Deserialize, Default)]
pub(crate) struct PostToolLinterConfig {
    pub(crate) pipelines: Option<Vec<PipelineConfig>>,
}

/// `[post_tool_use]` section: PostToolUse hook の non-linter sub-features.
///
/// 順位 177 (PR #197 で Tier 1 (優先実装) に格上げ済) で「ファイルサイズ閾値検出」を追加。
/// 既存 `[post_tool_linter]` (Layer 1 = custom-rules / Layer 2 = pipeline) とは独立した
/// Layer 0.5 として動作する。ADR-039 opt-in pattern 準拠で default OFF。
#[derive(Deserialize, Default)]
pub(crate) struct PostToolUseConfig {
    pub(crate) file_size_check: Option<FileSizeCheckConfig>,
}

/// `[post_tool_use.file_size_check]` section.
///
/// PostToolUse Edit / Write 直後にファイルサイズを確認し、threshold 超過時に
/// additionalContext で分割を促す。touch-trigger ratchet (default true) で
/// 既存超過ファイルは触られるまで grandfather される。
///
/// 由来: 4 PR 観測 (#133 / #172 / #186 / #197) で systemic risk = Very High frequency。
/// ADR-039 § 3 Bounded lifetime: 3-5 PR の dogfood 後に default-ON 昇格 or 却下を判定。
#[derive(Deserialize, Clone)]
pub(crate) struct FileSizeCheckConfig {
    /// ADR-039 § kill-switch: `false` で完全停止 (default false = opt-in)。
    #[serde(default)]
    pub(crate) enabled: bool,
    /// Threshold (bytes). Default 51200 = 50KB (Claude Code 読み取り安定性閾値)。
    #[serde(default = "default_file_size_threshold_bytes")]
    pub(crate) threshold_bytes: u64,
    /// 対象ファイルの glob list。default は markdown + Rust source。
    #[serde(default = "default_file_size_paths")]
    pub(crate) paths: Vec<String>,
    /// touch-trigger ratchet: `true` (default) なら触られたファイルのみチェック =
    /// 既存超過ファイルは未編集なら grandfather。`false` (strict) は将来の拡張で
    /// 「全 enabled paths を毎回スキャン」を予定 (MVP では受理のみ、挙動は true と同じ)。
    #[serde(default = "default_file_size_touch_trigger")]
    #[allow(dead_code)]
    pub(crate) touch_trigger: bool,
}

fn default_file_size_threshold_bytes() -> u64 {
    51200
}

fn default_file_size_paths() -> Vec<String> {
    vec!["docs/**/*.md".to_string(), "src/**/*.rs".to_string()]
}

fn default_file_size_touch_trigger() -> bool {
    true
}

#[derive(Deserialize, Clone)]
pub(crate) struct PipelineConfig {
    pub(crate) extensions: Vec<String>,
    pub(crate) steps: Vec<StepConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct StepConfig {
    pub(crate) cmd: String,
    pub(crate) args: Vec<String>,
    pub(crate) fix: bool,
}

/// デフォルトパイプライン (設定ファイルが無い場合のフォールバック)
pub(crate) fn default_pipelines() -> Vec<PipelineConfig> {
    vec![default_ts_pipeline(), default_py_pipeline()]
}

fn default_ts_pipeline() -> PipelineConfig {
    PipelineConfig {
        extensions: vec!["ts".into(), "tsx".into(), "js".into(), "jsx".into()],
        steps: vec![
            StepConfig {
                cmd: "npx".into(),
                args: vec![
                    "--no-install".into(),
                    "biome".into(),
                    "format".into(),
                    "--write".into(),
                    "{file}".into(),
                ],
                fix: true,
            },
            StepConfig {
                cmd: "npx".into(),
                args: vec![
                    "--no-install".into(),
                    "oxlint".into(),
                    "--fix".into(),
                    "{file}".into(),
                ],
                fix: true,
            },
            StepConfig {
                cmd: "npx".into(),
                args: vec!["--no-install".into(), "oxlint".into(), "{file}".into()],
                fix: false,
            },
        ],
    }
}

fn default_py_pipeline() -> PipelineConfig {
    PipelineConfig {
        extensions: vec!["py".into()],
        steps: vec![
            StepConfig {
                cmd: "ruff".into(),
                args: vec!["check".into(), "--fix".into(), "{file}".into()],
                fix: true,
            },
            StepConfig {
                cmd: "ruff".into(),
                args: vec!["format".into(), "{file}".into()],
                fix: true,
            },
            StepConfig {
                cmd: "ruff".into(),
                args: vec!["check".into(), "{file}".into()],
                fix: false,
            },
        ],
    }
}

/// 設定ファイルのパス解決
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む
pub(crate) fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "[post-tool-linter] Warning: Failed to parse {}: {}",
                path.display(),
                e
            );
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}
