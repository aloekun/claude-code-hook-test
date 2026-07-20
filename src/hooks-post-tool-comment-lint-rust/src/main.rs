//! PostToolUse Rust lint hook (Rust 限定 PoC)
//!
//! Bundle Z Phase 1 (#B-α): 決定論 lint hook。Rust ファイルに対する複数の構造的制約を
//! 一つのバイナリで提供する (binary 名 `comment-lint-rust` は歴史的経緯で残置、
//! 派生プロジェクトの `hooks-config.toml` 互換のため rename しない)。
//!
//! 適用される lint:
//! - **コメント存在制約** (Bundle Z #B-α): 非 doc コメントを禁止、例外マーカーのみ許可。
//!   "Why コメント" "What コメント" の意味的区別は試みず、構造的に防止する。
//! - **関数長スケーリング検出** (順位 48 / PR #101 T1-4): 関数長 > 50 行を error として block。
//!   touch-trigger ratchet 方式 (既存超過関数は変更行に触れた瞬間にだけ flag、grandfather)。
//! - **ファイル長スケーリング検出** (順位 147 / Bundle "既存ルール仕組み化"): ファイル長 > 800 行を
//!   error として feedback。touch-trigger ratchet (既存超過ファイルは触られるまで grandfather)。
//!
//! ADR 整合:
//! - ADR-001: Rust 実装
//! - ADR-002: PostToolUse の Biome+oxlint 二段構成とは独立 entry として配置
//! - ADR-007: AST 層 (tree-sitter)、正規表現層ではない
//! - ADR-026: Cargo workspace member

use serde::{Deserialize, Serialize};
use std::io::{self, Read};

mod comment_lint;
mod file_length;
mod fix_metrics_check;
mod function_length;
mod line_filter;
mod metrics;
mod modified_files_check;
mod violations;

use line_filter::{
    collect_all_violations, compute_changed_lines, extract_file_path, is_rust_file, ToolInput,
};
use metrics::compute_metrics;
use violations::LintViolation;

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Serialize)]
struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

fn run_metrics_mode(file_path: &str) -> i32 {
    let source = match std::fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("metrics: read failed for {}: {}", file_path, e);
            return 2;
        }
    };
    let metrics = compute_metrics(&source);
    match serde_json::to_string(&metrics) {
        Ok(json) => {
            println!("{}", json);
            0
        }
        Err(e) => {
            eprintln!("metrics: serialize failed: {}", e);
            2
        }
    }
}

fn emit_feedback(message: &str) {
    let output = HookOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PostToolUse".to_string(),
            additional_context: message.to_string(),
        },
    };
    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}

fn emit_violations_feedback(violations: &[LintViolation]) {
    let serialized: Vec<String> = violations
        .iter()
        .filter_map(|v| serde_json::to_string(v).ok())
        .collect();
    let feedback = format!(
        "[comment-lint-rust] {} violation(s) found:\n{}",
        serialized.len(),
        serialized.join("\n")
    );
    emit_feedback(&feedback);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--metrics" {
        std::process::exit(run_metrics_mode(&args[2]));
    }
    if args.len() >= 3 && args[1] == "--fix-metrics-check" {
        let pre_revset = args
            .get(3)
            .map(String::as_str)
            .unwrap_or(fix_metrics_check::DEFAULT_PRE_REVSET);
        std::process::exit(fix_metrics_check::run_fix_metrics_check(&args[2], pre_revset));
    }
    if args.len() >= 2 && args[1] == "--check-modified-files" {
        std::process::exit(modified_files_check::run_check_modified_files());
    }

    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return,
    };

    let tool_name = hook_input.tool_name.clone();
    let tool_input = hook_input.tool_input.unwrap_or_default();
    let file_path = extract_file_path(&tool_input);

    if file_path.is_empty() || !is_rust_file(&file_path) {
        return;
    }

    let source = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let line_filter = compute_changed_lines(tool_name.as_deref(), &tool_input, &source);
    if matches!(line_filter.as_deref(), Some([])) {
        return;
    }

    let violations = collect_all_violations(&file_path, &source, line_filter.as_deref());
    if !violations.is_empty() {
        emit_violations_feedback(&violations);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_input_parses_full_edit_payload() {
        let json = r#"{
            "session_id": "abc",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/tmp/x.rs",
                "old_string": "let x = 1;",
                "new_string": "let x = 2; // comment",
                "replace_all": false
            }
        }"#;
        let parsed: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool_name.as_deref(), Some("Edit"));
        let t = parsed.tool_input.unwrap();
        assert_eq!(t.file_path.as_deref(), Some("/tmp/x.rs"));
        assert_eq!(t.new_string.as_deref(), Some("let x = 2; // comment"));
        assert_eq!(t.old_string.as_deref(), Some("let x = 1;"));
        assert!(!t.replace_all);
    }

    #[test]
    fn hook_input_parses_write_payload() {
        let json = r#"{
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/x.rs",
                "content": "fn foo() {}\n"
            }
        }"#;
        let parsed: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool_name.as_deref(), Some("Write"));
        let t = parsed.tool_input.unwrap();
        assert_eq!(t.content.as_deref(), Some("fn foo() {}\n"));
    }

    #[test]
    fn hook_input_parses_with_extra_fields() {
        let json = r#"{
            "session_id": "abc",
            "transcript_path": "/tmp/t.jsonl",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/tmp/x.rs",
                "new_string": "x"
            }
        }"#;
        let parsed: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool_name.as_deref(), Some("Edit"));
    }
}
