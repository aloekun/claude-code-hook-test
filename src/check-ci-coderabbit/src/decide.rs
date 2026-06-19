//! CI / CodeRabbit 状態から `(status, action)` を判定するロジックと人間向け summary。

use crate::models::{CiStatus, CodeRabbitStatus};

/// CI と CodeRabbit の状態から `(status, action)` を決定する。
///
/// 判定優先順位 (上から):
///   1. CI failure → error / stop_monitoring_failure
///   2. walkthrough_clean かつ unresolved_threads 無し → complete / stop_monitoring_success
///   3. review_state == not_found かつ has_actionable → action_required
///   4. CI pending or CR pending → continue_monitoring
///   5. review_state failure/error → stop_monitoring_failure
///   6. has_actionable → action_required
///   7. それ以外 → complete / stop_monitoring_success
pub(crate) fn decide(ci: &CiStatus, cr: &CodeRabbitStatus) -> (String, String) {
    if ci.overall == "failure" {
        return ("error".to_string(), "stop_monitoring_failure".to_string());
    }
    let has_unresolved = cr.unresolved_threads.map(|n| n > 0).unwrap_or(false);
    let effective_new = if let Some(actionable) = cr.actionable_comments {
        std::cmp::max(cr.new_comments, actionable)
    } else {
        cr.new_comments
    };
    let has_actionable = effective_new > 0 || has_unresolved;
    if cr.walkthrough_clean && !has_unresolved {
        return (
            "complete".to_string(),
            "stop_monitoring_success".to_string(),
        );
    }
    if cr.review_state == "not_found" && has_actionable {
        return ("action_required".to_string(), "action_required".to_string());
    }
    let ci_pending = ci.overall == "pending" && !ci.runs.is_empty();
    let cr_pending = cr.review_state == "pending" || cr.review_state == "not_found";
    if ci_pending || cr_pending {
        return ("pending".to_string(), "continue_monitoring".to_string());
    }
    if cr.review_state == "failure" || cr.review_state == "error" {
        return ("error".to_string(), "stop_monitoring_failure".to_string());
    }
    if has_actionable {
        return ("action_required".to_string(), "action_required".to_string());
    }
    (
        "complete".to_string(),
        "stop_monitoring_success".to_string(),
    )
}

/// CI / CodeRabbit 状態を人間向け日本語サマリー文字列で返す。
pub(crate) fn build_summary(ci: &CiStatus, cr: &CodeRabbitStatus) -> String {
    let ci_part = build_summary_ci_part(ci);
    let cr_part = build_summary_cr_part(cr);
    format!("{}。{}", ci_part, cr_part)
}

fn build_summary_ci_part(ci: &CiStatus) -> String {
    match ci.overall.as_str() {
        "success" => "CI成功".to_string(),
        "failure" => {
            let failed: Vec<&str> = ci
                .runs
                .iter()
                .filter(|r| r.conclusion == "failure")
                .map(|r| r.name.as_str())
                .collect();
            format!("CI失敗 ({})", failed.join(", "))
        }
        _ => "CI実行中".to_string(),
    }
}

fn build_summary_cr_part(cr: &CodeRabbitStatus) -> String {
    match cr.review_state.as_str() {
        "success" => build_summary_cr_part_with_counts(cr, false),
        "pending" => "CodeRabbitレビュー待ち".to_string(),
        "not_found" => build_summary_cr_part_with_counts(cr, true),
        _ => format!("CodeRabbit状態: {}", cr.review_state),
    }
}

fn build_summary_cr_part_with_counts(cr: &CodeRabbitStatus, not_found: bool) -> String {
    let mut parts = vec![];
    let effective = if not_found {
        cr.actionable_comments.unwrap_or(cr.new_comments)
    } else {
        cr.actionable_comments
            .map(|a| std::cmp::max(a, cr.new_comments))
            .unwrap_or(cr.new_comments)
    };
    if effective > 0 {
        let label = if not_found {
            "新規コメント"
        } else {
            "新規指摘"
        };
        parts.push(format!("{}{}件", label, effective));
    }
    if let Some(n) = cr.unresolved_threads {
        if n > 0 {
            parts.push(format!("未解決スレッド{}件", n));
        }
    }
    if parts.is_empty() {
        if not_found {
            "CodeRabbitレビュー待ち".to_string()
        } else {
            "CodeRabbit指摘なし".to_string()
        }
    } else {
        format!("CodeRabbit: {}", parts.join("、"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CiRunSummary;

    #[test]
    fn decide_ci_pending() {
        let ci = CiStatus {
            overall: "pending".to_string(),
            runs: vec![CiRunSummary {
                name: "build".to_string(),
                conclusion: "".to_string(),
            }],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_cr_pending() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "pending".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_cr_not_found() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_ci_failure() {
        let ci = CiStatus {
            overall: "failure".to_string(),
            runs: vec![CiRunSummary {
                name: "test".to_string(),
                conclusion: "failure".to_string(),
            }],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "error");
        assert_eq!(action, "stop_monitoring_failure");
    }

    #[test]
    fn decide_new_comments() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 2,
            actionable_comments: None,
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_unresolved_threads() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(3),
            walkthrough_clean: false,
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_actionable_overrides_new_comments() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(3),
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_all_clean() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    #[test]
    fn decide_cr_failure() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "failure".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "error");
        assert_eq!(action, "stop_monitoring_failure");
    }

    #[test]
    fn decide_cr_not_found_with_comments() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            new_comments: 0,
            actionable_comments: Some(3),
            unresolved_threads: Some(3),
            walkthrough_clean: false,
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_no_ci_cr_success() {
        let ci = CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    #[test]
    fn decide_no_ci_cr_not_found_no_comments() {
        let ci = CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_walkthrough_clean_returns_complete_when_no_unresolved_threads() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            new_comments: 1,
            walkthrough_clean: true,
            unresolved_threads: Some(0),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    #[test]
    fn decide_walkthrough_clean_does_not_override_unresolved_threads() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            new_comments: 1,
            walkthrough_clean: true,
            unresolved_threads: Some(2),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn summary_all_clean() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };
        let summary = build_summary(&ci, &cr);
        assert!(summary.contains("CI成功"));
        assert!(summary.contains("指摘なし"));
    }

    #[test]
    fn summary_ci_failure() {
        let ci = CiStatus {
            overall: "failure".to_string(),
            runs: vec![CiRunSummary {
                name: "test".to_string(),
                conclusion: "failure".to_string(),
            }],
        };
        let cr = CodeRabbitStatus::default();
        let summary = build_summary(&ci, &cr);
        assert!(summary.contains("CI失敗"));
        assert!(summary.contains("test"));
    }

    #[test]
    fn summary_with_comments_and_threads() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 2,
            actionable_comments: Some(3),
            unresolved_threads: Some(1),
            walkthrough_clean: false,
        };
        let summary = build_summary(&ci, &cr);
        assert!(summary.contains("新規指摘3件"));
        assert!(summary.contains("未解決スレッド1件"));
    }
}
