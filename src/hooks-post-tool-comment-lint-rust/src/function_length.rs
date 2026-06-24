//! 関数長スケーリング検出 (順位 48 / PR #101 T1-4): 関数長 > 50 行を error として block。
//!
//! touch-trigger ratchet 方式 (既存超過関数は変更行に触れた瞬間にだけ flag、grandfather)。

use crate::line_filter::span_overlaps_ranges;
use crate::metrics::{compute_metrics, FunctionMetric};
use crate::violations::{
    LintViolation, ViolationExample, ViolationFix, ViolationLocation, MAX_VIOLATIONS,
};

/// 順位 48 (PR #101 T1-4): 関数長スケーリング検出の閾値。CLAUDE.md `coding-style.md`
/// の 50 行ガイドラインと同期。
pub(crate) const MAX_FUNCTION_LINES: usize = 50;

/// 順位 48 (PR #101 T1-4): 関数長 > MAX_FUNCTION_LINES (50) の関数を検出する
/// touch-trigger ratchet 方式。
///
/// `line_filter` が `Some(ranges)` の場合、関数 body が ranges と重なる関数のみ flag
/// (= 既存 50+ 行関数は触られるまで grandfather)。`None` の場合 (Write / MultiEdit /
/// 不明 tool) は全関数を flag。`Some(empty)` の場合は lint をスキップ (Edit 純削除)。
pub(crate) fn find_function_length_violations(
    file_path: &str,
    source: &str,
    line_filter: Option<&[(usize, usize)]>,
) -> Vec<LintViolation> {
    if matches!(line_filter, Some([])) {
        return Vec::new();
    }
    let metrics = compute_metrics(source);
    let mut violations = Vec::new();
    for f in &metrics.functions {
        if f.length <= MAX_FUNCTION_LINES {
            continue;
        }
        if let Some(ranges) = line_filter {
            if !span_overlaps_ranges(f.body_line_start, f.body_line_end, ranges) {
                continue;
            }
        }
        violations.push(function_too_long_violation(file_path, f));
        if violations.len() >= MAX_VIOLATIONS {
            break;
        }
    }
    violations
}

fn function_too_long_violation(file_path: &str, f: &FunctionMetric) -> LintViolation {
    LintViolation {
        r#type: "RUST_FUNCTION_TOO_LONG".to_string(),
        severity: "error".to_string(),
        location: ViolationLocation {
            file: file_path.to_string(),
            line: f.line_start,
            symbol: format!("fn {} ({} lines)", f.name, f.length),
        },
        message: format!(
            "関数長 {} 行 > 上限 {} 行 (順位 48 / CLAUDE.md coding-style 50 行ガイドライン)",
            f.length, MAX_FUNCTION_LINES
        ),
        why: "関数長が肥大化すると責務が混在し、レビュー / refactor / テスト独立性が劣化する。\
              CLAUDE.md coding-style.md の 50 行ガイドラインを決定論的に維持する \
              (touch-trigger ratchet: 既存超過関数は触られるまで grandfather)。"
            .to_string(),
        fix: ViolationFix {
            strategy: "関数を分割し、責務ごとに名前を与える".to_string(),
            steps: vec![
                "関数の責務を 2-3 つに分解する (どこから別関数にできるか特定)".to_string(),
                "early return / `let ... else` / guard clause で nesting を平らにする".to_string(),
                "match arm の body が長ければ helper 関数に切り出す".to_string(),
                "それでも 50 行を超える場合は struct + impl 分割を検討".to_string(),
            ],
        },
        example: ViolationExample {
            bad: "fn process(input: &str) -> Result<Output> {\n    // 60 行のロジック\n}".to_string(),
            good: "fn process(input: &str) -> Result<Output> {\n    let parsed = parse(input)?;\n    let validated = validate(&parsed)?;\n    finalize(validated)\n}".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_function_with_lines(name: &str, body_lines: usize) -> String {
        let mut s = format!("fn {}() {{\n", name);
        for i in 0..body_lines {
            s.push_str(&format!("    let _x_{} = {};\n", i, i));
        }
        s.push_str("}\n");
        s
    }

    #[test]
    fn function_length_under_threshold_no_violation() {
        let source = make_function_with_lines("small", 30);
        let v = find_function_length_violations("test.rs", &source, None);
        assert!(v.is_empty());
    }

    #[test]
    fn function_length_at_threshold_no_violation() {
        let source = make_function_with_lines("boundary", MAX_FUNCTION_LINES - 2);
        let v = find_function_length_violations("test.rs", &source, None);
        assert!(v.is_empty(), "function length == 50 should not violate");
    }

    #[test]
    fn function_length_over_threshold_violates_with_no_filter() {
        let source = make_function_with_lines("big", 100);
        let v = find_function_length_violations("test.rs", &source, None);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].r#type, "RUST_FUNCTION_TOO_LONG");
        assert!(v[0].location.symbol.contains("big"));
    }

    #[test]
    fn function_length_grandfathered_when_not_in_filter() {
        let source = make_function_with_lines("big", 100);
        let v = find_function_length_violations("test.rs", &source, Some(&[(500, 600)]));
        assert!(
            v.is_empty(),
            "function not touched by changed lines should be grandfathered"
        );
    }

    #[test]
    fn function_length_flagged_when_changed_line_in_body() {
        let source = make_function_with_lines("big", 100);
        let v = find_function_length_violations("test.rs", &source, Some(&[(50, 50)]));
        assert_eq!(v.len(), 1, "function touched by changed line should flag");
    }

    #[test]
    fn function_length_flagged_when_changed_line_overlaps_function_start() {
        let source = make_function_with_lines("big", 100);
        let v = find_function_length_violations("test.rs", &source, Some(&[(1, 1)]));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn function_length_skip_lint_when_filter_empty() {
        let source = make_function_with_lines("big", 100);
        let v = find_function_length_violations("test.rs", &source, Some(&[]));
        assert!(
            v.is_empty(),
            "empty filter (= pure deletion) should skip linting"
        );
    }

    #[test]
    fn function_length_only_long_function_flagged_when_multiple() {
        let mut source = make_function_with_lines("small", 10);
        source.push_str(&make_function_with_lines("big", 80));
        let v = find_function_length_violations("test.rs", &source, None);
        assert_eq!(v.len(), 1);
        assert!(v[0].location.symbol.contains("big"));
    }

    #[test]
    fn function_length_max_violations_capped() {
        let mut source = String::new();
        for i in 0..30 {
            source.push_str(&make_function_with_lines(&format!("big_{}", i), 80));
        }
        let v = find_function_length_violations("test.rs", &source, None);
        assert_eq!(v.len(), MAX_VIOLATIONS);
    }

    #[test]
    fn function_length_violation_json_has_required_fields() {
        let source = make_function_with_lines("big", 80);
        let v = find_function_length_violations("test.rs", &source, None);
        let json = serde_json::to_string(&v[0]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "RUST_FUNCTION_TOO_LONG");
        assert_eq!(parsed["severity"], "error");
        assert!(parsed["message"]
            .as_str()
            .unwrap()
            .contains(&MAX_FUNCTION_LINES.to_string()));
    }
}
