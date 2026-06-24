//! コメント存在制約 (Bundle Z #B-α): 非 doc コメントを禁止、例外マーカーのみ許可。
//!
//! 適用される lint:
//! - 非 doc コメントを構造的に禁止 (Why/What 区別は試みない)
//! - 例外マーカー (`/// `, `//! `, `// TODO:` 等) のみ許可
//!
//! `is_allowed_comment` は `count_non_doc_comments` (metrics) からも参照される。

use tree_sitter::{Parser, Query, QueryCursor};

use crate::line_filter::span_overlaps_ranges;
use crate::violations::{
    LintViolation, ViolationExample, ViolationFix, ViolationLocation, MAX_VIOLATIONS,
};

/// 例外マーカー (line_comment): 行頭スペース除去後にこれらのいずれかで始まれば許可
pub(crate) const ALLOWED_LINE_PREFIXES: &[&str] = &[
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
pub(crate) const ALLOWED_BLOCK_PREFIXES: &[&str] = &["/**", "/*!"];

pub(crate) fn is_allowed_comment(comment_text: &str) -> bool {
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

pub(crate) fn find_violations(
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
    let query = match Query::new(&language, "[(line_comment) (block_comment)] @comment") {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };
    collect_comment_violations(file_path, source, &tree, &query, line_filter)
}

fn collect_comment_violations(
    file_path: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    query: &Query,
    line_filter: Option<&[(usize, usize)]>,
) -> Vec<LintViolation> {
    let mut cursor = QueryCursor::new();
    let mut violations = Vec::new();
    let source_bytes = source.as_bytes();
    let matches = cursor.matches(query, tree.root_node(), source_bytes);
    'outer: for m in matches {
        for capture in m.captures {
            if let Some(v) =
                build_violation_for_node(capture.node, source_bytes, file_path, line_filter)
            {
                violations.push(v);
                if violations.len() >= MAX_VIOLATIONS {
                    break 'outer;
                }
            }
        }
    }
    violations
}

fn build_violation_for_node(
    node: tree_sitter::Node,
    source_bytes: &[u8],
    file_path: &str,
    line_filter: Option<&[(usize, usize)]>,
) -> Option<LintViolation> {
    let comment_text = node.utf8_text(source_bytes).ok()?;
    if is_allowed_comment(comment_text) {
        return None;
    }
    let start = node.start_position();
    let line = start.row + 1;
    if let Some(ranges) = line_filter {
        if !span_overlaps_ranges(line, node.end_position().row + 1, ranges) {
            return None;
        }
    }
    let snippet = comment_text
        .lines()
        .next()
        .unwrap_or(comment_text)
        .to_string();
    Some(comment_forbidden_violation(file_path, line, snippet))
}

fn comment_forbidden_violation(file_path: &str, line: usize, snippet: String) -> LintViolation {
    LintViolation {
        r#type: "RUST_COMMENT_FORBIDDEN".to_string(),
        severity: "error".to_string(),
        location: ViolationLocation {
            file: file_path.to_string(),
            line,
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
                "Why コメントが本当に必要なら // SAFETY: / // NOTE: 等のマーカー付きで書き直す"
                    .to_string(),
            ],
        },
        example: ViolationExample {
            bad: "// 成功時のみ更新する\nstate.value = Some(x);".to_string(),
            good: "if let Ok(updated) = result { state.value = Some(updated); }".to_string(),
        },
    }
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
    fn find_violations_multiline_block_comment_spanning_range_boundary() {
        let source = "/* line1\n   line2\n   line3 */\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[(3, 4)]));
        assert_eq!(
            v.len(),
            1,
            "block comment starting at line 1 but extending into range should be detected"
        );
    }

    #[test]
    fn find_violations_multiline_block_comment_range_covers_start_line_only() {
        let source = "/* line1\n   line2\n   line3 */\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[(1, 1)]));
        assert_eq!(
            v.len(),
            1,
            "range covering only the start line of a multiline block comment should detect"
        );
    }

    #[test]
    fn find_violations_multiline_block_comment_range_covers_end_line_only() {
        let source = "/* line1\n   line2\n   line3 */\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[(3, 3)]));
        assert_eq!(
            v.len(),
            1,
            "range covering only the end line of a multiline block comment should detect"
        );
    }

    #[test]
    fn find_violations_multiline_block_comment_range_covers_middle_line_only() {
        let source = "/* line1\n   line2\n   line3 */\nfn main() {}\n";
        let v = find_violations("test.rs", source, Some(&[(2, 2)]));
        assert_eq!(
            v.len(),
            1,
            "range covering only an internal line of a multiline block comment should detect"
        );
    }

    #[test]
    fn find_violations_inline_block_comment_range_exact_match() {
        let source =
            "fn foo() {}\nfn bar() {}\nfn baz() {}\nfn qux() {}\n/* inline */\nfn end() {}\n";
        let v = find_violations("test.rs", source, Some(&[(5, 5)]));
        assert_eq!(
            v.len(),
            1,
            "single-line range exactly on inline block comment line should detect"
        );
    }

    #[test]
    fn find_violations_inline_block_comment_range_starts_at_comment_line() {
        let source =
            "fn foo() {}\nfn bar() {}\nfn baz() {}\nfn qux() {}\n/* inline */\nfn end() {}\n";
        let v = find_violations("test.rs", source, Some(&[(5, 7)]));
        assert_eq!(
            v.len(),
            1,
            "range whose start equals inline block comment line should detect"
        );
    }

    #[test]
    fn find_violations_inline_block_comment_range_ends_at_comment_line() {
        let source =
            "fn foo() {}\nfn bar() {}\nfn baz() {}\nfn qux() {}\n/* inline */\nfn end() {}\n";
        let v = find_violations("test.rs", source, Some(&[(3, 5)]));
        assert_eq!(
            v.len(),
            1,
            "range whose end equals inline block comment line should detect"
        );
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
}
