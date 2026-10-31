//! 順位 585: CodeRabbit のスキップ通知を、生 JSON から `decide()` / summary まで連結で固定する。
//!
//! 2026-10-21 から CR は自動レビューをスキップし、PR に通知を付けるようになった
//! ([ADR-085](../../../../docs/adr/adr-085-third-party-review-means.md) § 改訂 2026-10-22)。
//! スキップは 2 つの面に出る:
//!
//! - commit status: state `success`、description が `Review skipped` で始まる
//! - walkthrough comment: `skip review by coderabbit.ai` の段 (レビューが完了すると消える)
//!
//! どちらか一方だけを見る実装に戻ると、片方の面にしか出ないケースで判定が崩れる
//! (re-push のスキップは status を付けず comment だけ — #600)。fixture の本文と status は
//! #585 / #586 / #600 で実際に付いたものから取った (ASCII アートの格言などは省いた)。

use super::{build_summary, decide};
use crate::models::{CiRunSummary, CiStatus, CodeRabbitStatus};
use crate::parsers::{
    parse_coderabbit_status, parse_new_comments, parse_walkthrough_clean_marker,
    resolve_review_state,
};

const PUSH_TIME: &str = "2026-10-22T04:48:00Z";
const BEFORE_PUSH: &str = "2026-10-22T04:40:00Z";
const AFTER_PUSH: &str = "2026-10-22T04:48:36Z";

/// #586 で付いたスキップの段 (walkthrough comment の一部)。
const SKIP_BLOCK: &str = "<!-- This is an auto-generated comment: skip review by coderabbit.ai -->

> [!IMPORTANT]
> ## Review skipped
>
> Auto incremental reviews are disabled on this repository.
>
> Use the checkbox below for a quick retry:
> - [ ] <!-- {\"checkboxId\":\"e9bb8d72-00e8-4f67-9cb2-caf3b22574fe\"} --> 🔍 Trigger review

<!-- end of auto-generated comment: skip review by coderabbit.ai -->";

const SUMMARY_HEADER: &str =
    "<!-- coderabbit-ai-generated:v1 -->\n\n<!-- This is an auto-generated comment: summarize by coderabbit.ai -->";

const CLEAN_MARKER: &str = "No actionable comments were generated in the recent review.";

fn ci_success() -> CiStatus {
    CiStatus {
        overall: "success".to_string(),
        runs: vec![CiRunSummary {
            name: "rust (ubuntu-latest)".to_string(),
            conclusion: "success".to_string(),
        }],
    }
}

fn status_json(entries: &[(&str, &str)]) -> String {
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|(state, description)| {
            serde_json::json!({"context": "CodeRabbit", "state": state, "description": description})
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

fn comment_json(body: &str, created_at: &str, updated_at: &str) -> String {
    serde_json::json!([{
        "user": {"login": "coderabbitai[bot]"},
        "body": body,
        "created_at": created_at,
        "updated_at": updated_at,
    }])
    .to_string()
}

/// main.rs の `run_check` と同じ順で parser を通す (gh の呼び出しだけを除く)。
fn cr_from(statuses_json: &str, comments_json: &str) -> CodeRabbitStatus {
    CodeRabbitStatus {
        review_state: resolve_review_state(
            parse_coderabbit_status(statuses_json),
            comments_json,
            PUSH_TIME,
        ),
        new_comments: parse_new_comments(comments_json, PUSH_TIME),
        actionable_comments: None,
        unresolved_threads: Some(0),
        walkthrough_clean: parse_walkthrough_clean_marker(comments_json, PUSH_TIME),
    }
}

/// 完了基準 1: スキップ通知だけが付いた PR で action_required を返さない。
/// 旧実装は通知を新しいコメントとして数え、action_required を返していた。
#[test]
fn skip_notice_only_is_not_started_and_keeps_monitoring() {
    let statuses = status_json(&[(
        "success",
        "Review skipped: manual review required for this OSS repository",
    )]);
    let body = format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}");
    let comments = comment_json(&body, AFTER_PUSH, AFTER_PUSH);

    let cr = cr_from(&statuses, &comments);
    assert_eq!(cr.review_state, "not_started");
    assert_eq!(cr.new_comments, 0, "スキップ通知は新しいコメントに数えない");

    let (status, action) = decide(&ci_success(), &cr, None);
    assert_eq!((status.as_str(), action.as_str()), ("pending", "continue_monitoring"));
    assert!(
        build_summary(&ci_success(), &cr, None).contains("CodeRabbitレビュー未開始"),
        "summary はレビュー済みと読める文言にしない"
    );
}

/// #586: 以前の clean な walkthrough とスキップの段が同居する。段を足す編集で updated_at が
/// push 後になるので、旧実装は clean marker を今回のものと読み complete を返していた。
#[test]
fn clean_walkthrough_with_skip_block_does_not_complete() {
    let statuses = status_json(&[
        ("success", "Review skipped: incremental reviews are disabled"),
        ("pending", "Review in progress"),
    ]);
    let body = format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}\n\n{CLEAN_MARKER}");
    let comments = comment_json(&body, BEFORE_PUSH, AFTER_PUSH);

    let cr = cr_from(&statuses, &comments);
    assert_eq!(cr.review_state, "not_started");
    assert!(!cr.walkthrough_clean, "段と同居する clean marker は今回のレビューの証拠ではない");

    let (status, _) = decide(&ci_success(), &cr, None);
    assert_ne!(status, "complete");
}

/// #600 の re-push: commit status が付かず、comment に段が足されただけ。
#[test]
fn skip_block_without_status_resolves_not_found_to_not_started() {
    let body = format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}");
    let comments = comment_json(&body, BEFORE_PUSH, AFTER_PUSH);

    let cr = cr_from("[]", &comments);
    assert_eq!(cr.review_state, "not_started");
}

/// push より前に付いた段は、今回の push のスキップを示さない。
#[test]
fn skip_block_edited_before_push_keeps_not_found() {
    let body = format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}");
    let comments = comment_json(&body, BEFORE_PUSH, BEFORE_PUSH);

    assert_eq!(cr_from("[]", &comments).review_state, "not_found");
}

/// 依頼の後は処理中になる。#586 では status が pending「Review in progress」へ変わった。
#[test]
fn review_in_progress_after_trigger_is_pending() {
    let statuses = status_json(&[
        ("pending", "Review in progress"),
        ("success", "Review skipped: manual review required for this OSS repository"),
    ]);
    let body = "<!-- This is an auto-generated comment: review in progress by coderabbit.ai -->\nCurrently processing new changes in this PR.";
    let comments = comment_json(body, AFTER_PUSH, AFTER_PUSH);

    let cr = cr_from(&statuses, &comments);
    assert_eq!(cr.review_state, "pending");
    let (status, action) = decide(&ci_success(), &cr, None);
    assert_eq!((status.as_str(), action.as_str()), ("pending", "continue_monitoring"));
}

/// 完了基準 2: 依頼の後に「Review completed」と clean な walkthrough が付けば、従来どおり完了。
/// 段はレビューが完了すると消える (#585 の最終的な comment で確認)。
#[test]
fn review_completed_after_skip_still_completes() {
    let statuses = status_json(&[
        ("success", "Review completed"),
        ("pending", "Review in progress"),
        ("success", "Review skipped: manual review required for this OSS repository"),
    ]);
    let body = format!("{SUMMARY_HEADER}\n\n{CLEAN_MARKER}");
    let comments = comment_json(&body, BEFORE_PUSH, AFTER_PUSH);

    let cr = cr_from(&statuses, &comments);
    assert_eq!(cr.review_state, "success");
    let (status, action) = decide(&ci_success(), &cr, None);
    assert_eq!((status.as_str(), action.as_str()), ("complete", "stop_monitoring_success"));
}

fn checked_skip_block() -> String {
    SKIP_BLOCK.replacen("- [ ]", "- [x]", 1)
}

fn request_comment_json(body: &str, created_at: &str) -> String {
    serde_json::json!([{
        "user": {"login": "aloekun"},
        "body": body,
        "created_at": created_at,
        "updated_at": created_at,
    }])
    .to_string()
}

fn merge_comments(a: &str, b: &str) -> String {
    let mut items: Vec<serde_json::Value> = serde_json::from_str(a).unwrap();
    items.extend(serde_json::from_str::<Vec<serde_json::Value>>(b).unwrap());
    serde_json::Value::Array(items).to_string()
}

/// #602: PR 作成時に Trigger review へチェックを入れた直後。status と段はまだスキップを示すが、
/// 依頼は済んでいるので「依頼してください」ではなくレビュー待ちと伝える。
#[test]
fn checked_trigger_after_skip_status_is_pending_without_asking() {
    let statuses = status_json(&[(
        "success",
        "Review skipped: manual review required for this OSS repository",
    )]);
    let body = format!("{SUMMARY_HEADER}\n{}", checked_skip_block());
    let comments = comment_json(&body, AFTER_PUSH, AFTER_PUSH);

    let cr = cr_from(&statuses, &comments);
    assert_eq!(cr.review_state, "pending");
    let (status, action) = decide(&ci_success(), &cr, None);
    assert_eq!((status.as_str(), action.as_str()), ("pending", "continue_monitoring"));
    let summary = build_summary(&ci_success(), &cr, None);
    assert!(!summary.contains("依頼"), "依頼済みの PR に依頼を促さない: {summary}");
}

/// re-push の形 (status 無し、段だけ) でも、チェック済みならレビュー待ち。
#[test]
fn checked_trigger_in_skip_block_without_status_is_pending() {
    let body = format!("{SUMMARY_HEADER}\n{}", checked_skip_block());
    let comments = comment_json(&body, BEFORE_PUSH, AFTER_PUSH);

    assert_eq!(cr_from("[]", &comments).review_state, "pending");
}

/// 通知を待たずに `@coderabbitai review` で依頼した場合もレビュー待ち。
#[test]
fn review_command_after_push_is_pending() {
    let statuses = status_json(&[("success", "Review skipped: incremental reviews are disabled")]);
    let skip = comment_json(&format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}"), AFTER_PUSH, AFTER_PUSH);
    let comments = merge_comments(&skip, &request_comment_json("@coderabbitai review", AFTER_PUSH));

    assert_eq!(cr_from(&statuses, &comments).review_state, "pending");
}

/// 通知のコメントが無くても、status がスキップで push 後に依頼していればレビュー待ち。
#[test]
fn review_command_after_push_without_notice_is_pending() {
    let statuses = status_json(&[("success", "Review skipped: incremental reviews are disabled")]);
    let comments = request_comment_json("@coderabbitai review", AFTER_PUSH);

    assert_eq!(cr_from(&statuses, &comments).review_state, "pending");
}

/// push 前の依頼を push 後に編集しても、今回の push を依頼したことにならない (#603 の指摘)。
/// 依頼コメントは `created_at` で見る。
#[test]
fn review_command_edited_after_push_keeps_not_started() {
    let statuses = status_json(&[("success", "Review skipped: incremental reviews are disabled")]);
    let skip = comment_json(&format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}"), AFTER_PUSH, AFTER_PUSH);
    let edited = serde_json::json!([{
        "user": {"login": "aloekun"},
        "body": "@coderabbitai review",
        "created_at": BEFORE_PUSH,
        "updated_at": AFTER_PUSH,
    }])
    .to_string();

    assert_eq!(cr_from(&statuses, &merge_comments(&skip, &edited)).review_state, "not_started");
}

/// push より前の依頼は、今回の push のレビューを頼んだことにならない。
#[test]
fn review_command_before_push_keeps_not_started() {
    let statuses = status_json(&[("success", "Review skipped: incremental reviews are disabled")]);
    let skip = comment_json(&format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}"), AFTER_PUSH, AFTER_PUSH);
    let comments = merge_comments(&skip, &request_comment_json("@coderabbitai review", BEFORE_PUSH));

    assert_eq!(cr_from(&statuses, &comments).review_state, "not_started");
}

/// 依頼の文言を文中で触れただけのコメントは依頼として数えない (行頭のコマンドだけを見る)。
#[test]
fn mentioning_review_command_mid_sentence_is_not_a_request() {
    let statuses = status_json(&[("success", "Review skipped: incremental reviews are disabled")]);
    let skip = comment_json(&format!("{SUMMARY_HEADER}\n{SKIP_BLOCK}"), AFTER_PUSH, AFTER_PUSH);
    let note = request_comment_json("必要なら `@coderabbitai review` を投稿する", AFTER_PUSH);

    assert_eq!(cr_from(&statuses, &merge_comments(&skip, &note)).review_state, "not_started");
}

/// コメントの JSON が壊れていても判定を変えない (status 由来の状態をそのまま返す)。
#[test]
fn broken_comments_json_keeps_status_state() {
    let statuses = status_json(&[(
        "success",
        "Review skipped: manual review required for this OSS repository",
    )]);

    assert_eq!(cr_from(&statuses, "not json").review_state, "not_started");
    assert_eq!(cr_from("[]", "not json").review_state, "not_found");
}

/// 判定は description の接頭辞で行い、スキップの理由の文言には頼らない。
/// 理由は停止のたびに変わってきた (seat / incremental / stars)。
#[test]
fn every_observed_skip_description_is_not_started() {
    for description in [
        "Review skipped: manual review required for this OSS repository",
        "Review skipped: incremental reviews are disabled",
        "Review skipped: bot user not eligible for review",
    ] {
        let statuses = status_json(&[("success", description)]);
        assert_eq!(
            parse_coderabbit_status(&statuses),
            "not_started",
            "success でもスキップはレビュー済みではない: {description}"
        );
    }
}
