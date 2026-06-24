//! File-level metrics for `--metrics` mode (Bundle Z Phase 2 / #B-β).
//!
//! 出力 JSON は `scripts/fix-metrics-check.ps1` が pre/post 比較に使用する。
//! `non_doc_comment_count` は `find_violations` と同じ判定ロジックで計上 (例外マーカー除外)。

use serde::Serialize;
use tree_sitter::{Node, Parser, Query, QueryCursor, Tree};

use crate::comment_lint::is_allowed_comment;

#[derive(Serialize, Debug, PartialEq, Eq, Default)]
pub(crate) struct FileMetrics {
    pub(crate) non_doc_comment_count: usize,
    pub(crate) functions: Vec<FunctionMetric>,
}

/// 関数単位のメトリクス。pre/post 比較は `name` で関数を突き合わせる (識別子変更時は新規扱い)。
#[derive(Serialize, Debug, PartialEq, Eq)]
pub(crate) struct FunctionMetric {
    pub(crate) name: String,
    pub(crate) line_start: usize,
    pub(crate) line_end: usize,
    pub(crate) body_line_start: usize,
    pub(crate) body_line_end: usize,
    pub(crate) length: usize,
    pub(crate) max_nesting_depth: usize,
}

pub(crate) fn compute_metrics(source: &str) -> FileMetrics {
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
    let line_start = node.start_position().row + 1;
    let line_end = node.end_position().row + 1;
    let length = line_end - line_start + 1;
    let (body_line_start, body_line_end, max_nesting_depth) = node
        .child_by_field_name("body")
        .map_or((line_start, line_end, 0), |b| {
            (
                b.start_position().row + 1,
                b.end_position().row + 1,
                max_block_depth_inside_body(b),
            )
        });
    Some(FunctionMetric {
        name,
        line_start,
        line_end,
        body_line_start,
        body_line_end,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
