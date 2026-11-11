//! Edit/Write の line range 解釈、touch-trigger ratchet、`is_rust_file`、
//! `extract_file_path`、`collect_all_violations` を提供する dispatch 層。
//!
//! 各 lint group (`comment_lint` / `function_length` / `file_length`) を統合し、
//! `line_filter` を共通シグネチャで渡す。

use serde::Deserialize;
use std::path::Path;

use crate::comment_lint::find_violations;
use crate::file_length::find_file_length_violations;
use crate::function_length::find_function_length_violations;
use crate::violations::{LintViolation, MAX_VIOLATIONS};

#[derive(Deserialize, Default)]
pub(crate) struct ToolInput {
    pub(crate) file_path: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) old_string: Option<String>,
    pub(crate) new_string: Option<String>,
}

/// 順位 50 (PR #102 T1-1): Edit が触れた行のみ lint 対象にするため、`new_string` の出現位置から
/// 変更行範囲を導出する。
///
/// 戻り値:
/// - `None`: フィルタなし (= ファイル全体を lint)。Write / MultiEdit / 不明 tool / `new_string` が
///   見つからない場合 (line ending 差異等) のフォールバック。
/// - `Some(ranges)`: 1-indexed inclusive 範囲のみ lint。
/// - `Some(empty)`: lint をスキップ (Edit で `new_string` が空 = 純削除、または中身の変わった行が無い)。
///
/// `old_string` があれば、`new_string` のうち `old_string` と前後で一致する行を範囲から外す
/// ([`changed_line_span`])。Edit の文字列は照合のために変えていない行を前後に含むので、
/// 外さないと既存のコメントや既存の長い関数を「触った」と判定する (#631 / #632 の作業で 3 回観測)。
pub(crate) fn compute_changed_lines(
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
                return None;
            }
            match tool_input.old_string.as_deref() {
                Some(old) => Some(narrow_to_changed_lines(&ranges, old, new_string)),
                None => Some(ranges),
            }
        }
        _ => None,
    }
}

/// `new_string` の出現範囲 (ファイルの行番号) を、中身の変わった行だけに縮める。
fn narrow_to_changed_lines(ranges: &[(usize, usize)], old: &str, new: &str) -> Vec<(usize, usize)> {
    let Some((first, end)) = changed_line_span(old, new) else {
        return Vec::new();
    };
    ranges
        .iter()
        .map(|&(start, last)| (start + first, (start + end - 1).min(last)))
        .filter(|(s, e)| s <= e)
        .collect()
}

/// `new_string` を行に分けたとき、`old_string` と前後で一致する行を除いた部分を
/// `[first, end)` (0 始まり) で返す。変わった行が無ければ (行の削除だけなど) `None`。
///
/// 後ろからの一致は、前からの一致と重ならない数までに抑える。`old` が `"a\nb"`、`new` が
/// `"a\nb\nb"` のとき、重なりを許すと増えた行を一致と数えて取りこぼす。
pub(crate) fn changed_line_span(old: &str, new: &str) -> Option<(usize, usize)> {
    let old_lines: Vec<&str> = old.split('\n').collect();
    let new_lines: Vec<&str> = new.split('\n').collect();
    let prefix = old_lines
        .iter()
        .zip(&new_lines)
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old_lines.len().min(new_lines.len()) - prefix;
    let suffix = old_lines
        .iter()
        .rev()
        .zip(new_lines.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let end = new_lines.len() - suffix;
    (prefix < end).then_some((prefix, end))
}

pub(crate) fn locate_string_line_ranges(source: &str, needle: &str) -> Vec<(usize, usize)> {
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

pub(crate) fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    let clamped = offset.min(source.len());
    source.as_bytes()[..clamped]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

pub(crate) fn span_overlaps_ranges(start: usize, end: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|(s, e)| start <= *e && end >= *s)
}

pub(crate) fn is_rust_file(file_path: &str) -> bool {
    Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("rs"))
        .unwrap_or(false)
}

pub(crate) fn extract_file_path(tool_input: &ToolInput) -> String {
    tool_input
        .file_path
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| tool_input.path.clone())
        .unwrap_or_default()
}

pub(crate) fn collect_all_violations(
    file_path: &str,
    source: &str,
    line_filter: Option<&[(usize, usize)]>,
) -> Vec<LintViolation> {
    let mut violations = find_violations(file_path, source, line_filter);
    violations.extend(find_function_length_violations(
        file_path,
        source,
        line_filter,
    ));
    violations.extend(find_file_length_violations(file_path, source, line_filter));
    violations.truncate(MAX_VIOLATIONS);
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_length::MAX_FILE_LINES;
    use crate::function_length::MAX_FUNCTION_LINES;

    fn tool_input_with(new_string: Option<&str>) -> ToolInput {
        ToolInput {
            new_string: new_string.map(|s| s.to_string()),
            ..ToolInput::default()
        }
    }

    fn make_source_with_lines(line_count: usize) -> String {
        let mut s = String::with_capacity(line_count * 16);
        for i in 0..line_count {
            s.push_str(&format!("let _x{} = {};\n", i, i));
        }
        s
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
    fn locate_string_line_ranges_handles_mixed_ascii_and_kanji() {
        let source =
            "fn main() {\n    let msg = \"hello 世界\";\n    let bye = \"hello 世界\";\n}\n";
        let ranges = locate_string_line_ranges(source, "\"hello 世界\"");
        assert_eq!(ranges, vec![(2, 2), (3, 3)]);
    }

    #[test]
    fn locate_string_line_ranges_handles_kanji_only() {
        let source = "// 漢字のみのコメント\nfn foo() {}\n// 漢字のみのコメント\n";
        let ranges = locate_string_line_ranges(source, "漢字のみのコメント");
        assert_eq!(ranges, vec![(1, 1), (3, 3)]);
    }

    #[test]
    fn locate_string_line_ranges_handles_emoji() {
        let source = "fn foo() {\n    let r = \"🎉\";\n    let s = \"🎉\";\n}\n";
        let ranges = locate_string_line_ranges(source, "\"🎉\"");
        assert_eq!(ranges, vec![(2, 2), (3, 3)]);
    }

    #[test]
    fn locate_string_line_ranges_handles_supplementary_plane_char() {
        let source = "fn foo() {\n    let s = \"𝕊\";\n    let t = \"𝕊\";\n}\n";
        let ranges = locate_string_line_ranges(source, "\"𝕊\"");
        assert_eq!(ranges, vec![(2, 2), (3, 3)]);
    }

    #[test]
    fn locate_string_line_ranges_handles_combining_character() {
        let source = "fn foo() {\n    let s = \"e\u{0301}\";\n    let t = \"e\u{0301}\";\n}\n";
        let ranges = locate_string_line_ranges(source, "\"e\u{0301}\"");
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
    fn byte_offset_to_line_handles_mid_multibyte_offset() {
        let s = "漢字\nfn foo() {}\n";
        assert_eq!(byte_offset_to_line(s, 5), 1);
        assert_eq!(byte_offset_to_line(s, 6), 1);
        assert_eq!(byte_offset_to_line(s, 7), 2);
    }

    #[test]
    fn span_overlaps_ranges_single_line_inclusive_bounds() {
        let ranges = [(2, 4), (10, 10)];
        assert!(!span_overlaps_ranges(1, 1, &ranges));
        assert!(span_overlaps_ranges(2, 2, &ranges));
        assert!(span_overlaps_ranges(3, 3, &ranges));
        assert!(span_overlaps_ranges(4, 4, &ranges));
        assert!(!span_overlaps_ranges(5, 5, &ranges));
        assert!(span_overlaps_ranges(10, 10, &ranges));
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

    fn edit(old: &str, new: &str) -> ToolInput {
        ToolInput {
            old_string: Some(old.to_string()),
            new_string: Some(new.to_string()),
            ..ToolInput::default()
        }
    }

    /// 照合のために含めた既存のコメント行は、範囲に入らない (#631 で `// NOTE:` を付けて回避した形)。
    #[test]
    fn unchanged_context_lines_are_not_in_the_range() {
        let old = "    // existing\n    let y = 2;\n    let z = 3;";
        let new = "    // existing\n    let y = 20;\n    let z = 3;";
        let post = "fn foo() {\n    // existing\n    let y = 20;\n    let z = 3;\n}\n";
        assert_eq!(compute_changed_lines(Some("Edit"), &edit(old, new), post), Some(vec![(3, 3)]));
        let violations = collect_all_violations("a.rs", post, Some(&[(3, 3)]));
        assert!(violations.is_empty(), "既存のコメントを報告した ({} 件)", violations.len());
        assert!(
            !collect_all_violations("a.rs", post, Some(&[(2, 2)])).is_empty(),
            "対照: コメントの行が範囲に入れば報告される"
        );
    }

    /// 行を消しただけの Edit は、残った行を変えていないので lint しない。`new_string` が `}` だけの
    /// ときに、ファイル中のすべての `}` の行を範囲にして既存の長い関数を報告していた (#632)。
    #[test]
    fn a_deletion_that_keeps_only_context_lines_skips_lint() {
        let old = "\n    fn removed() {\n        let a = 1;\n    }\n}";
        let mut post = String::from("fn long() {\n");
        for i in 0..(MAX_FUNCTION_LINES + 5) {
            post.push_str(&format!("    let _x{i} = {i};\n"));
        }
        post.push_str("}\n");
        assert_eq!(compute_changed_lines(Some("Edit"), &edit(old, "}"), &post), Some(Vec::new()));
    }

    #[test]
    fn an_inserted_line_is_the_only_changed_line() {
        assert_eq!(changed_line_span("a\nc", "a\nb\nc"), Some((1, 2)));
        assert_eq!(changed_line_span("a\nb", "a\nb\nb"), Some((2, 3)));
        assert_eq!(changed_line_span("a\nb\nc", "a\nc"), None);
        assert_eq!(changed_line_span("x", "y"), Some((0, 1)));
    }

    /// `new_string` が改行で終わると、最後の分割片は次の行に当たる。出現範囲の外へ広げない。
    #[test]
    fn the_range_does_not_pass_the_end_of_the_occurrence() {
        let post = "fn foo() {}\nlet a = 1;\nlet b = 2;\nfn bar() {}\n";
        let t = edit("let a = 1;\n", "let a = 1;\nlet b = 2;\n");
        assert_eq!(compute_changed_lines(Some("Edit"), &t, post), Some(vec![(3, 3)]));
    }

    /// `replace_all` で複数箇所に出現するときは、出現ごとに同じ位置の行だけを範囲にする。
    #[test]
    fn each_occurrence_is_narrowed_the_same_way() {
        let post = "// keep\nlet x = 2;\nfn f() {}\n// keep\nlet x = 2;\n";
        let t = edit("// keep\nlet x = 1;", "// keep\nlet x = 2;");
        assert_eq!(compute_changed_lines(Some("Edit"), &t, post), Some(vec![(2, 2), (5, 5)]));
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

    /// 順位 57 (PR #105 T2-1 採用): `collect_all_violations` の `truncate(MAX_VIOLATIONS)`
    /// contract test。`find_violations` (comment-forbidden 系) と
    /// `find_function_length_violations` (function-too-long 系) の **両 source 混在** で
    /// 合計が MAX_VIOLATIONS を超えるとき、最終 vec が MAX_VIOLATIONS に cap されることを
    /// 機械強制する。
    #[test]
    fn collect_all_violations_truncates_combined_total_to_max() {
        let mut source = String::new();
        source.push_str("fn long_function() {\n");
        for i in 0..(MAX_FUNCTION_LINES + 5) {
            source.push_str(&format!("    let _x{} = {};\n", i, i));
        }
        source.push_str("}\n");
        for i in 0..(MAX_VIOLATIONS + 5) {
            source.push_str(&format!("// trailing comment {}\n", i));
        }

        let violations = collect_all_violations("test.rs", &source, None);

        assert_eq!(
            violations.len(),
            MAX_VIOLATIONS,
            "combined violations from both sources must be truncated to MAX_VIOLATIONS (= {})",
            MAX_VIOLATIONS
        );
    }

    #[test]
    fn file_length_violation_emitted_via_collect_all_violations() {
        let source = make_source_with_lines(MAX_FILE_LINES + 5);
        let violations = collect_all_violations("test.rs", &source, None);
        assert!(violations.iter().any(|v| v.r#type == "RUST_FILE_TOO_LONG"));
    }
}
