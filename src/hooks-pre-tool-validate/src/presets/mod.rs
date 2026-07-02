//! ブロックパターンのプリセット集合 + dispatch table。
//!
//! preset 名 → `Vec<BlockedPattern>` の解決を担う。新しい preset を追加する場合は
//! 該当 sub-module に `preset_xxx` 関数を作成し、`resolve_preset_or_custom` に
//! match arm を追加する。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) mod basic;
pub(crate) mod gh;
pub(crate) mod jj;
pub(crate) mod safety;

pub(crate) use basic::{preset_default, preset_electron, preset_git};
pub(crate) use gh::{preset_gh_pr_create_guard, preset_gh_pr_merge_guard, preset_gh_repo_env_guard};
pub(crate) use jj::{
    preset_jj_immutable, preset_jj_main_guard, preset_jj_message_required, preset_jj_push_guard,
};
pub(crate) use safety::{
    preset_exe_help_block, preset_polling_anti_pattern, preset_powershell_destructive_write,
    preset_secret_detection,
};

pub(crate) fn default_preset_names() -> Vec<String> {
    vec![
        "default".to_string(),
        "git".to_string(),
        "jj-immutable".to_string(),
        "jj-main-guard".to_string(),
        "jj-push-guard".to_string(),
        "electron".to_string(),
        "secret-detection".to_string(),
        "powershell-destructive-write-block".to_string(),
    ]
}

pub(crate) fn resolve_preset_or_custom(name: &str) -> Vec<BlockedPattern> {
    match name {
        "default" => preset_default(),
        "git" => preset_git(),
        "jj-immutable" => preset_jj_immutable(),
        "jj-main-guard" => preset_jj_main_guard(),
        "jj-push-guard" => preset_jj_push_guard(),
        "gh-pr-create-guard" => preset_gh_pr_create_guard(),
        "gh-pr-merge-guard" => preset_gh_pr_merge_guard(),
        "gh-repo-env-guard" => preset_gh_repo_env_guard(),
        "jj-message-required" => preset_jj_message_required(),
        "secret-detection" => preset_secret_detection(),
        "polling-anti-pattern" => preset_polling_anti_pattern(),
        "exe-help-block" => preset_exe_help_block(),
        "electron" => preset_electron(),
        "powershell-destructive-write-block" => preset_powershell_destructive_write(),
        custom => custom_regex_pattern(custom),
    }
}

pub(crate) fn custom_regex_pattern(custom: &str) -> Vec<BlockedPattern> {
    match Regex::new(custom) {
        Ok(re) => vec![BlockedPattern {
            pattern: re,
            exception: None,
            message: "**カスタムパターンによりブロックされました**\n\nこのコマンドは hooks-config.toml のカスタムルールによりブロックされています。",
        }],
        Err(_) => {
            eprintln!(
                "[validate-command] Warning: Invalid regex in blocked_patterns: {}",
                custom
            );
            Vec::new()
        }
    }
}
