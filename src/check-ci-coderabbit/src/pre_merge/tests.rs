use super::*;

const HEAD: &str = "731c76e5b1bcf0ffbd3da1f83ef06cf86f101581";
const OLDER: &str = "6a0b200a130f8059d318a2f5bf2216dfb8beeb56";

/// #623 の要約コメント (2026-11-06) から、判定に使う部分を抜き出したもの。表の行・
/// マーカー・状態の画像は実データのまま (説明の列だけ短くした)。
fn pr623_summary_body(covered_commit: &str) -> String {
    format!(
        r#"<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- change_assessment_start -->
<!-- change_assessment_commit:"{covered_commit}" -->
<!-- change_assessment_end -->
<!-- final_review_risk_coverage:{{"sourceCommitId":"{covered_commit}","coveredCommitId":"{covered_commit}","kind":"reviewed"}} -->
<!-- pre_merge_checks_walkthrough_start -->

<details>
<summary><strong>Pre-merge checks: <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" alt="Passed"></picture></sup> 4 · <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" alt="Failed"></picture></sup> 1</strong></summary>

<strong>Failed checks: 1 warning</strong>

|     Check name     |                                              Status                                              | Explanation | Resolution |
| :---------------- | :----------------------------------------------------------------------------------------------: | :---------- | :--------- |
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 76.92% which is insufficient. The required threshold is 80.00%. | Write docstrings for the functions missing them to satisfy the coverage threshold. |

|         Check name         |                                             Status                                             | Explanation                                                              |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :----------------------------------------------------------------------- |
|      Description Check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped - CodeRabbit’s high-level summary is enabled.              |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | タイトルは変更の主目的と一致します。 |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request. |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request. |

</details>

<!-- pre_merge_checks_walkthrough_end -->
<!-- finishing_touch_checkbox_start -->
| Not a check | ![Error](https://example.invalid/pre-merge-checks-error.svg) | 節の外の行は読まない |
"#
    )
}

/// #624 の要約コメントのように、全部通っている表。
fn all_passed_body() -> String {
    pr623_summary_body(HEAD).replace(
        "| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg)",
        "| Docstring Coverage | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg)",
    )
}

fn comments_json(body: &str) -> String {
    serde_json::json!([
        {
            "user": {"login": "someone"},
            "body": "<!-- pre_merge_checks_walkthrough_start -->\n| Fake | ![Warning](x/pre-merge-checks-warning.svg) | 人間のコメントは読まない |",
            "created_at": "2026-11-06T00:00:00Z",
        },
        {
            "user": {"login": "coderabbitai[bot]"},
            "body": body,
            "created_at": "2026-11-06T01:00:00Z",
            "updated_at": "2026-11-06T02:00:00Z",
            "html_url": "https://github.com/o/r/pull/623#issuecomment-1",
        }
    ])
    .to_string()
}

fn checks(failed: usize, stale: bool) -> PreMergeChecks {
    PreMergeChecks {
        failed: (0..failed)
            .map(|i| PreMergeCheckFailure {
                name: format!("check {}", i),
                status: "Warning".to_string(),
                explanation: String::new(),
            })
            .collect(),
        measured_commit: Some(OLDER.to_string()),
        stale,
        summary_url: None,
    }
}

fn decided(status: &str, action: &str) -> (String, String) {
    (status.to_string(), action.to_string())
}

#[test]
fn reads_the_failed_check_of_pr623_and_ignores_passed_rows() {
    let parsed = parse_pre_merge_checks(&comments_json(&pr623_summary_body(HEAD)), Some(HEAD)).unwrap();
    assert_eq!(
        parsed.failed,
        vec![PreMergeCheckFailure {
            name: "Docstring Coverage".to_string(),
            status: "Warning".to_string(),
            explanation: "Docstring coverage is 76.92% which is insufficient. The required threshold is 80.00%.".to_string(),
        }]
    );
    assert_eq!(parsed.measured_commit.as_deref(), Some(HEAD));
    assert!(!parsed.stale);
    assert_eq!(parsed.summary_url.as_deref(), Some("https://github.com/o/r/pull/623#issuecomment-1"));
}

#[test]
fn all_passed_table_has_no_failures() {
    let parsed = parse_pre_merge_checks(&comments_json(&all_passed_body()), Some(HEAD)).unwrap();
    assert!(parsed.failed.is_empty());
}

#[test]
fn measurement_of_another_commit_is_stale() {
    let parsed = parse_pre_merge_checks(&comments_json(&pr623_summary_body(OLDER)), Some(HEAD)).unwrap();
    assert_eq!(parsed.measured_commit.as_deref(), Some(OLDER));
    assert!(parsed.stale);
}

/// fail-closed: 照合できないときは古いと見なさない (今の失敗として止める)。
#[test]
fn unknown_head_or_measured_commit_is_not_stale() {
    let parsed = parse_pre_merge_checks(&comments_json(&pr623_summary_body(OLDER)), None).unwrap();
    assert!(!parsed.stale);

    let without_markers = pr623_summary_body(OLDER)
        .lines()
        .filter(|l| !l.contains(OLDER))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = parse_pre_merge_checks(&comments_json(&without_markers), Some(HEAD)).unwrap();
    assert_eq!(parsed.measured_commit, None);
    assert!(!parsed.stale);
}

#[test]
fn falls_back_to_change_assessment_commit() {
    let body = pr623_summary_body(HEAD)
        .lines()
        .filter(|l| !l.contains("final_review_risk_coverage"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = parse_pre_merge_checks(&comments_json(&body), Some(HEAD)).unwrap();
    assert_eq!(parsed.measured_commit.as_deref(), Some(HEAD));
}

/// fail-closed: 節はあるのに行を読めない (書式が変わった) ことを、失敗として出す。
#[test]
fn unreadable_section_is_reported_as_a_failure() {
    let body = "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n\
                <!-- pre_merge_checks_walkthrough_start -->\nPre-merge checks: 新しい書式\n\
                <!-- pre_merge_checks_walkthrough_end -->";
    let parsed = parse_pre_merge_checks(&comments_json(body), Some(HEAD)).unwrap();
    assert_eq!(parsed.failed.len(), 1);
    assert_eq!(parsed.failed[0].name, UNREADABLE_TABLE_NAME);
}

/// 未知の状態名 (`Inconclusive` など) も通ったとは言えないので失敗に数える。
#[test]
fn unknown_status_counts_as_failure() {
    let body = all_passed_body().replace("![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped - CodeRabbit’s", "![Inconclusive](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-inconclusive.svg) | Check skipped - CodeRabbit’s");
    let parsed = parse_pre_merge_checks(&comments_json(&body), Some(HEAD)).unwrap();
    assert_eq!(parsed.failed.len(), 1);
    assert_eq!(parsed.failed[0].name, "Description Check");
    assert_eq!(parsed.failed[0].status, "Inconclusive");
}

#[test]
fn no_section_or_no_summary_comment_is_none() {
    let body = "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nNo actionable comments were generated in the recent review.";
    assert_eq!(parse_pre_merge_checks(&comments_json(body), Some(HEAD)), None);
    assert_eq!(parse_pre_merge_checks("[]", Some(HEAD)), None);
}

#[test]
fn current_failure_turns_success_into_action_required() {
    let gated = apply_pre_merge_gate(decided("complete", "stop_monitoring_success"), Some(&checks(1, false)));
    assert_eq!(gated, decided("action_required", "action_required"));
}

/// 古い測定の失敗は、直ったかを確かめられないので success にも action_required にもしない。
#[test]
fn stale_failure_turns_success_into_continue_monitoring() {
    let gated = apply_pre_merge_gate(decided("complete", "stop_monitoring_success"), Some(&checks(1, true)));
    assert_eq!(gated, decided("pending", "continue_monitoring"));
}

#[test]
fn gate_leaves_non_success_decisions_alone() {
    for d in [
        decided("error", "stop_monitoring_failure"),
        decided("pending", "continue_monitoring"),
        decided("action_required", "action_required"),
    ] {
        assert_eq!(apply_pre_merge_gate(d.clone(), Some(&checks(1, false))), d);
        assert_eq!(apply_pre_merge_gate(d.clone(), Some(&checks(1, true))), d);
    }
}

#[test]
fn gate_passes_success_through_without_failures() {
    let success = decided("complete", "stop_monitoring_success");
    assert_eq!(apply_pre_merge_gate(success.clone(), None), success);
    assert_eq!(apply_pre_merge_gate(success.clone(), Some(&checks(0, false))), success);
}

#[test]
fn summary_names_failures_and_asks_for_remeasurement_when_stale() {
    let mut c = checks(1, false);
    c.failed[0].name = "Docstring Coverage".to_string();
    assert_eq!(
        summary_part(Some(&c)).as_deref(),
        Some("pre-merge check 失敗: Docstring Coverage (Warning)")
    );
    c.stale = true;
    let stale = summary_part(Some(&c)).unwrap();
    assert!(stale.contains("6a0b200a で測った結果"), "{}", stale);
    assert!(stale.contains("Trigger review で再測定が必要"), "{}", stale);
    assert_eq!(summary_part(Some(&checks(0, false))), None);
    assert_eq!(summary_part(None), None);
}

/// fail-closed: 状態を読めない行は、読める `Passed` 行と混ざっていても失敗に数える
/// (PR #625 CodeRabbit)。見出しと区切り行は check の行として読まない。
#[test]
fn unreadable_row_among_passed_rows_counts_as_failure() {
    let body = all_passed_body().replace(
        "![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | タイトル",
        ":warning: 新しい書式 | タイトル",
    );
    let parsed = parse_pre_merge_checks(&comments_json(&body), Some(HEAD)).unwrap();
    assert_eq!(parsed.failed.len(), 1, "{:?}", parsed.failed);
    assert_eq!(parsed.failed[0].name, "Title check");
    assert_eq!(parsed.failed[0].status, UNREADABLE_STATUS);
}

/// 最新の要約コメントを先に選ぶ。最新のものに節が無ければ、古い要約の表は読まない
/// (PR #625 CodeRabbit)。
#[test]
fn newest_summary_without_section_hides_older_tables() {
    let newest = "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n節の無い要約";
    let comments = serde_json::json!([
        {
            "user": {"login": "coderabbitai[bot]"},
            "body": pr623_summary_body(HEAD),
            "created_at": "2026-11-06T01:00:00Z",
        },
        {
            "user": {"login": "coderabbitai[bot]"},
            "body": newest,
            "created_at": "2026-11-07T01:00:00Z",
        }
    ])
    .to_string();
    assert_eq!(parse_pre_merge_checks(&comments, Some(HEAD)), None);
}
