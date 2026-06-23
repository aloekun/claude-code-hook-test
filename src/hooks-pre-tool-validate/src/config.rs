//! 設定ファイル (`.claude/hooks-config.toml`) の deserialization。
//!
//! `[pre_tool_validate]` section + sub-section `[pre_tool_validate.todo_staleness]` を
//! 扱う。default-OFF in source (ADR-039 experimental pattern) のため、enabled フラグ
//! の Option<bool> は明示 true 指定が無ければ skip する。

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Default)]
pub(crate) struct Config {
    pub(crate) pre_tool_validate: Option<PreToolValidateConfig>,
}

#[derive(Deserialize, Default)]
pub(crate) struct PreToolValidateConfig {
    pub(crate) blocked_patterns: Option<Vec<String>>,
    pub(crate) extra_protected_files: Option<Vec<String>>,
    pub(crate) todo_staleness: Option<TodoStalenessConfig>,
}

/// 順位 136 案 B: `docs/todo*.md` Edit/Write 時の staleness 検知 + 既実装 grep 提示。
/// ADR-039 experimental pattern 準拠 (default-OFF in source、repo config で明示 enable)。
/// fail-closed (lineage 判定不能 = stale 扱いで安全側) per entry 設計決定。
#[derive(Deserialize, Default)]
pub(crate) struct TodoStalenessConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) default_branch: Option<String>,
    pub(crate) grep_recent_limit: Option<u64>,
}

pub(crate) const TODO_STALENESS_DEFAULT_BRANCH: &str = "master";
pub(crate) const TODO_STALENESS_DEFAULT_GREP_LIMIT: u64 = 20;
pub(crate) const TODO_STALENESS_JJ_TIMEOUT_SECS: u64 = 5;

/// 設定ファイルのパス解決: exe のあるディレクトリ / hooks-config.toml
pub(crate) fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む (存在しない場合はデフォルト)
pub(crate) fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "[validate-command] Warning: Failed to parse {}: {}",
                path.display(),
                e
            );
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}
