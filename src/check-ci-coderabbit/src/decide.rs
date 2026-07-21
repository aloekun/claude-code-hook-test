//! CI / CodeRabbit 状態から `(status, action)` を判定するロジックと人間向け summary。

use crate::models::{CiStatus, CodeRabbitStatus, RateLimitInfo};

/// CodeRabbit が**この push サイクルで実際にレビューを実施した**陽性証拠があるか。
///
/// `review_state` (commit status) を証拠に使ってはならない: CR はレート制限で
/// レビューを開始できなかった場合でも commit status を pass にする (2026-07-20、
/// PR #307/#309 で実観測)。そのため commit status だけを見ると「レビュー済み・
/// 指摘なし」と区別が付かず silent success に倒れる。
///
/// 証拠として採用するのは、いずれも `push_time` で絞り込まれた「今サイクルの
/// CR 出力そのもの」に限る:
/// - `walkthrough_clean`: CR が walkthrough を投稿し clean marker を出した
/// - `actionable_comments`: CR の review body から "Actionable comments posted: N"
///   を読めた (`Some(0)` も「レビューして 0 件だった」= 陽性証拠)
/// - `new_comments`: 今サイクルの CR コメントが存在する
///
/// `unresolved_threads` は `push_time` で絞られず過去サイクルの残骸を含み得るため
/// 証拠に採用しない (未解決スレッドの存在は「今回レビューが走った」ことを示さない)。
fn has_review_evidence(cr: &CodeRabbitStatus) -> bool {
    cr.walkthrough_clean || cr.actionable_comments.is_some() || cr.new_comments > 0
}

/// CI / CodeRabbit / rate-limit の状態から `(status, action)` を決定する。
///
/// 判定優先順位 (上から):
///   1. CI failure → error / stop_monitoring_failure
///   2. walkthrough_clean かつ unresolved_threads 無し → complete / stop_monitoring_success
///   3. **rate-limit 検出かつレビュー実施の陽性証拠なし → continue_monitoring** (R1)
///   4. review_state == not_found かつ has_actionable → action_required
///   5. CI pending or CR pending → continue_monitoring
///   6. review_state failure/error → stop_monitoring_failure
///   7. has_actionable → action_required
///   8. **レビュー実施の陽性証拠なし → continue_monitoring** (R4)
///   9. それ以外 → complete / stop_monitoring_success
///
/// 3 と 8 はどちらも「レビュー未実施を success/action_required と誤って確定しない」
/// ための gate だが、役割が違うので両方必要:
/// - 3 は rate-limit が判明しているケースを **7 (has_actionable) より先に**捕まえ、
///   監視を継続して呼び出し側の rate-limit branch (park / 再 trigger) に委ねる。
///   これが無いと、過去サイクル由来の未解決スレッドがあるだけで action_required に
///   抜け、レビューが走っていないのに監視が終了する。
/// - 8 は rate-limit marker 自体を CR が変えた場合の backstop。陽性証拠が無い限り
///   success を出さないので、marker 追随に失敗しても silent success には戻らない
///   (最悪 max_duration まで監視して timed_out = 安全側)。
pub(crate) fn decide(
    ci: &CiStatus,
    cr: &CodeRabbitStatus,
    rate_limit: Option<&RateLimitInfo>,
) -> (String, String) {
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
    if rate_limit.is_some() && !has_review_evidence(cr) {
        return ("pending".to_string(), "continue_monitoring".to_string());
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
    if !has_review_evidence(cr) {
        return ("pending".to_string(), "continue_monitoring".to_string());
    }
    (
        "complete".to_string(),
        "stop_monitoring_success".to_string(),
    )
}

/// CI / CodeRabbit 状態を人間向け日本語サマリー文字列で返す。
///
/// rate-limit 検出中でレビュー実施の陽性証拠が無い場合、CR 部分を
/// 「レート制限中」に差し替える。`review_state` は制限中でも pass になるため、
/// 差し替えないと「CodeRabbit指摘なし」= レビュー済みで問題なしと読める
/// 文言を出してしまう (PR #307/#309 実観測の silent success の一部)。
pub(crate) fn build_summary(
    ci: &CiStatus,
    cr: &CodeRabbitStatus,
    rate_limit: Option<&RateLimitInfo>,
) -> String {
    let ci_part = build_summary_ci_part(ci);
    let cr_part = if rate_limit.is_some() && !has_review_evidence(cr) {
        "CodeRabbitレート制限中 (レビュー未実施)".to_string()
    } else {
        build_summary_cr_part(cr)
    };
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

    /// PR #309 incident の CR 状態を再現する。
    ///
    /// 2026-07-20 の実観測: CR はレート制限でレビューを開始できなかったが
    /// commit status は pass (`review_state = "success"`)、今サイクルの CR 出力は
    /// 皆無 (`actionable_comments = None` / `new_comments = 0` / walkthrough なし)、
    /// 一方で過去サイクル由来の未解決スレッドが 2 件残っていた。
    fn pr309_incident_cr_status() -> CodeRabbitStatus {
        CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(2),
            walkthrough_clean: false,
        }
    }

    /// PR #309 incident の rate-limit 情報 (第 3 世代書式、57 分待機)。
    fn pr309_incident_rate_limit() -> RateLimitInfo {
        RateLimitInfo {
            until_unix_secs: 1_784_556_707,
            comment_event_time: "2026-07-20T12:10:47Z".to_string(),
            wait_minutes: 57,
            wait_seconds: 0,
            wait_time_parsed: true,
        }
    }

    fn ci_no_runs(overall: &str) -> CiStatus {
        CiStatus {
            overall: overall.to_string(),
            runs: vec![],
        }
    }

    /// R1 (incident 再現): rate-limit 検出中でレビュー実施の陽性証拠が無ければ、
    /// 未解決スレッドがあっても `action_required` で監視を終了せず継続する。
    ///
    /// この gate が無いと `has_actionable` 分岐に先に落ち、「レビューは走って
    /// いないのに未解決スレッドを理由に監視終了」= rate-limit branch (park /
    /// 再 trigger) に一度も到達しない。
    #[test]
    fn decide_rate_limited_without_review_evidence_continues_monitoring() {
        let ci = ci_no_runs("pending");
        let cr = pr309_incident_cr_status();
        let rl = pr309_incident_rate_limit();

        let (status, action) = decide(&ci, &cr, Some(&rl));

        assert_eq!(status, "pending");
        assert_eq!(
            action, "continue_monitoring",
            "rate-limit 中はレビュー未実施なので監視を継続し rate-limit branch に委ねる"
        );
    }

    /// R1 の gate が効いているのは **rate_limit の有無だけ**であることを対比で固定する。
    /// 同じ CR 状態でも rate_limit が無ければ従来どおり `action_required`。
    #[test]
    fn decide_same_cr_status_without_rate_limit_keeps_action_required() {
        let ci = ci_no_runs("pending");
        let cr = pr309_incident_cr_status();

        let (_, action) = decide(&ci, &cr, None);

        assert_eq!(
            action, "action_required",
            "rate_limit が無ければ未解決スレッドは通常どおり action_required"
        );
    }

    /// R1 が効きすぎないこと: walkthrough clean marker はレビュー完走の陽性証拠なので、
    /// 同サイクルに rate-limit comment が残っていても success を出す
    /// (残骸 rate-limit comment で監視が終わらなくなるのを防ぐ)。
    #[test]
    fn decide_rate_limited_but_walkthrough_clean_still_completes() {
        let ci = ci_no_runs("success");
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(0),
            walkthrough_clean: true,
        };
        let rl = pr309_incident_rate_limit();

        let (status, action) = decide(&ci, &cr, Some(&rl));

        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    /// R1 が効きすぎないこと 2: CR が実際にレビューして指摘を出していれば
    /// (= 陽性証拠あり)、rate-limit comment が残っていても action_required を出す。
    #[test]
    fn decide_rate_limited_with_actionable_evidence_reports_action_required() {
        let ci = ci_no_runs("success");
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(3),
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };
        let rl = pr309_incident_rate_limit();

        let (_, action) = decide(&ci, &cr, Some(&rl));

        assert_eq!(action, "action_required");
    }

    /// R4 backstop: rate-limit を **検出できなかった** 場合でも、レビュー実施の
    /// 陽性証拠が無い限り `stop_monitoring_success` を出さない。
    ///
    /// CR が marker 文言自体を変えて `parse_rate_limit` が沈黙しても、
    /// commit status の pass だけで success を確定しないことを固定する。
    #[test]
    fn decide_without_review_evidence_does_not_report_success() {
        let ci = ci_no_runs("success");
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };

        let (status, action) = decide(&ci, &cr, None);

        assert_eq!(status, "pending");
        assert_eq!(
            action, "continue_monitoring",
            "commit status の pass だけでは「レビュー済み・指摘なし」と確定できない"
        );
    }

    /// R4 の陽性証拠として `actionable_comments = Some(0)` が有効であること
    /// (「レビューして 0 件だった」= レビューは走った) を単独で固定する。
    #[test]
    fn decide_actionable_zero_counts_as_review_evidence() {
        let ci = ci_no_runs("success");
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
            walkthrough_clean: false,
        };

        let (status, action) = decide(&ci, &cr, None);

        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    /// R1: summary の CR 部分が rate-limit 中に「指摘なし」と断定しないこと。
    #[test]
    fn summary_rate_limited_does_not_claim_no_findings() {
        let ci = ci_no_runs("success");
        let cr = pr309_incident_cr_status();
        let rl = pr309_incident_rate_limit();

        let summary = build_summary(&ci, &cr, Some(&rl));

        assert!(
            summary.contains("レート制限中"),
            "rate-limit 中であることを summary に明示すべき: {}",
            summary
        );
        assert!(
            !summary.contains("指摘なし"),
            "レビュー未実施なのに「指摘なし」と断定してはならない: {}",
            summary
        );
    }

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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let (status, action) = decide(&ci, &cr, None);
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
        let summary = build_summary(&ci, &cr, None);
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
        let summary = build_summary(&ci, &cr, None);
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
        let summary = build_summary(&ci, &cr, None);
        assert!(summary.contains("新規指摘3件"));
        assert!(summary.contains("未解決スレッド1件"));
    }
}
