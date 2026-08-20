//! 順位 246: statusCheckRollup の生 JSON から `decide()` の結論までを**連結で**固定する。
//!
//! ## なぜ連結を pin するのか
//!
//! 「CodeRabbit-only 構成で幻の CI pending により recheck を上限まで繰り返す」不具合は、
//! 2 つの実装がそろって初めて解消する:
//!
//! 1. [`crate::parsers::parse_ci_rollup`] が CodeRabbit 自身の commit status を除外し、
//!    他に check が無ければ `runs` を空にする。
//! 2. [`super::decide`] の `ci_pending` が `ci.overall == "pending" && !ci.runs.is_empty()`
//!    で、**空 runs の pending を待機理由にしない**。
//!
//! どちらも単体テストは持っているが、**間の受け渡しを見るテストが無かった**。片方の
//! 条件だけが変わっても (例: parser が CR を `runs` に残す、decide が `runs` を見なく
//! なる) 単体テストは緑のまま通り、幻の CI pending が黙って復活する。本 module は
//! その連結だけを対象にする。
//!
//! ## 非退行側も同じ強さで固定する
//!
//! 短絡が効きすぎると「実 CI がまだ走っているのに merge-ready」を返す。これは
//! ADR-064 が排除した陽性証拠なき success そのものなので、実 check が pending の
//! ケースを対で置く。

use super::decide;
use crate::models::CodeRabbitStatus;
use crate::parsers::parse_ci_rollup;

/// レビュー完了 (actionable 0 / unresolved 0) を表す CR 状態。
fn cr_review_complete() -> CodeRabbitStatus {
    CodeRabbitStatus {
        review_state: "success".to_string(),
        new_comments: 0,
        actionable_comments: Some(0),
        unresolved_threads: Some(0),
        walkthrough_clean: false,
    }
}

/// 順位 246 (incident 再現): check が CodeRabbit の commit status だけの構成で、
/// レビューが完了していれば **待機せず merge-ready** を返すこと。
///
/// PR #231 / #232 では、ここが `continue_monitoring` に落ちるため
/// `gh pr view --json mergeStateStatus,mergeable` を人手で叩いて merge 可能を
/// 確認する必要があった (= 幻の CI pending)。
#[test]
fn coderabbit_only_rollup_with_completed_review_is_merge_ready() {
    let rollup = r#"[
        {"__typename":"StatusContext","context":"CodeRabbit","state":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(rollup);
    assert!(
        ci.runs.is_empty(),
        "CodeRabbit の commit status は実 CI check として数えないこと"
    );

    let (status, action) = decide(&ci, &cr_review_complete(), None);

    assert_eq!(status, "complete");
    assert_eq!(
        action, "stop_monitoring_success",
        "CodeRabbit-only 構成で recheck を繰り返してはならない (幻の CI pending)"
    );
}

/// 実 CI check が 1 件も作られていない構成でも同じ結論になること。
///
/// `parse_ci_rollup` は空 rollup を `overall="pending"` で返す (ADR-064: check 未作成を
/// success と報告しない)。その pending を `decide` が待機理由にしないことが要点。
#[test]
fn empty_rollup_with_completed_review_is_merge_ready() {
    let ci = parse_ci_rollup("[]");
    assert_eq!(ci.overall, "pending");

    let (status, action) = decide(&ci, &cr_review_complete(), None);

    assert_eq!(status, "complete");
    assert_eq!(action, "stop_monitoring_success");
}

/// **短絡が効きすぎないこと**: 実 CI check が pending なら、レビューが完了していても
/// 従来どおり待機する。
#[test]
fn real_ci_pending_keeps_waiting_even_when_review_is_complete() {
    let rollup = r#"[
        {"__typename":"CheckRun","name":"rust (ubuntu-latest)","status":"IN_PROGRESS"},
        {"__typename":"StatusContext","context":"CodeRabbit","state":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(rollup);
    assert_eq!(ci.runs.len(), 1, "実 check は runs に残ること");

    let (status, action) = decide(&ci, &cr_review_complete(), None);

    assert_eq!(status, "pending");
    assert_eq!(
        action, "continue_monitoring",
        "実 CI が走っている間に merge-ready を返してはならない (ADR-064)"
    );
}

/// 実 CI が完了していれば従来どおり merge-ready。上のテストが「常に待つ」方向へ
/// 壊れていないことの対比。
#[test]
fn real_ci_success_with_completed_review_is_merge_ready() {
    let rollup = r#"[
        {"__typename":"CheckRun","name":"rust (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"StatusContext","context":"CodeRabbit","state":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(rollup);

    let (status, action) = decide(&ci, &cr_review_complete(), None);

    assert_eq!(status, "complete");
    assert_eq!(action, "stop_monitoring_success");
}

/// CodeRabbit-only 構成でも **レビューが未実施なら待機を続ける** こと。
///
/// 「CI を待たない」が「レビューを待たない」に広がると、ADR-064 が塞いだ
/// 陽性証拠なき success に戻る。短絡の適用範囲を CI 側だけに閉じ込める。
#[test]
fn coderabbit_only_rollup_without_review_keeps_waiting() {
    let rollup = r#"[
        {"__typename":"StatusContext","context":"CodeRabbit","state":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(rollup);
    let cr = CodeRabbitStatus {
        review_state: "not_found".to_string(),
        ..Default::default()
    };

    let (status, action) = decide(&ci, &cr, None);

    assert_eq!(status, "pending");
    assert_eq!(
        action, "continue_monitoring",
        "レビュー未実施は CI 短絡の対象外 (陽性証拠が要る)"
    );
}
