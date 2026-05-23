use crate::config::load_config;
use crate::fix_commit::{create_fix_commit, FixCommitState};
use crate::lock::{acquire as acquire_lock, LockResult};
use crate::log::{log_info, truncate_safe};
use crate::stages::collect::collect_findings;
use crate::stages::poll::run_poll_loop;
use crate::stages::repush::execute_repush_flow;
use crate::stages::takt::run_takt;
use crate::state::{read_state, write_state, PrMonitorState};
use crate::util::{get_pr_info, utc_now_iso8601, PrInfo};

// ─── 監視開始 (sequential chain) ───

pub(crate) fn start_monitoring(pr_info: &PrInfo) -> i32 {
    start_monitoring_inner(pr_info, false)
}

/// Bb-2: wakeup invocation 用 (state リセットを skip し前回の next_wakeup_at_unix /
/// review_recheck_count を保持したまま single-iteration check を実行する)。
pub(crate) fn start_monitoring_wakeup(pr_info: &PrInfo) -> i32 {
    start_monitoring_inner(pr_info, true)
}

fn start_monitoring_inner(pr_info: &PrInfo, is_wakeup: bool) -> i32 {
    let config = load_config();
    if !config.monitor.enabled {
        log_info("監視は設定で無効化されています");
        return 0;
    }

    let lock_guard = match try_acquire_monitor_lock() {
        AcquireResult::Acquired(g) => g,
        AcquireResult::Skip => return 0,
    };

    let pr_label = pr_info
        .pr_number
        .map(|n| format!("PR #{}", n))
        .unwrap_or_else(|| "PR".to_string());

    init_or_resume_state(pr_info, is_wakeup, &pr_label);

    let poll_result = run_poll_loop(&config, pr_info, is_wakeup);
    log_info(&format!(
        "ポーリング完了: action={}, summary={}",
        poll_result.action, poll_result.summary
    ));

    let takt_outcome = run_takt_stage(&poll_result, pr_info, &config);
    finalize_repush(&takt_outcome, &config, &pr_label);

    print_report(&poll_result, &pr_label);

    drop(lock_guard);
    0
}

enum AcquireResult {
    Acquired(Option<crate::lock::MonitorLock>),
    Skip,
}

fn try_acquire_monitor_lock() -> AcquireResult {
    match acquire_lock("start_monitoring") {
        LockResult::Acquired(lock) => AcquireResult::Acquired(Some(lock)),
        LockResult::Busy {
            holder_pid,
            holder_age_secs,
        } => {
            log_info(&format!(
                "[lock] 別の cli-pr-monitor が走行中 (pid={}, age={}s)、本セッションは skip",
                holder_pid, holder_age_secs
            ));
            AcquireResult::Skip
        }
        LockResult::Unavailable { reason } => {
            log_info(&format!(
                "[lock] lock 取得不可 (lock なしで継続): {}",
                reason
            ));
            AcquireResult::Acquired(None)
        }
    }
}

fn init_or_resume_state(pr_info: &PrInfo, is_wakeup: bool, pr_label: &str) {
    if is_wakeup {
        log_info(&format!("{} の監視を再開 (wakeup)", pr_label));
        return;
    }
    log_info(&format!("{} の監視を開始", pr_label));
    let mut init_state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        pr_info.push_time.clone().unwrap_or_else(utc_now_iso8601),
    );
    init_state.fix_push_time = pr_info.fix_push_time.clone();
    if let Err(e) = write_state(&init_state) {
        log_info(&format!("[state] 初期化書き込み失敗 (継続): {}", e));
    }
}

struct TaktOutcome {
    takt_succeeded: bool,
    has_coderabbit_findings: bool,
    pre_takt_cid: Option<String>,
    fix_state: FixCommitState,
}

fn run_takt_stage(
    poll_result: &crate::stages::poll::PollResult,
    pr_info: &PrInfo,
    config: &crate::config::Config,
) -> TaktOutcome {
    let has_coderabbit_findings = !poll_result.findings.is_empty()
        || poll_result
            .coderabbit
            .as_ref()
            .map(|c| c.new_comments > 0 || c.unresolved_threads.unwrap_or(0) > 0)
            .unwrap_or(false);

    let mut outcome = TaktOutcome {
        takt_succeeded: false,
        has_coderabbit_findings,
        pre_takt_cid: None,
        fix_state: FixCommitState::None,
    };

    if !has_coderabbit_findings {
        return outcome;
    }
    if !collect_findings(poll_result) {
        log_info("review-comments.json 書き出し失敗 (takt 分析をスキップ)");
        return outcome;
    }
    if poll_result.rate_limit.is_some() {
        log_info(
            "[rate_limit] CR rate-limit が active のため post-pr-review takt invoke を skip \
             (stale findings の空打ち回避、#C-3)",
        );
        return outcome;
    }
    let Some(takt_config) = &config.takt else {
        log_info("takt 設定なし: AI 分析をスキップ");
        return outcome;
    };

    invoke_takt_into_outcome(&mut outcome, takt_config, pr_info, &poll_result.findings);
    outcome
}

fn invoke_takt_into_outcome(
    outcome: &mut TaktOutcome,
    takt_config: &crate::config::TaktConfig,
    pr_info: &PrInfo,
    findings: &[lib_report_formatter::Finding],
) {
    outcome.fix_state = create_fix_commit(pr_info.pr_number, findings);
    outcome.pre_takt_cid = crate::runner::capture_commit_id();
    log_info(&format!(
        "[state] pre_takt_commit_id: {:?}",
        outcome.pre_takt_cid
    ));
    outcome.takt_succeeded = run_takt(takt_config);
    log_info(&format!(
        "[state] takt_succeeded: {}",
        outcome.takt_succeeded
    ));
    if !outcome.takt_succeeded {
        log_info("takt ワークフロー失敗 (非致命的: ポーリング結果はそのまま報告)");
    }
}

fn finalize_repush(outcome: &TaktOutcome, config: &crate::config::Config, pr_label: &str) {
    if outcome.takt_succeeded && outcome.has_coderabbit_findings {
        execute_repush_flow(
            &config.fix,
            pr_label,
            outcome.pre_takt_cid.as_deref(),
            &outcome.fix_state,
        );
    } else if let FixCommitState::Created { commit_id } = &outcome.fix_state {
        crate::fix_commit::try_abandon_empty_fix_commit("takt 未完了:", Some(commit_id));
    }
}

// ─── 監視のみモード ───

pub(crate) fn run_monitor_only() -> i32 {
    let config = load_config();
    if !config.monitor.enabled {
        return 0;
    }

    let mut pr_info = get_pr_info();
    if pr_info.pr_number.is_none() {
        log_info("PR が存在しないため、監視をスキップします");
        return 0;
    }

    log_info("監視のみモード (既存 PR 検出)");

    if let Some(resume_push_time) = detect_wakeup_resume(&pr_info) {
        log_info(&format!(
            "[wakeup] 前回 park の next_wakeup_at_unix が経過 → state を継続 (started_at={})",
            resume_push_time
        ));
        pr_info.push_time = Some(resume_push_time.clone());
        pr_info.fix_push_time = resume_fix_push_time_or_started_at(&resume_push_time);
        start_monitoring_wakeup(&pr_info)
    } else {
        let now = utc_now_iso8601();
        pr_info.push_time = Some(now.clone());
        pr_info.fix_push_time = Some(now);
        start_monitoring(&pr_info)
    }
}

/// 順位 141: wakeup resume 経路で state から `fix_push_time` を取り出す。
/// legacy state (本フィールド未設定) では `started_at` に fallback して挙動を維持する。
fn resume_fix_push_time_or_started_at(started_at_fallback: &str) -> Option<String> {
    read_state()
        .and_then(|s| s.fix_push_time)
        .or_else(|| Some(started_at_fallback.to_string()))
}

/// Bb-2: 既存 state file が「自分の PR / repo / head commit の wakeup 待ち」かを判定し、
/// 該当すれば push_time として継続用 ISO 8601 (state.started_at) を返す。
///
/// CR Major #1 fix (Bb-2 PR #114 review): 同一 PR でも新 commit が push されれば head_commit
/// が変わるため、stored vs current head 一致も check する。head 不一致なら fresh push 扱い。
fn detect_wakeup_resume(pr_info: &PrInfo) -> Option<String> {
    let state = read_state()?;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if !should_resume_wakeup(&state, pr_info, now_unix) {
        return None;
    }
    Some(state.started_at)
}

/// CR Major #1 fix: detect_wakeup_resume の判定 invariant を pure に分離してテスト可能にする。
///
/// resume 条件 (全て true):
///   1. state.pr == pr_info.pr_number AND state.repo == pr_info.repo
///   2. state.next_wakeup_at_unix が Some かつ now を経過
///   3. state.head_commit が Some かつ pr_info.head_commit と一致
///
/// 1 つでも不一致なら resume せず fresh push 経路に倒す。legacy state (head_commit None) は
/// 自動的に 3 で False になり安全側 (fresh push) に倒れる。
fn should_resume_wakeup(state: &PrMonitorState, pr_info: &PrInfo, now_unix: i64) -> bool {
    if state.pr != pr_info.pr_number || state.repo != pr_info.repo {
        return false;
    }
    let Some(wakeup_at) = state.next_wakeup_at_unix else {
        return false;
    };
    if wakeup_at > now_unix {
        return false;
    }
    match (state.head_commit.as_deref(), pr_info.head_commit.as_deref()) {
        (Some(stored), Some(current)) => stored == current,
        _ => false,
    }
}

// ─── レポート出力 ───

fn print_report(result: &crate::stages::poll::PollResult, pr_label: &str) {
    let ci_status = result
        .ci
        .as_ref()
        .map(|c| c.overall.as_str())
        .unwrap_or("unknown");
    let cr_comments = result
        .coderabbit
        .as_ref()
        .map(|c| c.new_comments)
        .unwrap_or(0);
    let cr_threads = result
        .coderabbit
        .as_ref()
        .and_then(|c| c.unresolved_threads)
        .unwrap_or(0);

    println!();
    println!("## Review Report ({})", pr_label);
    println!();
    println!(
        "CI: {} | CodeRabbit: 新規コメント{}件, 未解決スレッド{}件",
        ci_status, cr_comments, cr_threads
    );
    println!("action: {} | summary: {}", result.action, result.summary);
    println!();
    println!("**判定**: {}", compute_verdict(result));

    if !result.findings.is_empty() {
        print_findings_table(&result.findings);
    }
}

fn compute_verdict(result: &crate::stages::poll::PollResult) -> &'static str {
    match result.action.as_str() {
        "parked_rate_limit" => {
            return "CodeRabbit rate-limit のため wakeup を予約 (上記 PARK signal 参照)";
        }
        "parked_review_recheck" => {
            return "review 完了待ちのため wakeup を予約 (上記 PARK signal 参照)";
        }
        _ => {}
    }

    if let Some(cr) = &result.coderabbit {
        if cr.review_state == "not_found" || cr.review_state == "pending" {
            return "CodeRabbit review が未完了のため、判定を保留します";
        }
    }

    let critical_major = result
        .findings
        .iter()
        .filter(|f| {
            let s = f.severity.to_lowercase();
            s == "critical" || s == "high" || s == "major"
        })
        .count();

    if critical_major > 0 {
        "修正が必要な指摘があります"
    } else if !result.findings.is_empty() {
        "重大な問題は見つかりませんでした。軽微な改善提案があります"
    } else {
        "問題は見つかりませんでした"
    }
}

fn print_findings_table(findings: &[lib_report_formatter::Finding]) {
    println!();
    println!("| # | Source | Severity | File (Line) | Issue | Suggestion |");
    println!("|---|--------|----------|-------------|-------|------------|");
    for (i, f) in findings.iter().enumerate() {
        let suggestion = if f.suggestion.chars().count() > 80 {
            format!("{}...", truncate_safe(&f.suggestion, 77))
        } else {
            f.suggestion.clone()
        };
        println!(
            "| {} | {} | {} | {} ({}) | {} | {} |",
            i + 1,
            f.source,
            f.severity,
            f.file,
            f.line,
            f.issue,
            suggestion
        );
    }
}

#[cfg(test)]
mod tests {
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

    fn make_park_state(pr: u64, repo: &str, wakeup_at: i64, head: Option<&str>) -> PrMonitorState {
        let mut s = PrMonitorState::new(Some(pr), Some(repo.into()), "t".into());
        s.next_wakeup_at_unix = Some(wakeup_at);
        s.wakeup_reason = Some("review_recheck".into());
        s.head_commit = head.map(String::from);
        s
    }

    #[test]
    fn should_resume_wakeup_true_when_pr_repo_head_match_and_due() {
        let state = make_park_state(42, "o/r", 100, Some("abc1234"));
        let pr_info = make_pr_info(42, "o/r", Some("abc1234"));
        assert!(should_resume_wakeup(&state, &pr_info, 200));
    }

    #[test]
    fn should_resume_wakeup_false_when_head_differs() {
        let state = make_park_state(42, "o/r", 100, Some("abc1234"));
        let pr_info = make_pr_info(42, "o/r", Some("def5678"));
        assert!(
            !should_resume_wakeup(&state, &pr_info, 200),
            "CR Major #1: head 不一致なら fresh push 経路に倒す"
        );
    }

    #[test]
    fn should_resume_wakeup_false_when_state_head_missing() {
        let state = make_park_state(42, "o/r", 100, None);
        let pr_info = make_pr_info(42, "o/r", Some("abc1234"));
        assert!(
            !should_resume_wakeup(&state, &pr_info, 200),
            "legacy state (head_commit None) は安全側で fresh push 扱い"
        );
    }

    #[test]
    fn should_resume_wakeup_false_when_pr_info_head_missing() {
        let state = make_park_state(42, "o/r", 100, Some("abc1234"));
        let pr_info = make_pr_info(42, "o/r", None);
        assert!(
            !should_resume_wakeup(&state, &pr_info, 200),
            "current head 取得失敗時は安全側で fresh push 扱い"
        );
    }

    #[test]
    fn should_resume_wakeup_false_when_pr_or_repo_differs() {
        let state = make_park_state(42, "o/r", 100, Some("abc1234"));
        let other_pr = make_pr_info(99, "o/r", Some("abc1234"));
        let other_repo = make_pr_info(42, "x/y", Some("abc1234"));
        assert!(!should_resume_wakeup(&state, &other_pr, 200));
        assert!(!should_resume_wakeup(&state, &other_repo, 200));
    }

    #[test]
    fn should_resume_wakeup_false_when_wakeup_in_future() {
        let state = make_park_state(42, "o/r", 1000, Some("abc1234"));
        let pr_info = make_pr_info(42, "o/r", Some("abc1234"));
        assert!(
            !should_resume_wakeup(&state, &pr_info, 100),
            "next_wakeup_at_unix が未来ならまだ resume しない"
        );
    }

    #[test]
    fn should_resume_wakeup_false_when_next_wakeup_unset() {
        let mut state = make_park_state(42, "o/r", 100, Some("abc1234"));
        state.next_wakeup_at_unix = None;
        let pr_info = make_pr_info(42, "o/r", Some("abc1234"));
        assert!(!should_resume_wakeup(&state, &pr_info, 200));
    }

    use crate::stages::poll::PollResult;
    use crate::state::CodeRabbitState;
    use lib_report_formatter::Finding;

    fn poll_result(
        action: &str,
        review_state: Option<&str>,
        findings: Vec<Finding>,
    ) -> PollResult {
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

    const VERDICT_PARK_RATE_LIMIT: &str =
        "CodeRabbit rate-limit のため wakeup を予約 (上記 PARK signal 参照)";
    const VERDICT_PARK_REVIEW: &str = "review 完了待ちのため wakeup を予約 (上記 PARK signal 参照)";
    const VERDICT_REVIEW_PENDING: &str = "CodeRabbit review が未完了のため、判定を保留します";
    const VERDICT_NO_PROBLEMS: &str = "問題は見つかりませんでした";
    const VERDICT_MINOR: &str = "重大な問題は見つかりませんでした。軽微な改善提案があります";
    const VERDICT_CRITICAL: &str = "修正が必要な指摘があります";

    #[test]
    fn verdict_park_rate_limit_takes_precedence_over_review_state() {
        let r = poll_result("parked_rate_limit", Some("not_found"), vec![]);
        assert_eq!(compute_verdict(&r), VERDICT_PARK_RATE_LIMIT);
    }

    #[test]
    fn verdict_park_review_recheck_takes_precedence_over_findings() {
        let r = poll_result("parked_review_recheck", Some("not_found"), vec![finding("critical")]);
        assert_eq!(compute_verdict(&r), VERDICT_PARK_REVIEW);
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

    /// PR_MONITOR_STATE_FILE_OVERRIDE は process-global env var のため、
    /// override 設定 / 解除を test 並行実行で race させない serial guard。
    fn env_override_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    /// 順位 141: `resume_fix_push_time_or_started_at` Case A —
    /// state に `fix_push_time` が設定済みの場合、fallback の `started_at` ではなく
    /// state の値が返されることを検証する。
    #[test]
    fn resume_returns_fix_push_time_from_state_when_set() {
        let _guard = env_override_lock();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut s = PrMonitorState::new(Some(1), None, "t".into());
        s.fix_push_time = Some("2026-05-22T06:06:00Z".into());
        std::fs::write(tmp.path(), serde_json::to_string(&s).unwrap()).unwrap();
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", tmp.path());
        let result = resume_fix_push_time_or_started_at("2026-05-22T06:00:00Z");
        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");
        assert_eq!(
            result.as_deref(),
            Some("2026-05-22T06:06:00Z"),
            "state に fix_push_time がある場合、fallback の started_at ではなく state の値が返る"
        );
    }
}
