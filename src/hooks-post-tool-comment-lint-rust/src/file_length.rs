//! ファイル長スケーリング検出 (順位 147 / Bundle "既存ルール仕組み化"):
//! ファイル長 > 800 行を error として feedback。
//!
//! touch-trigger ratchet (既存超過ファイルは触られるまで grandfather)。
//! soft-feedback 性質: `additionalContext` を返すだけで Edit 自体は block しないため、
//! override env は持たない (block されない nag に override は無意味)。

use crate::violations::{LintViolation, ViolationExample, ViolationFix, ViolationLocation};

/// 順位 147 (Bundle "既存ルール仕組み化"): ファイル長検出の閾値。
/// `~/.claude/rules/common/coding-style.md` § File Organization の 800 行 max
/// ガイドラインを決定論的に維持する (touch-trigger ratchet)。
pub(crate) const MAX_FILE_LINES: usize = 800;

/// 順位 147 (Bundle "既存ルール仕組み化"): ファイル長 > MAX_FILE_LINES (800) のファイルを検出する
/// touch-trigger ratchet 方式。
///
/// 関数長 (順位 48) と異なり、ファイル全体は常に「触られている」ため per-function の
/// overlap 検査は不要。`line_filter` の `Some(empty)` (= Edit 純削除) のみ skip し、
/// それ以外の任意の編集 (Edit / Write / MultiEdit / 不明) で file > 800 行ならば 1 件 flag。
pub(crate) fn find_file_length_violations(
    file_path: &str,
    source: &str,
    line_filter: Option<&[(usize, usize)]>,
) -> Vec<LintViolation> {
    if matches!(line_filter, Some([])) {
        return Vec::new();
    }
    let line_count = count_source_lines(source);
    if line_count <= MAX_FILE_LINES {
        return Vec::new();
    }
    vec![file_too_long_violation(file_path, line_count)]
}

pub(crate) fn count_source_lines(source: &str) -> usize {
    if source.is_empty() {
        return 0;
    }
    let newline_count = source.bytes().filter(|b| *b == b'\n').count();
    if source.ends_with('\n') {
        newline_count
    } else {
        newline_count + 1
    }
}

fn file_too_long_violation(file_path: &str, line_count: usize) -> LintViolation {
    LintViolation {
        r#type: "RUST_FILE_TOO_LONG".to_string(),
        severity: "error".to_string(),
        location: ViolationLocation {
            file: file_path.to_string(),
            line: 1,
            symbol: format!("file ({} lines)", line_count),
        },
        message: format!(
            "ファイル長 {} 行 > 上限 {} 行 (順位 147 / coding-style.md File Organization 800 行ガイドライン)",
            line_count, MAX_FILE_LINES
        ),
        why: "ファイル長が肥大化すると責務が混在し、レビュー / refactor / 認知負荷が劣化する。\
              CLAUDE.md coding-style.md の 800 行 max ガイドラインを決定論的に維持する \
              (touch-trigger ratchet: 既存超過ファイルは触られるまで grandfather)。"
            .to_string(),
        fix: ViolationFix {
            strategy: "ファイルを責務ごとに分割し、複数 module に切り出す".to_string(),
            steps: vec![
                "ファイルの責務を 2-3 つに分解する (どこから別ファイルにできるか特定)".to_string(),
                "domain / layer ごとに sub-module (`foo.rs` → `foo/mod.rs` + `foo/bar.rs`) を作成".to_string(),
                "test mod は通常そのままで OK だが、production 側分割に追随できるなら test も分割".to_string(),
                "1 ファイル 200-400 行が目安、800 行を超えるなら必ず分割を検討".to_string(),
            ],
        },
        example: ViolationExample {
            bad: "// src/big_module.rs (1000 行)\nfn parse() { ... }\nfn validate() { ... }\nfn render() { ... }".to_string(),
            good: "// src/big_module/mod.rs\nmod parse; mod validate; mod render;\n// src/big_module/parse.rs (300 行)\n// src/big_module/validate.rs (300 行)\n// src/big_module/render.rs (300 行)".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source_with_lines(line_count: usize) -> String {
        let mut s = String::with_capacity(line_count * 16);
        for i in 0..line_count {
            s.push_str(&format!("let _x{} = {};\n", i, i));
        }
        s
    }

    #[test]
    fn count_source_lines_empty() {
        assert_eq!(count_source_lines(""), 0);
    }

    #[test]
    fn count_source_lines_single_line_no_newline() {
        assert_eq!(count_source_lines("foo"), 1);
    }

    #[test]
    fn count_source_lines_single_line_with_newline() {
        assert_eq!(count_source_lines("foo\n"), 1);
    }

    #[test]
    fn count_source_lines_multiple_lines_trailing_newline() {
        assert_eq!(count_source_lines("a\nb\nc\n"), 3);
    }

    #[test]
    fn count_source_lines_multiple_lines_no_trailing_newline() {
        assert_eq!(count_source_lines("a\nb\nc"), 3);
    }

    #[test]
    fn file_length_under_threshold_no_violation() {
        let source = make_source_with_lines(MAX_FILE_LINES - 10);
        let v = find_file_length_violations("test.rs", &source, None);
        assert!(v.is_empty());
    }

    #[test]
    fn file_length_at_threshold_no_violation() {
        let source = make_source_with_lines(MAX_FILE_LINES);
        let v = find_file_length_violations("test.rs", &source, None);
        assert!(v.is_empty(), "exactly MAX_FILE_LINES should not violate");
    }

    #[test]
    fn file_length_over_threshold_violates_with_no_filter() {
        let source = make_source_with_lines(MAX_FILE_LINES + 50);
        let v = find_file_length_violations("test.rs", &source, None);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].r#type, "RUST_FILE_TOO_LONG");
        assert!(v[0]
            .location
            .symbol
            .contains(&format!("{} lines", MAX_FILE_LINES + 50)));
    }

    #[test]
    fn file_length_flagged_when_filter_non_empty() {
        let source = make_source_with_lines(MAX_FILE_LINES + 50);
        let v = find_file_length_violations("test.rs", &source, Some(&[(1, 1)]));
        assert_eq!(
            v.len(),
            1,
            "any non-empty edit to an over-threshold file should flag (touch-trigger ratchet)"
        );
    }

    #[test]
    fn file_length_flagged_even_when_filter_far_from_threshold_boundary() {
        let source = make_source_with_lines(MAX_FILE_LINES + 50);
        let v = find_file_length_violations("test.rs", &source, Some(&[(500, 500)]));
        assert_eq!(
            v.len(),
            1,
            "file length flag is whole-file scope, unlike function length per-overlap"
        );
    }

    #[test]
    fn file_length_skip_lint_when_filter_empty() {
        let source = make_source_with_lines(MAX_FILE_LINES + 50);
        let v = find_file_length_violations("test.rs", &source, Some(&[]));
        assert!(
            v.is_empty(),
            "empty filter (= pure deletion) should skip linting"
        );
    }

    #[test]
    fn file_length_under_threshold_skip_even_with_filter() {
        let source = make_source_with_lines(MAX_FILE_LINES - 5);
        let v = find_file_length_violations("test.rs", &source, Some(&[(1, 1)]));
        assert!(v.is_empty(), "under-threshold file should never flag");
    }

    #[test]
    fn file_length_violation_json_has_required_fields() {
        let source = make_source_with_lines(MAX_FILE_LINES + 50);
        let v = find_file_length_violations("test.rs", &source, None);
        let json = serde_json::to_string(&v[0]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "RUST_FILE_TOO_LONG");
        assert_eq!(parsed["severity"], "error");
        assert!(parsed["message"]
            .as_str()
            .unwrap()
            .contains(&MAX_FILE_LINES.to_string()));
        assert_eq!(parsed["location"]["line"], 1);
    }
}
