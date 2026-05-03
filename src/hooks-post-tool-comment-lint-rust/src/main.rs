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
use tree_sitter::{Node, Parser, Query, QueryCursor, Tree};

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct ToolInput {
    file_path: Option<String>,
    path: Option<String>,
    old_string: Option<String>,
    new_string: Option<String>,
    content: Option<String>,
    #[serde(default)]
    replace_all: bool,
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

/// 順位 50 (PR #102 T1-1): Edit が触れた行のみ lint 対象にするため、`new_string` の出現位置から
/// 変更行範囲を導出する。
///
/// 戻り値:
/// - `None`: フィルタなし (= ファイル全体を lint)。Write / MultiEdit / 不明 tool / `new_string` が
///   見つからない場合 (line ending 差異等) のフォールバック。
/// - `Some(ranges)`: 1-indexed inclusive 範囲のみ lint。
/// - `Some(empty)`: lint をスキップ (Edit で `new_string` が空 = 純削除)。
fn compute_changed_lines(
    tool_name: Option<&str>,
    tool_input: &ToolInput,
    post_source: &str,
) -> Option<Vec<(usize, usize)>> {
    match tool_name {
        Some("Edit") => {
            let new_string = tool_input.new_string.as_deref()?;
            if new_string.is_empty() {
                return Some(Vec::new());
            }
            let ranges = locate_string_line_ranges(post_source, new_string);
            if ranges.is_empty() {
                None
            } else {
                Some(ranges)
            }
        }
        _ => None,
    }
}

fn locate_string_line_ranges(source: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut search_start = 0;
    while search_start <= source.len() {
        let remaining = &source[search_start..];
        match remaining.find(needle) {
            Some(idx) => {
                let absolute = search_start + idx;
                let start_line = byte_offset_to_line(source, absolute);
                let end_line = byte_offset_to_line(source, absolute + needle.len() - 1);
                ranges.push((start_line, end_line));
                search_start = (absolute + needle.len()).min(source.len());
            }
            None => break,
        }
    }
    ranges
}

fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    let clamped = offset.min(source.len());
    source[..clamped].bytes().filter(|b| *b == b'\n').count() + 1
}

fn span_overlaps_ranges(start: usize, end: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|(s, e)| start <= *e && end >= *s)
}

fn line_in_ranges(line: usize, ranges: &[(usize, usize)]) -> bool {
    span_overlaps_ranges(line, line, ranges)
}

fn find_violations(
    file_path: &str,
    source: &str,
    line_filter: Option<&[(usize, usize)]>,
) -> Vec<LintViolation> {
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
            let line = start.row + 1;

            if let Some(ranges) = line_filter {
                if !span_overlaps_ranges(line, node.end_position().row + 1, ranges) {
                    continue;
                }
            }

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

/// File-level metrics for `--metrics` mode (Bundle Z Phase 2 / #B-β)。
///
/// 出力 JSON は `scripts/fix-metrics-check.ps1` が pre/post 比較に使用する。
/// `non_doc_comment_count` は `find_violations` と同じ判定ロジックで計上 (例外マーカー除外)。
#[derive(Serialize, Debug, PartialEq, Eq, Default)]
struct FileMetrics {
    non_doc_comment_count: usize,
    functions: Vec<FunctionMetric>,
}

/// 関数単位のメトリクス。pre/post 比較は `name` で関数を突き合わせる (識別子変更時は新規扱い)。
#[derive(Serialize, Debug, PartialEq, Eq)]
struct FunctionMetric {
    name: String,
    line_start: usize,
    line_end: usize,
    length: usize,
    max_nesting_depth: usize,
}

fn compute_metrics(source: &str) -> FileMetrics {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::language();
    if parser.set_language(&language).is_err() {
        return FileMetrics::default();
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return FileMetrics::default(),
    };
    let source_bytes = source.as_bytes();
    FileMetrics {
        non_doc_comment_count: count_non_doc_comments(&tree, source_bytes, &language),
        functions: collect_functions(&tree, source_bytes),
    }
}

fn count_non_doc_comments(
    tree: &Tree,
    source_bytes: &[u8],
    language: &tree_sitter::Language,
) -> usize {
    let query = match Query::new(language, "[(line_comment) (block_comment)] @comment") {
        Ok(q) => q,
        Err(_) => return 0,
    };
    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, tree.root_node(), source_bytes);
    let mut count = 0;
    for m in matches {
        for capture in m.captures {
            if let Ok(text) = capture.node.utf8_text(source_bytes) {
                if !is_allowed_comment(text) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn collect_functions(tree: &Tree, source_bytes: &[u8]) -> Vec<FunctionMetric> {
    let mut results = Vec::new();
    let mut cursor = tree.walk();
    visit_function_nodes(&mut cursor, source_bytes, &mut results);
    results
}

fn visit_function_nodes(
    cursor: &mut tree_sitter::TreeCursor,
    source_bytes: &[u8],
    out: &mut Vec<FunctionMetric>,
) {
    let node = cursor.node();
    if node.kind() == "function_item" {
        if let Some(metric) = function_metric(node, source_bytes) {
            out.push(metric);
        }
    }
    if cursor.goto_first_child() {
        loop {
            visit_function_nodes(cursor, source_bytes, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn function_metric(node: Node, source_bytes: &[u8]) -> Option<FunctionMetric> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source_bytes).ok()?.to_string();

    let start = node.start_position();
    let end = node.end_position();
    let line_start = start.row + 1;
    let line_end = end.row + 1;
    let length = line_end - line_start + 1;

    let body = node.child_by_field_name("body");
    let max_nesting_depth = match body {
        Some(b) => max_block_depth_inside_body(b),
        None => 0,
    };

    Some(FunctionMetric {
        name,
        line_start,
        line_end,
        length,
        max_nesting_depth,
    })
}

/// 関数 body block 内の最大ネスト深度を計算する。
///
/// 関数 body 自体は深度 0 とし、その内部の `block` ノード (if / loop / match arm body /
/// closure body / block expression 等) を発見するたびに +1 する。
/// 例:
/// - `fn foo() { let x = 1; }` → 0
/// - `fn foo() { if x { ... } }` → 1
/// - `fn foo() { if x { if y { ... } } }` → 2
fn max_block_depth_inside_body(body: Node) -> usize {
    let mut max = 0;
    walk_for_blocks(body, 0, &mut max);
    max
}

fn walk_for_blocks(node: Node, depth: usize, max: &mut usize) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let child_depth = if child.kind() == "block" {
                let d = depth + 1;
                if d > *max {
                    *max = d;
                }
                d
            } else {
                depth
            };
            walk_for_blocks(child, child_depth, max);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
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

fn is_rust_file(file_path: &str) -> bool {
    Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("rs"))
        .unwrap_or(false)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--metrics" {
        let exit_code = run_metrics_mode(&args[2]);
        std::process::exit(exit_code);
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

    let file_path = tool_input
        .file_path
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| tool_input.path.clone())
        .unwrap_or_default();

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

    let violations = find_violations(&file_path, &source, line_filter.as_deref());
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
        find_violations("test.rs", source, None)
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

    #[test]
    fn metrics_empty_file_zero_everything() {
        let m = compute_metrics("");
        assert_eq!(m.non_doc_comment_count, 0);
        assert!(m.functions.is_empty());
    }

    #[test]
    fn metrics_single_function_no_nesting() {
        let source = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.non_doc_comment_count, 0);
        assert_eq!(m.functions.len(), 1);
        let f = &m.functions[0];
        assert_eq!(f.name, "foo");
        assert_eq!(f.line_start, 1);
        assert_eq!(f.line_end, 4);
        assert_eq!(f.length, 4);
        assert_eq!(f.max_nesting_depth, 0);
    }

    #[test]
    fn metrics_single_if_block_depth_one() {
        let source = "fn foo(x: i32) {\n    if x > 0 {\n        let y = x;\n    }\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.functions.len(), 1);
        assert_eq!(m.functions[0].max_nesting_depth, 1);
    }

    #[test]
    fn metrics_nested_if_depth_two() {
        let source =
            "fn foo(x: i32, y: i32) {\n    if x > 0 {\n        if y > 0 {\n            let z = x + y;\n        }\n    }\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.functions[0].max_nesting_depth, 2);
    }

    #[test]
    fn metrics_match_arm_with_block() {
        let source = "fn foo(x: i32) -> i32 {\n    match x {\n        0 => 1,\n        _ => {\n            let y = x * 2;\n            y\n        }\n    }\n}\n";
        let m = compute_metrics(source);
        assert!(
            m.functions[0].max_nesting_depth >= 1,
            "match arm body block contributes to depth"
        );
    }

    #[test]
    fn metrics_multiple_functions_each_tracked() {
        let source = "fn foo() {\n    let x = 1;\n}\n\nfn bar() {\n    if true {\n        let y = 2;\n    }\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.functions.len(), 2);
        let foo = m.functions.iter().find(|f| f.name == "foo").unwrap();
        let bar = m.functions.iter().find(|f| f.name == "bar").unwrap();
        assert_eq!(foo.max_nesting_depth, 0);
        assert_eq!(bar.max_nesting_depth, 1);
    }

    #[test]
    fn metrics_allowed_comment_not_counted() {
        let source = "/// Public doc\nfn foo() {\n    // TODO: implement\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.non_doc_comment_count, 0);
    }

    #[test]
    fn metrics_forbidden_comment_counted() {
        let source = "fn foo() {\n    // forbidden 1\n    // forbidden 2\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.non_doc_comment_count, 2);
    }

    #[test]
    fn metrics_function_length_inclusive_of_braces() {
        let source = "fn foo() {\n    let x = 1;\n}\n";
        let m = compute_metrics(source);
        assert_eq!(m.functions[0].length, 3);
    }

    #[test]
    fn metrics_function_signature_in_trait_not_tracked() {
        let source = "trait T {\n    fn no_body(&self);\n}\n";
        let m = compute_metrics(source);
        assert!(
            !m.functions.iter().any(|f| f.name == "no_body"),
            "trait method signatures (function_signature_item) are not function_item — skipped"
        );
    }

    #[test]
    fn metrics_trait_default_method_with_body_tracked() {
        let source = "trait T {\n    fn default_impl(&self) {\n        let _x = 1;\n    }\n}\n";
        let m = compute_metrics(source);
        assert!(
            m.functions.iter().any(|f| f.name == "default_impl"),
            "default methods (with body) are function_item — tracked"
        );
    }

    #[test]
    fn metrics_json_serialization_roundtrip() {
        let source = "fn foo() {\n    if true { let x = 1; }\n}\n";
        let m = compute_metrics(source);
        let json = serde_json::to_string(&m).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["non_doc_comment_count"], 0);
        let functions = v["functions"].as_array().unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0]["name"], "foo");
        assert_eq!(functions[0]["max_nesting_depth"], 1);
    }

    #[test]
    fn metrics_closure_with_block_increments_depth() {
        let source = "fn foo() {\n    let _f = |x: i32| { x + 1 };\n}\n";
        let m = compute_metrics(source);
        assert!(
            m.functions[0].max_nesting_depth >= 1,
            "closure body block contributes to depth"
        );
    }

    #[test]
    fn locate_string_line_ranges_single_line_match() {
        let source = "fn foo() {\n    let x = 1;\n    // new comment\n}\n";
        let ranges = locate_string_line_ranges(source, "// new comment");
        assert_eq!(ranges, vec![(3, 3)]);
    }

    #[test]
    fn locate_string_line_ranges_multiline_match() {
        let source = "fn foo() {\n    let x = 1;\n    // line1\n    // line2\n}\n";
        let ranges = locate_string_line_ranges(source, "// line1\n    // line2");
        assert_eq!(ranges, vec![(3, 4)]);
    }

    #[test]
    fn locate_string_line_ranges_multiple_occurrences() {
        let source = "// foo\nfn bar() {}\n// foo\n";
        let ranges = locate_string_line_ranges(source, "// foo");
        assert_eq!(ranges, vec![(1, 1), (3, 3)]);
    }

    #[test]
    fn locate_string_line_ranges_empty_needle_returns_empty() {
        let ranges = locate_string_line_ranges("source", "");
        assert!(ranges.is_empty());
    }

    #[test]
    fn locate_string_line_ranges_no_match_returns_empty() {
        let ranges = locate_string_line_ranges("fn foo() {}\n", "missing");
        assert!(ranges.is_empty());
    }

    #[test]
    fn locate_string_line_ranges_handles_multibyte_utf8() {
        let source = "fn foo() {\n    let s = \"日本語\";\n    let t = \"日本語\";\n}\n";
        let ranges = locate_string_line_ranges(source, "\"日本語\"");
        assert_eq!(ranges, vec![(2, 2), (3, 3)]);
    }

    #[test]
    fn byte_offset_to_line_handles_offsets_correctly() {
        let s = "abc\ndef\nghi\n";
        assert_eq!(byte_offset_to_line(s, 0), 1);
        assert_eq!(byte_offset_to_line(s, 3), 1);
        assert_eq!(byte_offset_to_line(s, 4), 2);
        assert_eq!(byte_offset_to_line(s, 8), 3);
    }

    #[test]
    fn line_in_ranges_inclusive_bounds() {
        let ranges = [(2, 4), (10, 10)];
        assert!(!line_in_ranges(1, &ranges));
        assert!(line_in_ranges(2, &ranges));
        assert!(line_in_ranges(3, &ranges));
        assert!(line_in_ranges(4, &ranges));
        assert!(!line_in_ranges(5, &ranges));
        assert!(line_in_ranges(10, &ranges));
    }

    #[test]
    fn span_overlaps_ranges_detects_overlap() {
        let ranges = [(5, 10)];
        assert!(span_overlaps_ranges(1, 5, &ranges));
        assert!(span_overlaps_ranges(5, 15, &ranges));
        assert!(span_overlaps_ranges(6, 8, &ranges));
        assert!(!span_overlaps_ranges(1, 4, &ranges));
        assert!(!span_overlaps_ranges(11, 20, &ranges));
    }

    #[test]
    fn find_violations_multiline_block_comment_spanning_range_boundary() {
        let source = "/* line1\n   line2\n   line3 */\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[(3, 4)]));
        assert_eq!(v.len(), 1, "block comment starting at line 1 but extending into range should be detected");
    }

    fn tool_input_with(new_string: Option<&str>) -> ToolInput {
        ToolInput {
            new_string: new_string.map(|s| s.to_string()),
            ..ToolInput::default()
        }
    }

    #[test]
    fn compute_changed_lines_edit_with_empty_new_string_signals_skip() {
        let t = tool_input_with(Some(""));
        let result = compute_changed_lines(Some("Edit"), &t, "fn foo() {}\n");
        assert_eq!(result, Some(Vec::new()));
    }

    #[test]
    fn compute_changed_lines_edit_locates_change() {
        let t = tool_input_with(Some("// new"));
        let post = "fn foo() {\n    // new\n}\n";
        let result = compute_changed_lines(Some("Edit"), &t, post);
        assert_eq!(result, Some(vec![(2, 2)]));
    }

    #[test]
    fn compute_changed_lines_edit_unioned_when_multiple_matches() {
        let t = tool_input_with(Some("// dup"));
        let post = "// dup\nfn foo() {}\n// dup\n";
        let result = compute_changed_lines(Some("Edit"), &t, post);
        assert_eq!(result, Some(vec![(1, 1), (3, 3)]));
    }

    #[test]
    fn compute_changed_lines_edit_no_match_falls_back_to_no_filter() {
        let t = tool_input_with(Some("missing"));
        let result = compute_changed_lines(Some("Edit"), &t, "fn foo() {}\n");
        assert_eq!(result, None);
    }

    #[test]
    fn compute_changed_lines_edit_without_new_string_returns_none() {
        let t = tool_input_with(None);
        let result = compute_changed_lines(Some("Edit"), &t, "fn foo() {}\n");
        assert_eq!(result, None);
    }

    #[test]
    fn compute_changed_lines_write_no_filter() {
        let t = tool_input_with(Some("// ignored for Write"));
        let result = compute_changed_lines(Some("Write"), &t, "fn foo() {}\n");
        assert_eq!(result, None);
    }

    #[test]
    fn compute_changed_lines_multiedit_no_filter_v1() {
        let t = tool_input_with(Some("// ignored for MultiEdit"));
        let result = compute_changed_lines(Some("MultiEdit"), &t, "fn foo() {}\n");
        assert_eq!(result, None);
    }

    #[test]
    fn compute_changed_lines_unknown_tool_no_filter() {
        let t = tool_input_with(Some("// any"));
        let result = compute_changed_lines(Some("UnknownTool"), &t, "fn foo() {}\n");
        assert_eq!(result, None);
    }

    #[test]
    fn compute_changed_lines_no_tool_name_no_filter() {
        let t = tool_input_with(Some("// any"));
        let result = compute_changed_lines(None, &t, "fn foo() {}\n");
        assert_eq!(result, None);
    }

    #[test]
    fn find_violations_with_filter_excludes_outside_lines() {
        let source = "// outside\nfn foo() {\n    // inside\n}\n";
        let v = find_violations("test.rs", source, Some(&[(3, 3)]));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].location.line, 3);
    }

    #[test]
    fn find_violations_with_filter_keeps_multiple_in_range() {
        let source = "// l1\n// l2\n// l3\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[(1, 2)]));
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|x| x.location.line <= 2));
    }

    #[test]
    fn find_violations_with_empty_filter_lints_nothing() {
        let source = "// foo\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[]));
        assert!(v.is_empty());
    }

    #[test]
    fn find_violations_with_no_filter_lints_all() {
        let source = "// l1\n// l2\nfn main() {}\n";
        let v = find_violations("test.rs", source, None);
        assert_eq!(v.len(), 2);
    }

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
