//! PostToolUse リンターフック (設定駆動型)
//!
//! Write/Edit ツール使用後にファイルに対してリンター/フォーマッターを実行し、
//! 診断結果を additionalContext として Claude にフィードバックします。
//!
//! `.claude/hooks-config.toml` の `[post_tool_linter]` / `[post_tool_use]` セクションから
//! 拡張子ごとのパイプラインと file_size_check 等の sub-feature を読み込みます。
//!
//! ## レイヤ構成
//!
//! 1. **UTF-8 整合性** (`utf8_integrity`): U+FFFD 検出時は即 feedback + 後続スキップ
//! 2. **Layer 0.5 file size check** (`file_size_check`): 閾値超過時に分割を促す feedback
//! 3. **Layer 1 custom rules** (`custom_rules`): regex ベースのカスタムリンタ
//! 4. **Layer 2 pipeline** (`pipeline_runner`): biome / oxlint / ruff 等の外部ツール

mod config;
mod custom_rules;
mod file_size_check;
mod pipeline_runner;
mod utf8_integrity;
mod violation;

use serde::Deserialize;
use std::io::{self, Read};

use config::load_config;
use custom_rules::run_custom_rules_layer;
use file_size_check::run_file_size_layer;
use pipeline_runner::run_pipeline_layer;
use utf8_integrity::run_utf8_layer;

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: Option<String>,
    path: Option<String>,
}

fn read_hook_input_file() -> Option<String> {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[post-tool-linter] Warning: Failed to read stdin: {}", e);
        return None;
    }
    let hook_input: HookInput = serde_json::from_str(&input).ok()?;
    let file = hook_input
        .tool_input
        .and_then(|t| t.file_path.filter(|s| !s.is_empty()).or(t.path))
        .unwrap_or_default();
    if file.is_empty() {
        None
    } else {
        Some(file)
    }
}

fn main() {
    let config = load_config();
    let Some(file) = read_hook_input_file() else {
        return;
    };
    if run_utf8_layer(&file) {
        return;
    }
    run_file_size_layer(&file, &config);
    run_custom_rules_layer(&file);
    run_pipeline_layer(&file, config);
}
