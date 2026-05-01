//! PostToolUse comment lint hook (Rust 限定 PoC)
//!
//! Bundle Z Phase 1 (#B-α): 決定論 comment lint hook。
//! Rust ファイルに対してコメント存在自体を制約し、例外マーカーのみ許可する。
//! "Why コメント" "What コメント" の意味的区別は試みず、構造的に防止する。
//!
//! ADR 整合:
//! - ADR-001: Rust 実装
//! - ADR-002: PostToolUse の Biome+oxlint 二段構成とは独立 entry として配置
//! - ADR-007: AST 層 (tree-sitter)、正規表現層ではない
//! - ADR-026: Cargo workspace member
//!
//! 例外マーカー一覧は `ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES` 参照。

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: Option<String>,
    path: Option<String>,
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

#[derive(Serialize)]
struct LintViolation {
    r#type: String,
    severity: String,
    location: ViolationLocation,
    message: String,
    why: String,
    fix: ViolationFix,
    example: ViolationExample,
}

#[derive(Serialize)]
struct ViolationLocation {
    file: String,
    line: usize,
    symbol: String,
}

#[derive(Serialize)]
struct ViolationFix {
    strategy: String,
    steps: Vec<String>,
}

#[derive(Serialize)]
struct ViolationExample {
    bad: String,
    good: String,
}

const MAX_VIOLATIONS: usize = 20;

/// 例外マーカー (line_comment): 行頭スペース除去後にこれらのいずれかで始まれば許可
const ALLOWED_LINE_PREFIXES: &[&str] = &[
    "///",
    "//!",
    "// TODO:",
    "// FIXME:",
    "// SAFETY:",
    "// NOTE:",
    "// HACK:",
    "// XXX:",
];

/// 例外マーカー (block_comment): rustdoc 形式のみ許可
const ALLOWED_BLOCK_PREFIXES: &[&str] = &["/**", "/*!"];

fn is_allowed_comment(comment_text: &str) -> bool {
    let trimmed = comment_text.trim_start();
    if trimmed.starts_with("//") {
        ALLOWED_LINE_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
    } else if trimmed.starts_with("/*") {
        ALLOWED_BLOCK_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
    } else {
        false
    }
}

fn find_violations(file_path: &str, source: &str) -> Vec<LintViolation> {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::language();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let query_source = "[(line_comment) (block_comment)] @comment";
    let query = match Query::new(&language, query_source) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = QueryCursor::new();
    let mut violations = Vec::new();
    let source_bytes = source.as_bytes();

    let matches = cursor.matches(&query, tree.root_node(), source_bytes);
    'outer: for m in matches {
        for capture in m.captures {
            let node = capture.node;
            let comment_text = match node.utf8_text(source_bytes) {
                Ok(t) => t,
                Err(_) => continue,
            };

            if is_allowed_comment(comment_text) {
                continue;
            }

            let start = node.start_position();
            let snippet = comment_text
                .lines()
                .next()
                .unwrap_or(comment_text)
                .to_string();

            violations.push(LintViolation {
                r#type: "RUST_COMMENT_FORBIDDEN".to_string(),
                severity: "error".to_string(),
                location: ViolationLocation {
                    file: file_path.to_string(),
                    line: start.row + 1,
                    symbol: snippet,
                },
                message: "非 doc コメントは禁止です (Bundle Z #B-α)".to_string(),
                why: "コメントの存在自体を制約する決定論層 (Bundle Z #B-α)。\
                      コメントを書きたくなったら識別子名 / 関数分割で意図を表現すること。"
                    .to_string(),
                fix: ViolationFix {
                    strategy: "コメントを削除し、識別子名や関数分割で意図を表現".to_string(),
                    steps: vec![
                        "コメントを削除する".to_string(),
                        "(必要なら) 関数を分割して名前で意図を伝える".to_string(),
                        "(必要なら) 変数名を意図を表す名前にリネーム".to_string(),
                        "Why コメントが本当に必要なら // SAFETY: / // NOTE: 等のマーカー付きで書き直す".to_string(),
                    ],
                },
                example: ViolationExample {
                    bad: "// 成功時のみ更新する\nstate.value = Some(x);".to_string(),
                    good: "if let Ok(updated) = result { state.value = Some(updated); }"
                        .to_string(),
                },
            });

            if violations.len() >= MAX_VIOLATIONS {
                break 'outer;
            }
        }
    }

    violations
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

fn is_rust_file(file_path: &str) -> bool {
    Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("rs"))
        .unwrap_or(false)
}

fn main() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return,
    };

    let file_path = hook_input
        .tool_input
        .and_then(|t| t.file_path.filter(|s| !s.is_empty()).or(t.path))
        .unwrap_or_default();

    if file_path.is_empty() || !is_rust_file(&file_path) {
        return;
    }

    let source = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let violations = find_violations(&file_path, &source);
    if violations.is_empty() {
        return;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str) -> Vec<LintViolation> {
        find_violations("test.rs", source)
    }

    #[test]
    fn empty_file_no_violations() {
        let violations = lint("");
        assert!(violations.is_empty());
    }

    #[test]
    fn no_comments_no_violations() {
        let source = "fn main() {\n    let x = 1;\n}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn line_comment_detected() {
        let source = "fn main() {\n    // 値を更新する\n    let x = 1;\n}\n";
        let violations = lint(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].location.line, 2);
    }

    #[test]
    fn block_comment_detected() {
        let source = "fn main() {\n    /* これは説明 */\n    let x = 1;\n}\n";
        let violations = lint(source);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn rustdoc_outer_allowed() {
        let source = "/// Public doc\nfn foo() {}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn rustdoc_inner_allowed() {
        let source = "//! Module doc\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn block_rustdoc_outer_allowed() {
        let source = "/** Public doc */\nfn foo() {}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn block_rustdoc_inner_allowed() {
        let source = "/*! Module doc */\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn todo_marker_allowed() {
        let source = "fn main() {\n    // TODO: implement later\n}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn fixme_marker_allowed() {
        let source = "// FIXME: race condition\nfn main() {}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn safety_marker_allowed() {
        let source = "// SAFETY: ptr is non-null\nlet _x = 1;\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn note_marker_allowed() {
        let source = "// NOTE: temporary workaround\nfn main() {}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn hack_marker_allowed() {
        let source = "// HACK: workaround for issue\nfn main() {}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn xxx_marker_allowed() {
        let source = "// XXX: investigate\nfn main() {}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn marker_must_be_at_start() {
        let source = "// 説明 TODO: not at start\nfn main() {}\n";
        let violations = lint(source);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn comment_inside_string_not_detected() {
        // NOTE: tree-sitter は文字列リテラル内の `//` を comment と認識しない (regress 防止)
        let source = "fn main() {\n    let s = \"// not a comment\";\n}\n";
        let violations = lint(source);
        assert!(violations.is_empty());
    }

    #[test]
    fn multiple_violations_collected() {
        let source = "// foo\n// bar\n// baz\nfn main() {}\n";
        let violations = lint(source);
        assert_eq!(violations.len(), 3);
    }

    #[test]
    fn mixed_doc_and_forbidden() {
        let source = "/// Public doc\n// 禁止コメント\nfn foo() {}\n";
        let violations = lint(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].location.line, 2);
    }

    #[test]
    fn max_violations_capped() {
        let mut source = String::new();
        for i in 0..30 {
            source.push_str(&format!("// comment {}\n", i));
        }
        source.push_str("fn main() {}\n");
        let violations = lint(&source);
        assert_eq!(violations.len(), MAX_VIOLATIONS);
    }

    #[test]
    fn is_rust_file_accepts_rs() {
        assert!(is_rust_file("main.rs"));
        assert!(is_rust_file("src/lib.rs"));
        assert!(is_rust_file(r"e:\work\project\src\app.rs"));
    }

    #[test]
    fn is_rust_file_case_insensitive() {
        assert!(is_rust_file("file.RS"));
        assert!(is_rust_file("file.Rs"));
    }

    #[test]
    fn is_rust_file_rejects_other() {
        assert!(!is_rust_file("main.ts"));
        assert!(!is_rust_file("style.css"));
        assert!(!is_rust_file("Makefile"));
        assert!(!is_rust_file(""));
    }

    #[test]
    fn violation_json_has_all_fields() {
        let source = "// 説明\nfn main() {}\n";
        let violations = lint(source);
        let json = serde_json::to_string(&violations[0]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "RUST_COMMENT_FORBIDDEN");
        assert_eq!(v["severity"], "error");
        assert!(v.get("location").is_some());
        assert!(v["location"].get("file").is_some());
        assert!(v["location"].get("line").is_some());
        assert!(v["location"].get("symbol").is_some());
        assert!(v.get("message").is_some());
        assert!(v.get("why").is_some());
        assert!(v.get("fix").is_some());
        assert!(v.get("example").is_some());
    }

    #[test]
    fn is_allowed_comment_handles_leading_whitespace() {
        assert!(is_allowed_comment("    /// doc"));
        assert!(is_allowed_comment("\t// TODO: x"));
        assert!(!is_allowed_comment("    // forbidden"));
    }
}
