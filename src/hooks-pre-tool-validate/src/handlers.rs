//! Tool 別 handler (Bash / Write / Edit / PowerShell)。

use crate::blocked_patterns::{build_blocked_patterns, validate_command};
use crate::config::Config;
use crate::presets::{default_preset_names, preset_secret_detection};
use crate::protected_files::is_protected_config;
use crate::todo_staleness::check_todo_staleness;
use crate::ToolInput;
use std::io::{self, Write};
use std::process::ExitCode;

pub(crate) fn handle_bash_tool(config: &Config, tool_input: &ToolInput) -> ExitCode {
    let command = tool_input.command.clone().unwrap_or_default();
    if command.trim().is_empty() {
        return ExitCode::SUCCESS;
    }
    let patterns = build_blocked_patterns(config);
    if let Some(message) = validate_command(&command, &patterns) {
        let _ = io::stderr().write_all(message.as_bytes());
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// 順位 212: PowerShell tool 用ハンドラ。`handle_bash_tool` と同形で
/// `tool_input.command` を `build_blocked_patterns` の全 preset で照合する。
///
/// PowerShell preset (`powershell-destructive-write-block`) は default-on のため
/// `WriteAllText` / `WriteAllBytes` / `WriteAllLines` / `Out-File` / `Set-Content -Value` を
/// block する。Bash と共通の guard (git push / jj immutable / electron 等) も
/// 同時に適用される (PowerShell 上で git push 等を叩く case もカバー)。
pub(crate) fn handle_powershell_tool(config: &Config, tool_input: &ToolInput) -> ExitCode {
    let command = tool_input.command.clone().unwrap_or_default();
    if command.trim().is_empty() {
        return ExitCode::SUCCESS;
    }
    let patterns = build_blocked_patterns(config);
    if let Some(message) = validate_command(&command, &patterns) {
        let _ = io::stderr().write_all(message.as_bytes());
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

pub(crate) fn handle_write_edit_tool(config: &Config, tool_input: &ToolInput) -> ExitCode {
    let file_path = resolve_edit_file_path(tool_input);
    if let Some(code) = check_protected_file(config, &file_path) {
        return code;
    }
    if let Some(code) = check_secret_in_content(config, tool_input) {
        return code;
    }
    if let Some(code) = check_todo_staleness_for_edit(config, tool_input, &file_path) {
        return code;
    }
    ExitCode::SUCCESS
}

fn resolve_edit_file_path(tool_input: &ToolInput) -> String {
    tool_input
        .file_path
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| tool_input.path.clone())
        .unwrap_or_default()
}

fn check_protected_file(config: &Config, file_path: &str) -> Option<ExitCode> {
    let extra_protected = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.extra_protected_files.as_ref())
        .cloned()
        .unwrap_or_default();
    if file_path.is_empty() || !is_protected_config(file_path, &extra_protected) {
        return None;
    }
    let msg = format!(
        "**保護されたファイルの編集がブロックされました**\n\n\
         `{}` は保護対象ファイル（設定ファイル/機密ファイル）のため、編集が禁止されています。\n\n\
         リンター設定の場合: 設定を変更するのではなく **コード側を修正** してください。\n\
         機密ファイルの場合: 秘密情報の漏洩を防ぐため、編集できません。\n\n\
         変更が本当に必要な場合は、ユーザーに確認を取ってください。",
        file_path.rsplit(['/', '\\']).next().unwrap_or(file_path)
    );
    let _ = io::stderr().write_all(msg.as_bytes());
    Some(ExitCode::from(2))
}

fn check_secret_in_content(config: &Config, tool_input: &ToolInput) -> Option<ExitCode> {
    if !is_secret_detection_enabled(config) {
        return None;
    }
    let scan_text = collect_text_for_secret_scan(tool_input);
    if scan_text.is_empty() {
        return None;
    }
    let secret_patterns = preset_secret_detection();
    let message = validate_command(&scan_text, &secret_patterns)?;
    let _ = io::stderr().write_all(message.as_bytes());
    Some(ExitCode::from(2))
}

fn check_todo_staleness_for_edit(
    config: &Config,
    tool_input: &ToolInput,
    file_path: &str,
) -> Option<ExitCode> {
    let staleness_config = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.todo_staleness.as_ref())?;
    let text = collect_text_for_keywords(tool_input);
    let result = check_todo_staleness(file_path, &text, staleness_config)?;
    let _ = io::stderr().write_all(result.message.as_bytes());
    if result.stale {
        Some(ExitCode::from(2))
    } else {
        None
    }
}

fn collect_text_for_keywords(tool_input: &ToolInput) -> String {
    let mut parts = Vec::new();
    if let Some(old) = &tool_input.old_string {
        parts.push(old.as_str());
    }
    if let Some(new_s) = &tool_input.new_string {
        parts.push(new_s.as_str());
    }
    if let Some(content) = &tool_input.content {
        parts.push(content.as_str());
    }
    parts.join("\n")
}

/// Edit/Write 時の secret scan 対象テキスト (new_string + content のみ、old_string は除外)。
/// 順位 146 (PR #200 follow-up): old_string は「既存ファイル内の文字列 = 削除対象 or 置換元」
/// であり、ここを scan すると「secret を削除する Edit」までも block してしまうため除外する。
pub(crate) fn collect_text_for_secret_scan(tool_input: &ToolInput) -> String {
    let mut parts = Vec::new();
    if let Some(new_s) = &tool_input.new_string {
        parts.push(new_s.as_str());
    }
    if let Some(content) = &tool_input.content {
        parts.push(content.as_str());
    }
    parts.join("\n")
}

pub(crate) fn is_secret_detection_enabled(config: &Config) -> bool {
    let preset_names: Vec<String> = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.blocked_patterns.as_ref())
        .cloned()
        .unwrap_or_else(default_preset_names);
    preset_names.iter().any(|n| n == "secret-detection")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PreToolValidateConfig;

    #[test]
    fn is_secret_detection_enabled_returns_true_when_listed_in_blocked_patterns() {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(vec!["secret-detection".to_string()]),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        assert!(is_secret_detection_enabled(&config));
    }

    #[test]
    fn is_secret_detection_enabled_returns_false_when_excluded_from_blocked_patterns() {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(vec!["default".to_string(), "git".to_string()]),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        assert!(!is_secret_detection_enabled(&config));
    }

    #[test]
    fn is_secret_detection_enabled_returns_true_for_default_config_default_on() {
        assert!(is_secret_detection_enabled(&Config::default()));
    }

    #[test]
    fn collect_text_for_secret_scan_excludes_old_string_to_allow_secret_removal() {
        let removed = format!("{}{}", "AKIA", "IOSFODNN7EXAMPLE");
        let placeholder = "AKIATEST".to_string();
        let tool_input = ToolInput {
            command: None,
            file_path: Some("foo.rs".to_string()),
            path: None,
            old_string: Some(removed.clone()),
            new_string: Some(placeholder.clone()),
            content: None,
        };
        let scanned = collect_text_for_secret_scan(&tool_input);
        assert!(!scanned.contains(&removed));
        assert!(scanned.contains(&placeholder));
    }

    #[test]
    fn collect_text_for_secret_scan_includes_both_new_string_and_content() {
        let tool_input = ToolInput {
            command: None,
            file_path: Some("foo.rs".to_string()),
            path: None,
            old_string: None,
            new_string: Some("new-text".to_string()),
            content: Some("full-content".to_string()),
        };
        let scanned = collect_text_for_secret_scan(&tool_input);
        assert!(scanned.contains("new-text"));
        assert!(scanned.contains("full-content"));
    }

    fn build_todo_path(suffix: &str) -> String {
        format!("docs/todo{}.md", suffix)
    }

    #[test]
    fn collect_text_for_keywords_combines_fields() {
        let input = ToolInput {
            command: None,
            file_path: Some(build_todo_path("")),
            path: None,
            old_string: Some("old text".to_string()),
            new_string: Some("new text".to_string()),
            content: Some("full content".to_string()),
        };
        let text = collect_text_for_keywords(&input);
        assert!(text.contains("old text"));
        assert!(text.contains("new text"));
        assert!(text.contains("full content"));
    }
}
