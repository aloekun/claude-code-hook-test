use super::*;
fn make_pr_info(pr: u64, repo: &str, head: Option<&str>) -> PrInfo {
    PrInfo {
        pr_number: Some(pr),
        repo: Some(repo.into()),
        push_time: None,
        head_commit: head.map(String::from),
        fix_push_time: None,
    }
}

fn make_state(pr: u64, repo: &str, head: Option<&str>) -> PrMonitorState {
    let mut s = PrMonitorState::new(Some(pr), Some(repo.into()), "t".into());
    s.head_commit = head.map(String::from);
    s
}

/// findings 0 件なら fix commit を作らない (空 commit → abandon の noise を作らない)。
///
/// takt は CodeRabbit がコメントを投稿しただけでも起動するため、findings が空のまま
/// ここへ来る経路が実在する (2026-08-18 の PR #417 監視で実観測)。
///
/// あわせて **severity では絞らない**契約も固定する。`extract_severity` の `"Info"` は
/// 判定不能時の受け皿でもあるため、`Info` だけを「直す対象なし」に倒すと、書式変更で
/// 解析できなかった実指摘まで黙って skip する。
/// **findings 0 件は re-push 経路へ進ませない** (CodeRabbit PR #418 Major 指摘)。
///
/// fix commit を作らない = `FixCommitState::None` のまま `execute_repush_flow` へ進むと、
/// `decide_repush_action` の `(HasChange, _, true)` が `AutoPush` を選び、**分離コミット
/// なしで push される**。auto push を切っていても `UserConfirmNoSeparation` になる。
#[test]
fn empty_findings_must_not_reach_the_repush_flow() {
    use crate::stages::repush::decide_repush_action;
    use crate::stages::repush::{RepushAction, RepushDecision};

    assert!(
        matches!(
            decide_repush_action(&RepushDecision::HasChange, &FixCommitState::None, true),
            RepushAction::AutoPush
        ),
        "前提: fix_state=None でも HasChange + auto なら AutoPush が選ばれる"
    );
    assert!(
        !has_findings_to_fix(&[]),
        "したがって findings 0 件は repush 経路へ入る前に止める必要がある"
    );
}

#[test]
fn only_the_finding_count_decides_whether_a_fix_commit_is_needed() {
    assert!(!has_findings_to_fix(&[]), "0 件なら事前作成しない");
    assert!(
        has_findings_to_fix(&[finding("Info")]),
        "Info のみでも「直す対象なし」とはしない (Info は判定不能の受け皿)"
    );
    assert!(has_findings_to_fix(&[finding("Critical")]));
}

#[test]
fn should_continue_state_true_when_pr_repo_head_match() {
    let state = make_state(42, "o/r", Some("abc1234"));
    let pr_info = make_pr_info(42, "o/r", Some("abc1234"));
    assert!(should_continue_state(&state, &pr_info));
}

#[test]
fn should_continue_state_false_when_head_differs() {
    let state = make_state(42, "o/r", Some("abc1234"));
    let pr_info = make_pr_info(42, "o/r", Some("def5678"));
    assert!(
        !should_continue_state(&state, &pr_info),
        "CR Major #1 由来: head 不一致なら fresh 初期化に倒す (stale な時刻窓を持ち込まない)"
    );
}

#[test]
fn should_continue_state_false_when_state_head_missing() {
    let state = make_state(42, "o/r", None);
    let pr_info = make_pr_info(42, "o/r", Some("abc1234"));
    assert!(
        !should_continue_state(&state, &pr_info),
        "legacy state (head_commit None) は安全側で fresh 初期化扱い"
    );
}

#[test]
fn should_continue_state_false_when_pr_info_head_missing() {
    let state = make_state(42, "o/r", Some("abc1234"));
    let pr_info = make_pr_info(42, "o/r", None);
    assert!(
        !should_continue_state(&state, &pr_info),
        "current head 取得失敗時は安全側で fresh 初期化扱い"
    );
}

#[test]
fn should_continue_state_false_when_pr_or_repo_differs() {
    let state = make_state(42, "o/r", Some("abc1234"));
    let other_pr = make_pr_info(99, "o/r", Some("abc1234"));
    let other_repo = make_pr_info(42, "x/y", Some("abc1234"));
    assert!(!should_continue_state(&state, &other_pr));
    assert!(!should_continue_state(&state, &other_repo));
}

use crate::stages::poll::PollResult;
use crate::state::CodeRabbitState;
use lib_report_formatter::Finding;

fn poll_result(action: &str, review_state: Option<&str>, findings: Vec<Finding>) -> PollResult {
    PollResult {
        action: action.into(),
        summary: "test".into(),
        ci: None,
        coderabbit: review_state.map(|rs| CodeRabbitState {
            review_state: rs.into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: None,
        }),
        findings,
        check_output: None,
        rate_limit: None,
    }
}

fn finding(severity: &str) -> Finding {
    Finding {
        severity: severity.into(),
        file: "f.rs".into(),
        line: "1".into(),
        issue: "test issue".into(),
        suggestion: "test suggestion".into(),
        source: "test".into(),
    }
}

const VERDICT_RATE_LIMITED: &str =
    "CodeRabbit rate-limit 中でレビュー未実施のため、判定を保留します (後続は GitHub Actions 経路が処理)";
const VERDICT_PENDING_REVIEW: &str =
    "review 未確定のため判定を保留します (後続は GitHub Actions 経路が処理)";
const VERDICT_REVIEW_PENDING: &str = "CodeRabbit review が未完了のため、判定を保留します";
const VERDICT_NO_PROBLEMS: &str = "問題は見つかりませんでした";
const VERDICT_MINOR: &str = "重大な問題は見つかりませんでした。軽微な改善提案があります";
const VERDICT_CRITICAL: &str = "修正が必要な指摘があります";

#[test]
fn verdict_rate_limited_takes_precedence_over_review_state() {
    let r = poll_result("rate_limited", Some("not_found"), vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_RATE_LIMITED);
}

#[test]
fn verdict_pending_review_takes_precedence_over_findings() {
    let r = poll_result(
        "pending_review",
        Some("not_found"),
        vec![finding("critical")],
    );
    assert_eq!(compute_verdict(&r), VERDICT_PENDING_REVIEW);
}

#[test]
fn verdict_pending_when_review_not_found_with_no_findings() {
    let r = poll_result("continue_monitoring", Some("not_found"), vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_REVIEW_PENDING);
}

#[test]
fn verdict_pending_when_review_pending_with_no_findings() {
    let r = poll_result("continue_monitoring", Some("pending"), vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_REVIEW_PENDING);
}

#[test]
fn verdict_pending_when_review_not_found_even_with_findings() {
    let r = poll_result(
        "continue_monitoring",
        Some("not_found"),
        vec![finding("major")],
    );
    assert_eq!(compute_verdict(&r), VERDICT_REVIEW_PENDING);
}

#[test]
fn verdict_no_problems_when_review_success_with_no_findings() {
    let r = poll_result("stop_monitoring_success", Some("success"), vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_NO_PROBLEMS);
}

#[test]
fn verdict_minor_when_review_success_with_low_severity_findings() {
    let r = poll_result(
        "stop_monitoring_success",
        Some("success"),
        vec![finding("minor")],
    );
    assert_eq!(compute_verdict(&r), VERDICT_MINOR);
}

#[test]
fn verdict_critical_when_review_success_with_critical_findings() {
    let r = poll_result(
        "stop_monitoring_success",
        Some("success"),
        vec![finding("critical")],
    );
    assert_eq!(compute_verdict(&r), VERDICT_CRITICAL);
}

#[test]
fn verdict_critical_when_severity_is_high() {
    let r = poll_result(
        "stop_monitoring_success",
        Some("success"),
        vec![finding("high")],
    );
    assert_eq!(compute_verdict(&r), VERDICT_CRITICAL);
}

#[test]
fn verdict_critical_when_severity_is_major() {
    let r = poll_result(
        "stop_monitoring_success",
        Some("success"),
        vec![finding("major")],
    );
    assert_eq!(compute_verdict(&r), VERDICT_CRITICAL);
}

#[test]
fn verdict_no_problems_when_review_skipped() {
    let r = poll_result("stop_monitoring_success", Some("skipped"), vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_NO_PROBLEMS);
}

#[test]
fn verdict_no_problems_when_coderabbit_state_absent() {
    let r = poll_result("stop_monitoring_success", None, vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_NO_PROBLEMS);
}

/// R3 用: 未解決スレッド数と rate-limit を指定できる `PollResult` を組む。
fn poll_result_with_unsettled(
    unresolved_threads: Option<usize>,
    rate_limit: Option<crate::state::RateLimitState>,
    findings: Vec<Finding>,
) -> PollResult {
    PollResult {
        action: "stop_monitoring_success".into(),
        summary: "test".into(),
        ci: None,
        coderabbit: Some(CodeRabbitState {
            review_state: "success".into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads,
        }),
        findings,
        check_output: None,
        rate_limit,
    }
}

/// PR #309 incident の rate-limit 情報 (第 3 世代書式、57 分待機)。
fn pr309_rate_limit() -> crate::state::RateLimitState {
    crate::state::RateLimitState {
        until_unix_secs: 1_784_556_707,
        comment_event_time: "2026-07-20T12:10:47Z".into(),
        wait_minutes: 57,
        wait_seconds: 0,
    }
}

/// R3 (incident 再現): rate-limit 中は findings が空でも「問題なし」と断定しない。
///
/// findings が空なのは「レビューして何も無かった」からではなく「レビューが
/// 走っていない」からであり、両者を判定文で区別する必要がある。
#[test]
fn verdict_holds_when_rate_limit_present_even_with_no_findings() {
    let r = poll_result_with_unsettled(Some(0), Some(pr309_rate_limit()), vec![]);
    let verdict = compute_verdict(&r);
    assert!(
        verdict.contains("レート制限"),
        "rate-limit 中であることを判定文に出すべき: {}",
        verdict
    );
    assert_ne!(verdict, VERDICT_NO_PROBLEMS);
}

/// R3 (incident 再現): 「未解決スレッド2件」を表示しながら同一レポートで
/// 「問題は見つかりませんでした」と断定していた矛盾を塞ぐ (PR #307/#309 実観測)。
#[test]
fn verdict_holds_when_unresolved_threads_remain_with_no_findings() {
    let r = poll_result_with_unsettled(Some(2), None, vec![]);
    let verdict = compute_verdict(&r);
    assert_eq!(
        verdict,
        "未解決スレッドが2件残っているため、判定を保留します"
    );
}

/// findings が Minor のみでも、未解決スレッドが残っていれば
/// 「重大な問題は見つかりませんでした」と断定しない。
#[test]
fn verdict_holds_when_unresolved_threads_remain_with_minor_findings() {
    let r = poll_result_with_unsettled(Some(1), None, vec![finding("minor")]);
    let verdict = compute_verdict(&r);
    assert_ne!(verdict, VERDICT_MINOR);
    assert!(
        verdict.contains("未解決スレッドが1件"),
        "verdict={}",
        verdict
    );
}

/// 重大な指摘がある場合は、未解決スレッドより「修正が必要」を優先する
/// (どちらも行動を促す文言だが、より強い方を出す)。
#[test]
fn verdict_critical_takes_precedence_over_unresolved_threads() {
    let r = poll_result_with_unsettled(Some(2), None, vec![finding("critical")]);
    assert_eq!(compute_verdict(&r), VERDICT_CRITICAL);
}

/// 新 guard が効きすぎないこと: 未解決スレッド 0 件・rate-limit なし・findings 空
/// なら従来どおり「問題は見つかりませんでした」を出す。
#[test]
fn verdict_no_problems_when_no_unsettled_signals_remain() {
    let r = poll_result_with_unsettled(Some(0), None, vec![]);
    assert_eq!(compute_verdict(&r), VERDICT_NO_PROBLEMS);
}

/// 順位 141: `resume_fix_push_time_or_started_at` Case A —
/// state に `fix_push_time` が設定済みの場合、fallback の `started_at` ではなく
/// state の値が返されることを検証する。
#[test]
fn resume_returns_fix_push_time_from_state_when_set() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let mut s = PrMonitorState::new(Some(1), None, "t".into());
    s.fix_push_time = Some("2026-05-22T06:06:00Z".into());
    std::fs::write(tmp.path(), serde_json::to_string(&s).unwrap()).unwrap();
    let result = resume_fix_push_time_or_started_at("2026-05-22T06:00:00Z", tmp.path());
    assert_eq!(
        result.as_deref(),
        Some("2026-05-22T06:06:00Z"),
        "state に fix_push_time がある場合、fallback の started_at ではなく state の値が返る"
    );
}

// ─── 順位 490: takt 前後の作業ツリー変更判定 ───

/// **順位 490 の実観測の再現** — feature ブランチで `@` が PR の中身そのもの (= 非空) でも、
/// takt が何も変えていなければ「変更なし」と判定されること。移送前はここが必ず警告だった。
#[test]
fn an_untouched_tree_is_unchanged_even_when_at_is_not_an_empty_commit() {
    let verdict = judge_tree_change(Some("abc123"), |from| {
        assert_eq!(from, "abc123", "捕捉済みの pre_takt_cid を基準に比較すること");
        Ok(String::new())
    });
    assert_eq!(verdict, TreeChange::Unchanged);
}

/// jj が空白のみを返す場合も「変更なし」。
#[test]
fn a_whitespace_only_summary_is_unchanged() {
    let verdict = judge_tree_change(Some("abc123"), |_| Ok("\n  \n".to_string()));
    assert_eq!(verdict, TreeChange::Unchanged);
}

/// **stderr の警告だけでは `Changed` にならない** (CodeRabbit #446)。
///
/// `capture_diff_summary` は成功時 stdout だけを返すので、jj が警告を出しても判定に届く
/// summary は空のままになる。ここでは判定側の契約 (空 summary → `Unchanged`) を、
/// 警告文字列が混ざった場合と対にして固定する。
#[test]
fn a_stderr_warning_must_not_be_mistaken_for_a_change() {
    let clean = judge_tree_change(Some("abc123"), |_| Ok(String::new()));
    assert_eq!(clean, TreeChange::Unchanged);

    let contaminated = judge_tree_change(Some("abc123"), |_| {
        Ok("Warning: unrecognized config option\n".to_string())
    });
    assert_eq!(
        contaminated,
        TreeChange::Changed,
        "判定側は非空を Changed と読む。だから I/O 層が stderr を混ぜてはいけない"
    );
}

/// 逆方向: takt が実際に変更した場合は従来どおり警告する。
#[test]
fn a_modified_tree_is_changed() {
    let verdict = judge_tree_change(Some("abc123"), |_| Ok("M src/lib.rs\n".to_string()));
    assert_eq!(verdict, TreeChange::Changed);
}

/// `pre_takt_cid` が None のときは「変更された」と言わない (2026-08-25 ユーザー決定)。
/// **fetch は呼ばれない** — 基準が無い状態で jj を叩いても意味がないため。
#[test]
fn a_missing_base_commit_is_undeterminable_and_skips_the_diff() {
    let verdict = judge_tree_change(None, |_| panic!("基準が無いのに jj を呼んではいけない"));
    assert!(matches!(verdict, TreeChange::Undeterminable { .. }));
    assert_ne!(verdict, TreeChange::Changed);
}

/// jj の失敗も「判定不能」であって「変更された」ではない。
#[test]
fn a_failed_diff_is_undeterminable() {
    let verdict = judge_tree_change(Some("abc123"), |_| Err("jj: no such revision".to_string()));
    let TreeChange::Undeterminable { reason } = &verdict else {
        panic!("判定不能に倒れていない: {verdict:?}");
    };
    assert!(reason.contains("no such revision"), "失敗理由を握り潰さない: {reason}");
}

/// **判定不能では片付けコマンドを案内しない** — 額面どおり実行すると PR の中身を失うため
/// (順位 490 の実害)。
#[test]
fn the_undeterminable_message_never_suggests_discarding_work() {
    let message = tree_change_message(&TreeChange::Undeterminable {
        reason: "取得失敗".to_string(),
    });
    assert!(!message.contains("jj abandon"), "{message}");
    assert!(!message.contains("jj restore"), "{message}");
    assert!(message.contains("判定できません"), "{message}");
}

/// 変更ありのときは従来どおり片付けを案内してよい (材料が揃っているため)。
#[test]
fn the_changed_message_keeps_the_cleanup_guidance() {
    let message = tree_change_message(&TreeChange::Changed);
    assert!(message.starts_with("[warn]"), "{message}");
    assert!(message.contains("jj abandon"), "{message}");
}

/// 変更なしは warning ではなく state ログ (run ログの noise にしない)。
#[test]
fn the_unchanged_message_is_not_a_warning() {
    let message = tree_change_message(&TreeChange::Unchanged);
    assert!(message.starts_with("[state]"), "{message}");
}
