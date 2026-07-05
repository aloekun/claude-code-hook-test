//! CI / CodeRabbit state + comment JSON parsers (順位 209 / 順位 208 PR A refactor)。

use crate::markers::{is_clean_walkthrough_comment, is_rate_limit_comment};
use crate::models::{CiRunSummary, CiStatus, GhComment, GhReview, GhRunItem, GhStatusItem};

/// gh run list の JSON をパースして CI 状態を返す。
pub(crate) fn parse_ci_runs(json: &str) -> CiStatus {
    let items: Vec<GhRunItem> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] CI runs JSON パースエラー: {}", e);
        vec![]
    });

    if items.is_empty() {
        return CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
    }

    let runs: Vec<CiRunSummary> = items
        .iter()
        .map(|item| CiRunSummary {
            name: item.name.clone(),
            conclusion: item
                .conclusion
                .clone()
                .unwrap_or_else(|| "pending".to_string()),
        })
        .collect();

    CiStatus {
        overall: classify_ci_overall(&items).to_string(),
        runs,
    }
}

fn classify_ci_overall(items: &[GhRunItem]) -> &'static str {
    if items.iter().any(is_pending_run) {
        "pending"
    } else if items.iter().any(is_failure_run) {
        "failure"
    } else {
        "success"
    }
}

fn is_pending_run(i: &GhRunItem) -> bool {
    matches!(
        i.conclusion.as_deref(),
        None | Some("")
            | Some("pending")
            | Some("queued")
            | Some("in_progress")
            | Some("waiting")
    )
}

fn is_failure_run(i: &GhRunItem) -> bool {
    matches!(
        i.conclusion.as_deref(),
        Some("failure")
            | Some("cancelled")
            | Some("timed_out")
            | Some("action_required")
            | Some("stale")
    )
}

/// gh api .../statuses の JSON から CodeRabbit のレビュー状態を返す。
pub(crate) fn parse_coderabbit_status(json: &str) -> String {
    let items: Vec<GhStatusItem> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] statuses JSON パースエラー: {}", e);
        vec![]
    });

    let cr_statuses: Vec<&GhStatusItem> = items
        .iter()
        .filter(|s| {
            s.context
                .as_deref()
                .map(|c| c.contains("CodeRabbit"))
                .unwrap_or(false)
        })
        .collect();

    if cr_statuses.is_empty() {
        return "not_found".to_string();
    }

    cr_statuses
        .first()
        .and_then(|s| s.state.clone())
        .unwrap_or_else(|| "not_found".to_string())
}

/// PR コメントの JSON から push_time 以降の CodeRabbit 新規コメント数を返す。
///
/// 「review in progress」通知コメントと rate-limit コメントは除外する
/// (含めると `decide()` が action_required を早期 return し rate-limit retry 経路に入らない)。
pub(crate) fn parse_new_comments(json: &str, push_time: &str) -> usize {
    let comments: Vec<GhComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] comments JSON パースエラー: {}", e);
        vec![]
    });

    comments
        .iter()
        .filter(|c| is_kept_new_comment(c, push_time))
        .count()
}

fn is_kept_new_comment(c: &GhComment, push_time: &str) -> bool {
    let is_coderabbit = c
        .user
        .as_ref()
        .and_then(|u| u.login.as_deref())
        .map(|l| l == "coderabbitai[bot]")
        .unwrap_or(false);

    let after_push_time = c
        .created_at
        .as_deref()
        .map(|t| t >= push_time)
        .unwrap_or(false);

    let is_review_in_progress = c
        .body
        .as_deref()
        .map(|b| b.contains("review in progress"))
        .unwrap_or(false);

    let is_rate_limit = is_rate_limit_comment(c);

    is_coderabbit && after_push_time && !is_review_in_progress && !is_rate_limit
}

/// 順位 208: CR walkthrough comment body から clean marker を検出する。
///
/// formal Review object が無い (= `review_state == "not_found"`) 場合でも本シグナルが
/// true なら `decide()` で `(complete, stop_monitoring_success)` を返し、PR #210/#211 で
/// 発生した recheck loop を構造的に終了させる。
pub(crate) fn parse_walkthrough_clean_marker(json: &str, push_time: &str) -> bool {
    let comments: Vec<GhComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!(
            "[check-ci-coderabbit] walkthrough JSON パースエラー: {}",
            e
        );
        vec![]
    });

    comments
        .iter()
        .any(|c| is_clean_walkthrough_comment(c, push_time))
}

/// PR レビューの JSON から最新の CodeRabbit レビューの "Actionable comments posted: N" を抽出。
pub(crate) fn parse_actionable_comments(json: &str, push_time: &str) -> Option<usize> {
    let reviews: Vec<GhReview> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] reviews JSON パースエラー: {}", e);
        vec![]
    });

    let latest = reviews.iter().rfind(|r| {
        let is_coderabbit = r
            .user
            .as_ref()
            .and_then(|u| u.login.as_deref())
            .map(|l| l == "coderabbitai[bot]")
            .unwrap_or(false);

        let after_push_time = r
            .submitted_at
            .as_deref()
            .map(|t| t >= push_time)
            .unwrap_or(false);

        is_coderabbit && after_push_time
    })?;

    let body = latest.body.as_deref()?;
    extract_actionable_count(body)
}

/// 文字列から "Actionable comments posted: N" の N を抽出。
pub(crate) fn extract_actionable_count(body: &str) -> Option<usize> {
    let marker = "Actionable comments posted: ";
    let pos = body.find(marker)?;
    let rest = &body[pos + marker.len()..];
    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse::<usize>().ok()
}

/// GraphQL レスポンスから未解決スレッド数をパースする。
pub(crate) fn parse_unresolved_threads(json: &str) -> Option<usize> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")?
        .as_array()?;

    let unresolved = nodes
        .iter()
        .filter(|n| n.get("isResolved").and_then(|v| v.as_bool()) == Some(false))
        .count();

    Some(unresolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_all_success() {
        let json = r#"[
            {"name": "build", "conclusion": "success"},
            {"name": "test", "conclusion": "success"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "success");
        assert_eq!(ci.runs.len(), 2);
    }

    #[test]
    fn ci_one_failure() {
        let json = r#"[
            {"name": "build", "conclusion": "success"},
            {"name": "test", "conclusion": "failure"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "failure");
    }

    #[test]
    fn ci_pending_null_conclusion() {
        let json = r#"[
            {"name": "build", "conclusion": null},
            {"name": "test", "conclusion": "success"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "pending");
    }

    #[test]
    fn ci_pending_in_progress() {
        let json = r#"[
            {"name": "build", "conclusion": "in_progress"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "pending");
    }

    #[test]
    fn ci_empty_runs() {
        let json = "[]";
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "pending");
        assert!(ci.runs.is_empty());
    }

    #[test]
    fn ci_cancelled_is_failure() {
        let json = r#"[{"name": "deploy", "conclusion": "cancelled"}]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "failure");
    }

    #[test]
    fn cr_status_success() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "success"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "success");
    }

    #[test]
    fn cr_status_pending() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "pending"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "pending");
    }

    #[test]
    fn cr_status_not_found() {
        let json = r#"[
            {"context": "ci/build", "state": "success"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "not_found");
    }

    #[test]
    fn cr_status_empty() {
        assert_eq!(parse_coderabbit_status("[]"), "not_found");
    }

    #[test]
    fn cr_status_multiple_takes_first() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "success"},
            {"context": "CodeRabbit", "state": "pending"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "success");
    }

    /// 順位 213 (PR #213 post-merge-feedback T2-1 採用): GitHub commit statuses API は
    /// reverse-chronological 返却 (新しい→古い順) であり、`parse_coderabbit_status` は
    /// `.first()` で「最新 state」を取得する。この semantics を test 名 + doc comment +
    /// assert message で explicit に固定し、refactor 時 (例: `.first()` → `.last()`) に
    /// 意図しない意味変更を機械的に検出する。
    ///
    /// 由来: PR #213 takt-fix iter 2 で `.last()` (= 最古 state を選んでいた) → `.first()`
    /// の semantic fix が行われたが、元 production code (main.rs 時代) のコメントが
    /// 「最新エントリ」と書きながら `.last()` を使っていた drift 事例。test 表現で固定する。
    #[test]
    fn cr_status_reverse_chronological_picks_first() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "success"},
            {"context": "CodeRabbit", "state": "pending"}
        ]"#;
        assert_eq!(
            parse_coderabbit_status(json),
            "success",
            "GitHub statuses API は reverse-chronological 返却のため、配列先頭 (`.first()`) が最新 state。`.last()` (古い state) を返すと CR レビュー終了を見逃す"
        );
    }

    /// 順位258 harm #2 回帰: PR #247 head の実測 commit statuses (reverse-chronological,
    /// pending/success 混在の 6 件) を与え、`.first()` = 最新 "Review completed" success を
    /// 返すことを固定する。正しい head SHA さえ `fetch_coderabbit_commit_state` に渡れば
    /// この list から "success" を読み取り `decide()` が park ループを止められることを示す
    /// (harm #2 の真因は SHA 取得経路 `get_head_sha` であり、parse 側ではないことの根拠)。
    #[test]
    fn cr_status_pr247_real_shape_picks_latest_success() {
        let json = r#"[
            {"context": "CodeRabbit", "description": "Review completed", "state": "success"},
            {"context": "CodeRabbit", "description": "Review in progress", "state": "pending"},
            {"context": "CodeRabbit", "description": "Review completed", "state": "success"},
            {"context": "CodeRabbit", "description": "Review in progress", "state": "pending"},
            {"context": "CodeRabbit", "description": "Review skipped: incremental reviews are disabled", "state": "success"},
            {"context": "CodeRabbit", "description": "Review queued", "state": "pending"}
        ]"#;
        assert_eq!(
            parse_coderabbit_status(json),
            "success",
            "指摘ゼロ完了の実測 status list 先頭は success。正しい SHA を渡せば完了検知できる"
        );
    }

    #[test]
    fn walkthrough_clean_detected_when_marker_present_with_header() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"},
             "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nNo actionable comments were generated in the recent review. 🎉",
             "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert!(parse_walkthrough_clean_marker(
            json,
            "2026-04-01T12:00:00Z"
        ));
    }

    #[test]
    fn walkthrough_clean_skipped_when_rate_limit_overlay_present() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"},
             "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nrate limited by coderabbit.ai\nMore reviews will be available in 1 minute and 30 seconds.\nNo actionable comments were generated in the recent review.",
             "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert!(!parse_walkthrough_clean_marker(
            json,
            "2026-04-01T12:00:00Z"
        ));
    }

    #[test]
    fn walkthrough_clean_skipped_when_marker_missing() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"},
             "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nReview summary: 3 changes detected.\n## Walkthrough\n...",
             "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert!(!parse_walkthrough_clean_marker(
            json,
            "2026-04-01T12:00:00Z"
        ));
    }

    #[test]
    fn walkthrough_clean_skipped_when_header_missing_to_avoid_user_post_false_positive() {
        let json = r#"[
            {"user": {"login": "humanreviewer"},
             "body": "Quoting CR: No actionable comments were generated in the recent review.",
             "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert!(!parse_walkthrough_clean_marker(
            json,
            "2026-04-01T12:00:00Z"
        ));
    }

    #[test]
    fn walkthrough_clean_skipped_when_coderabbitai_post_lacks_header_marker() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"},
             "body": "Plain text without header.\nNo actionable comments were generated in the recent review.",
             "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert!(!parse_walkthrough_clean_marker(
            json,
            "2026-04-01T12:00:00Z"
        ));
    }

    #[test]
    fn walkthrough_clean_skipped_when_event_time_before_push_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"},
             "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nNo actionable comments were generated in the recent review.",
             "created_at": "2026-04-01T11:00:00Z"}
        ]"#;
        assert!(!parse_walkthrough_clean_marker(
            json,
            "2026-04-01T12:00:00Z"
        ));
    }

    #[test]
    fn comments_filters_by_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "_old comment", "created_at": "2026-04-01T10:00:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "_new comment", "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    #[test]
    fn comments_filters_by_user() {
        let json = r#"[
            {"user": {"login": "someuser"}, "body": "_comment", "created_at": "2026-04-01T12:30:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "_comment", "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    #[test]
    fn comments_filters_coderabbit_user_only() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Summary of changes", "created_at": "2026-04-01T12:30:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "<!-- auto-generated -->", "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 2);
    }

    #[test]
    fn comments_empty() {
        assert_eq!(parse_new_comments("[]", "2026-04-01T12:00:00Z"), 0);
    }

    #[test]
    fn comments_excludes_review_in_progress() {
        let json = r#"[
            {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:00:00Z","body":"<!-- review in progress by coderabbit.ai -->\nCurrently processing..."},
            {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:05:00Z","body":"_Actionable comments posted: 2_\nReview summary..."}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    /// rate-limit comment は new_comments から除外する
    /// (CR review feedback PR #97 round 2: rate-limit が new_comments を汚染すると
    ///  decide() が action_required を早期 return して rate-limit retry 経路に入らない)
    #[test]
    fn comments_excludes_rate_limit() {
        let json = r#"[
            {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:00:00Z","body":"Rate limit exceeded\nPlease wait 5 minutes and 0 seconds before requesting another review."},
            {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:05:00Z","body":"_Actionable comments posted: 2_\nReview summary..."}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    #[test]
    fn actionable_extracts_count() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Some review\nActionable comments posted: 3\nMore text", "submitted_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            Some(3)
        );
    }

    #[test]
    fn actionable_no_match() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "No actionable items", "submitted_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            None
        );
    }

    #[test]
    fn actionable_filters_by_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 5", "submitted_at": "2026-04-01T10:00:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            None
        );
    }

    /// 順位 214 (PR #213 post-merge-feedback T2-2 採用): `parse_actionable_comments` の
    /// `submitted_at >= push_time` inclusive 比較を境界で固定する。`>` (exclusive) に
    /// 戻された際にこの test が落ちる構造で、direct push 直後の CR review が同時刻
    /// イベントとして報告される edge case の retest 取りこぼしを機械的に防ぐ。
    ///
    /// 由来: PR #213 takt-fix iter 2 で `t > push_time` → `t >= push_time` の境界 fix。
    /// 既存 rule⑦ `no-time-field-strict-greater` は `submitted_at` 名直接使用 ケースのみ
    /// catch し、変数名 `t` に抽出された後は static lint 不能。test による second defense layer。
    #[test]
    fn actionable_includes_review_at_exact_push_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 3", "submitted_at": "2026-04-01T12:00:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            Some(3),
            "submitted_at == push_time の review は inclusive 比較で含むべき (`>=` が `>` に戻されると取りこぼす)"
        );
    }

    /// 順位 214 (negative sentinel): 配列 latest 位置に「push 以前の sentinel 99」を
    /// 置き、time filter が壊れた場合 `rfind` がそれを先に返す構造にする。time filter が
    /// 正しく動くと sentinel は除外され、配列前方の `>=` 適合 review (= 5) が選ばれる。
    /// 既存 `actionable_filters_by_time` は単一 review のみで test するが、本 test は
    /// 「複数 review 中で boundary 適合のみを選ぶ」exclusion 挙動を sentinel pre-populate
    /// で固定する (memory `feedback_test_dry_antipattern.md` 適用、独立 setup)。
    #[test]
    fn actionable_excludes_review_before_push_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 5", "submitted_at": "2026-04-01T12:30:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 99", "submitted_at": "2026-04-01T11:00:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            Some(5),
            "submitted_at < push_time の sentinel review (Actionable: 99) は time filter で除外され、push_time 以降の review (Actionable: 5) が選ばれるべき。99 が返れば time filter が機能していない"
        );
    }

    #[test]
    fn extract_count_from_body() {
        assert_eq!(
            extract_actionable_count("Actionable comments posted: 7"),
            Some(7)
        );
    }

    #[test]
    fn extract_count_zero() {
        assert_eq!(
            extract_actionable_count("Actionable comments posted: 0"),
            Some(0)
        );
    }

    #[test]
    fn extract_count_not_found() {
        assert_eq!(extract_actionable_count("No issues found"), None);
    }

    #[test]
    fn unresolved_threads_counts() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": false},
                                {"isResolved": true},
                                {"isResolved": false}
                            ]
                        }
                    }
                }
            }
        }"#;
        assert_eq!(parse_unresolved_threads(json), Some(2));
    }

    #[test]
    fn unresolved_threads_all_resolved() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": true}
                            ]
                        }
                    }
                }
            }
        }"#;
        assert_eq!(parse_unresolved_threads(json), Some(0));
    }

    #[test]
    fn unresolved_threads_invalid_json() {
        assert_eq!(parse_unresolved_threads("{}"), None);
    }
}
