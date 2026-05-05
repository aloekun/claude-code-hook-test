use lib_report_formatter::Finding;
use std::time::Duration;

use crate::config::{Config, MonitorConfig, RateLimitConfig, DEFAULT_CHECK_TIMEOUT_SECS};
use crate::log::{log_info, truncate_safe};
use crate::runner::{checker_exe_path, run_cmd_direct, run_gh_quiet};
use crate::state::{
    read_state, update_state_from_check_result, write_state, CiState, CodeRabbitState,
    PrMonitorState, RateLimitState,
};
use crate::util::{utc_now_iso8601, PrInfo};

pub(crate) struct PollResult {
    pub(crate) action: String,
    pub(crate) summary: String,
    pub(crate) ci: Option<CiState>,
    pub(crate) coderabbit: Option<CodeRabbitState>,
    pub(crate) findings: Vec<Finding>,
    pub(crate) check_output: Option<serde_json::Value>,
    /// 終了時点で rate-limit が active なら Some。caller (monitor.rs) は
    /// `is_some()` を見て post-pr-review takt invoke を skip する (#C-3)。
    /// rate-limit 中は CR の fresh review が得られないため、stale な findings に
    /// 対する takt 分析は空打ちになる。
    pub(crate) rate_limit: Option<RateLimitState>,
}

struct PollContext<'a> {
    checker: &'a std::path::Path,
    push_time: &'a str,
    pr_info: &'a PrInfo,
    rate_limit_config: &'a RateLimitConfig,
    start: std::time::Instant,
    max_duration: u64,
    skip_ci: bool,
    skip_coderabbit: bool,
}

/// in-process 同期ポーリングループ (daemon.rs の同期版)
pub(crate) fn run_poll_loop(full_config: &Config, pr_info: &PrInfo) -> PollResult {
    let config: &MonitorConfig = &full_config.monitor;
    let poll_interval = config.poll_interval_secs;

    let checker = checker_exe_path();
    if !checker.exists() {
        log_info(&format!(
            "check-ci-coderabbit.exe が見つかりません: {}",
            checker.display()
        ));
        return error_poll_result("check-ci-coderabbit.exe が見つかりません");
    }

    let ctx = PollContext {
        checker: &checker,
        push_time: pr_info
            .push_time
            .as_deref()
            .unwrap_or("1970-01-01T00:00:00Z"),
        pr_info,
        rate_limit_config: &full_config.rate_limit,
        start: std::time::Instant::now(),
        max_duration: config.max_duration_secs,
        skip_ci: !config.check_ci,
        skip_coderabbit: !config.check_coderabbit,
    };

    loop {
        if let Some(terminal) = run_one_iteration(&ctx) {
            return terminal;
        }
        std::thread::sleep(Duration::from_secs(poll_interval));
    }
}

fn run_one_iteration(ctx: &PollContext<'_>) -> Option<PollResult> {
    let args = build_checker_args(ctx.push_time, ctx.pr_info);
    let result = match invoke_checker(ctx.checker, &args) {
        Ok(r) => r,
        Err(pr) => return Some(*pr),
    };
    let mut state = build_state_for_iteration(
        ctx.pr_info,
        ctx.push_time,
        &result,
        ctx.skip_ci,
        ctx.skip_coderabbit,
    );
    log_info(&format!(
        "ポーリング: action={}, summary={}",
        state.action, state.summary
    ));

    if state.action != "continue_monitoring" {
        return Some(make_terminal_result(state, result));
    }

    if let Some(terminal) =
        handle_rate_limit_branch(&mut state, ctx.rate_limit_config, ctx.pr_info, &result)
    {
        return Some(terminal);
    }

    if ctx.start.elapsed() >= Duration::from_secs(ctx.max_duration) {
        log_info(&format!("監視タイムアウト ({}秒)", ctx.max_duration));
        return Some(make_timeout_result(state, ctx.max_duration, result));
    }

    None
}

fn build_checker_args(push_time: &str, pr_info: &PrInfo) -> Vec<String> {
    let mut args: Vec<String> = vec!["--push-time".into(), push_time.into()];
    if let Some(ref repo) = pr_info.repo {
        args.push("--repo".into());
        args.push(repo.clone());
    }
    if let Some(pr) = pr_info.pr_number {
        args.push("--pr".into());
        args.push(pr.to_string());
    }
    args
}

fn invoke_checker(
    checker: &std::path::Path,
    args: &[String],
) -> Result<serde_json::Value, Box<PollResult>> {
    let (success, output) = run_cmd_direct(
        &checker.to_string_lossy(),
        &[],
        args,
        DEFAULT_CHECK_TIMEOUT_SECS,
    );

    if !success {
        log_info(&format!("checker 失敗: {}", truncate_safe(&output, 200)));
        return Err(Box::new(error_poll_result(&format!(
            "check-ci-coderabbit.exe 失敗: {}",
            truncate_safe(&output, 200)
        ))));
    }

    serde_json::from_str::<serde_json::Value>(&output).map_err(|e| {
        log_info(&format!("JSON パース失敗: {}", e));
        Box::new(error_poll_result(&format!(
            "checker 出力の JSON パース失敗: {}",
            e
        )))
    })
}

fn error_poll_result(summary: &str) -> PollResult {
    PollResult {
        action: "error".into(),
        summary: summary.into(),
        ci: None,
        coderabbit: None,
        findings: Vec::new(),
        check_output: None,
        rate_limit: None,
    }
}

/// `PrMonitorState::new` は毎回 notified / rate_limit_retries を 0 リセットするため、
/// 既存 state から runtime-updated な値を読み戻して 1 iteration の base state を組む。
fn build_state_for_iteration(
    pr_info: &PrInfo,
    push_time: &str,
    result: &serde_json::Value,
    skip_ci: bool,
    skip_coderabbit: bool,
) -> PrMonitorState {
    let mut state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        push_time.to_string(),
    );
    update_state_from_check_result(&mut state, result);

    if let Some(existing) = read_state() {
        state.notified = existing.notified;
        state.rate_limit_retries = existing.rate_limit_retries;
        state.rate_limit_last_retriggered_at = existing.rate_limit_last_retriggered_at;
    }

    apply_skip_handling(&mut state, skip_ci, skip_coderabbit);
    state.last_checked = Some(utc_now_iso8601());
    let _ = write_state(&state);
    state
}

fn apply_skip_handling(state: &mut PrMonitorState, skip_ci: bool, skip_coderabbit: bool) {
    if skip_ci {
        state.ci = Some(CiState {
            overall: "skipped".into(),
            runs: vec![],
        });
    }
    if skip_coderabbit {
        state.coderabbit = Some(CodeRabbitState {
            review_state: "skipped".into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: None,
        });
        state.findings = Vec::new();
    }
    if skip_ci || skip_coderabbit {
        state.action = recompute_action(state, skip_ci, skip_coderabbit);
    }
}

fn make_terminal_result(state: PrMonitorState, result: serde_json::Value) -> PollResult {
    PollResult {
        action: state.action,
        summary: state.summary,
        ci: state.ci,
        coderabbit: state.coderabbit,
        findings: state.findings,
        check_output: Some(result),
        rate_limit: state.rate_limit,
    }
}

fn make_timeout_result(
    state: PrMonitorState,
    max_duration: u64,
    result: serde_json::Value,
) -> PollResult {
    PollResult {
        action: "timed_out".into(),
        summary: format!("監視タイムアウト ({}秒)", max_duration),
        ci: state.ci,
        coderabbit: state.coderabbit,
        findings: state.findings,
        check_output: Some(result),
        rate_limit: state.rate_limit,
    }
}

/// rate-limit 検出 branch を集約する。
///
/// dedup: 同一 rate-limit comment は iteration を跨いで残るため `comment_event_time`
/// で dedup する。dedup なしでは即時 retrigger を秒単位で繰り返し max_retries を浪費する。
/// CR が新たな rate-limit comment を投稿すると event_time が変わり再 handle 対象になる。
fn handle_rate_limit_branch(
    state: &mut PrMonitorState,
    rate_limit_config: &RateLimitConfig,
    pr_info: &PrInfo,
    result: &serde_json::Value,
) -> Option<PollResult> {
    let rl = state.rate_limit.clone()?;
    let already_handled =
        state.rate_limit_last_retriggered_at.as_deref() == Some(rl.comment_event_time.as_str());

    if already_handled {
        log_info(&format!(
            "[rate_limit] 同じ rate-limit comment ({}) は処理済み、retrigger スキップ",
            rl.comment_event_time
        ));
        return None;
    }

    if state.rate_limit_retries >= rate_limit_config.max_retries {
        log_info(&format!(
            "[rate_limit] max_retries={} 到達、自動 retry を停止",
            rate_limit_config.max_retries
        ));
        return Some(make_max_retries_result(state, result));
    }

    if !rate_limit_config.auto_retry_enabled {
        return None;
    }

    dispatch_rate_limit_outcome(state, &rl, pr_info, rate_limit_config.max_retries, result)
}

fn dispatch_rate_limit_outcome(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    max_retries: u32,
    result: &serde_json::Value,
) -> Option<PollResult> {
    match handle_rate_limit_retry(rl, state, pr_info, max_retries) {
        RateLimitOutcome::Posted => finalize_posted_retrigger(state, rl, result),
        RateLimitOutcome::Parked { wakeup_at_unix } => Some(finalize_parked(
            state,
            rl,
            pr_info,
            wakeup_at_unix,
            max_retries,
            result,
        )),
        RateLimitOutcome::Failed(e) => {
            log_info(&format!("[rate_limit] retrigger 失敗: {}", e));
            Some(make_action_required_result(
                state,
                result,
                &format!(
                    "rate-limit 自動 retry 失敗 ({})。手動で `@coderabbitai review` を投稿してください",
                    e
                ),
            ))
        }
    }
}

fn finalize_posted_retrigger(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    result: &serde_json::Value,
) -> Option<PollResult> {
    state.rate_limit_last_retriggered_at = Some(rl.comment_event_time.clone());
    if let Err(e) = write_state(state) {
        log_info(&format!(
            "[rate_limit] retrigger 後の state 永続化失敗、自動 retry を停止: {}",
            e
        ));
        return Some(make_action_required_result(
            state,
            result,
            &format!(
                "rate-limit retry 後の state 永続化に失敗 ({})。手動で `@coderabbitai review` の重複投稿に注意してください",
                e
            ),
        ));
    }
    None
}

fn finalize_parked(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    wakeup_at_unix: i64,
    max_retries: u32,
    result: &serde_json::Value,
) -> PollResult {
    state.action = "parked_rate_limit".into();
    state.next_wakeup_at_unix = Some(wakeup_at_unix);
    state.wakeup_reason = Some("rate_limit_retry".into());
    state.summary = format!(
        "CodeRabbit rate-limit: wakeup を {}m{}s 後に予約 (PARK signal 参照)",
        rl.wait_minutes, rl.wait_seconds
    );
    if let Err(e) = write_state(state) {
        log_info(&format!("[rate_limit] park state 永続化失敗: {}", e));
    }
    let signal = format_park_signal(state, rl, pr_info, max_retries);
    println!("{}", signal);

    PollResult {
        action: state.action.clone(),
        summary: state.summary.clone(),
        ci: state.ci.clone(),
        coderabbit: state.coderabbit.clone(),
        findings: state.findings.clone(),
        check_output: Some(result.clone()),
        rate_limit: state.rate_limit.clone(),
    }
}

fn make_max_retries_result(state: &PrMonitorState, result: &serde_json::Value) -> PollResult {
    let summary = format!(
        "CodeRabbit rate-limit が {} 回再試行後も継続。手動で `@coderabbitai review` を投稿してください",
        state.rate_limit_retries
    );
    make_action_required_result(state, result, &summary)
}

fn make_action_required_result(
    state: &PrMonitorState,
    result: &serde_json::Value,
    summary: &str,
) -> PollResult {
    PollResult {
        action: "action_required".into(),
        summary: summary.into(),
        ci: state.ci.clone(),
        coderabbit: state.coderabbit.clone(),
        findings: state.findings.clone(),
        check_output: Some(result.clone()),
        rate_limit: state.rate_limit.clone(),
    }
}

/// `handle_rate_limit_retry` の outcome 種別 (Bb-1, Bundle b PR-1)。
///
/// rate-limit 検出時の振る舞いは sleep 廃止 + park 化に切り替わった:
///
/// - `Posted`: reset 時刻が既に過去 (`sleep_secs <= 0`) のため、その場で
///   `@coderabbitai review` を投稿し `rate_limit_retries` をインクリメント。
///   caller は polling を継続する (現状挙動と同じ)。
/// - `Parked`: reset 時刻が未来。同プロセス内で sleep せず、caller に「state に
///   `next_wakeup_at_unix` を保存し PARK signal を stdout に出して終端 action で
///   exit せよ」と通知する。実 wakeup は CronCreate (`durable: true`) 経由で
///   `cli-pr-monitor.exe --monitor-only` を再 invoke する流れ (ADR-030 L1+L2 を踏襲)。
/// - `Failed`: PR 番号未確定 / gh post 失敗。caller は state を更新せず
///   action_required で抜ける。
pub(crate) enum RateLimitOutcome {
    Posted,
    Parked { wakeup_at_unix: i64 },
    Failed(String),
}

/// rate-limit 検出時の outcome を返す。
///
/// `until_unix_secs > now`: park (sleep しない、caller が wakeup 予約を依頼)
/// `until_unix_secs <= now`: その場で `@coderabbitai review` を投稿
fn handle_rate_limit_retry(
    rl: &crate::state::RateLimitState,
    state: &mut PrMonitorState,
    pr_info: &PrInfo,
    max_retries: u32,
) -> RateLimitOutcome {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let sleep_secs = (rl.until_unix_secs - now_unix).max(0) as u64;

    let Some(pr) = pr_info.pr_number else {
        return RateLimitOutcome::Failed("PR 番号未確定のため retrigger スキップ".into());
    };

    if sleep_secs > 0 {
        log_info(&format!(
            "[rate_limit] reset まで {}秒 (wait={}m{}s + 60s buffer)、Park で wakeup 要求 (retry 候補={}/{})",
            sleep_secs,
            rl.wait_minutes,
            rl.wait_seconds,
            state.rate_limit_retries + 1,
            max_retries
        ));
        return RateLimitOutcome::Parked {
            wakeup_at_unix: rl.until_unix_secs,
        };
    }

    post_review_immediately(pr, state)
}

/// `sleep_secs <= 0` 経路の即時 `@coderabbitai review` 投稿を担う helper。
fn post_review_immediately(pr: u64, state: &mut PrMonitorState) -> RateLimitOutcome {
    log_info(&format!(
        "[rate_limit] reset 時刻は既に過去、即時 retrigger (retry={})",
        state.rate_limit_retries + 1
    ));

    let pr_str = pr.to_string();
    if run_gh_quiet(&["pr", "comment", &pr_str, "--body", "@coderabbitai review"]).is_none() {
        return RateLimitOutcome::Failed(format!("gh pr comment 投稿失敗 (PR #{})", pr));
    }

    log_info(&format!(
        "[rate_limit] @coderabbitai review を投稿 (PR #{}, retry={})",
        pr,
        state.rate_limit_retries + 1
    ));

    state.rate_limit_retries += 1;
    RateLimitOutcome::Posted
}

/// PARK signal を stdout に書き出すための pure 関数 (Bb-1)。
///
/// Claude Code が stdout を読み、含まれる `CronCreate(...)` 呼び出しテンプレートに
/// 従って `durable: true` で wakeup を予約する想定。`reset_at_iso_utc` は UTC な
/// ため、cron 式は Claude が local timezone に変換して計算する (chrono 依存を
/// 避けるため Rust 側では cron 式を組まない)。
pub(crate) fn format_park_signal(
    state: &PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    max_retries: u32,
) -> String {
    let pr = pr_info
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let repo = pr_info.repo.as_deref().unwrap_or("?");
    let reset_iso = if rl.until_unix_secs > 0 {
        lib_pending_file::epoch_secs_to_iso8601(rl.until_unix_secs as u64)
    } else {
        "?".into()
    };
    let wait_total_secs = rl.wait_minutes * 60 + rl.wait_seconds;
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "cli-pr-monitor.exe".into());
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into());
    let retry_attempt = state.rate_limit_retries + 1;

    format!(
        "[PR_MONITOR_PARK]
pr: {pr}
repo: {repo}
reset_at_unix: {until}
reset_at_iso_utc: {reset_iso}
wait_total_seconds: {wait_total_secs}
retry_count: {retry_attempt}
max_retries: {max_retries}
exe: {exe}
cwd: {cwd}

ACTION REQUIRED: please schedule one-shot wakeup using CronCreate.

CronCreate({{
  cron: <compute from reset_at_iso_utc in your local timezone, format \"M H DoM Mon DoW\">,
  recurring: false,
  durable: true,
  prompt: \"Wakeup: rate-limit retry for PR #{pr} ({repo}). cd {cwd} && {exe} --monitor-only\"
}})
[/PR_MONITOR_PARK]",
        until = rl.until_unix_secs,
    )
}

/// skip 適用後に、有効なチェックだけを見て action を再導出する
fn recompute_action(state: &PrMonitorState, skip_ci: bool, skip_coderabbit: bool) -> String {
    let ci_ok = skip_ci
        || state
            .ci
            .as_ref()
            .map(|c| c.overall == "success" || c.overall == "skipped")
            .unwrap_or(false);

    let cr_ok = skip_coderabbit
        || state
            .coderabbit
            .as_ref()
            .map(|c| {
                c.review_state == "skipped"
                    || (c.new_comments == 0 && c.unresolved_threads.unwrap_or(0) == 0)
            })
            .unwrap_or(false);

    let ci_pending = !skip_ci
        && state
            .ci
            .as_ref()
            .map(|c| c.overall == "pending")
            .unwrap_or(true);

    let cr_pending = !skip_coderabbit
        && state
            .coderabbit
            .as_ref()
            .map(|c| c.review_state == "not_found" || c.review_state == "pending")
            .unwrap_or(true);

    if ci_pending || cr_pending {
        return "continue_monitoring".into();
    }

    let ci_failed = !skip_ci
        && state
            .ci
            .as_ref()
            .map(|c| c.overall == "failure")
            .unwrap_or(false);

    let cr_action_required = !skip_coderabbit
        && state
            .coderabbit
            .as_ref()
            .map(|c| c.new_comments > 0 || c.unresolved_threads.unwrap_or(0) > 0)
            .unwrap_or(false);

    if ci_failed {
        "stop_monitoring_failure".into()
    } else if cr_action_required {
        "action_required".into()
    } else if ci_ok && cr_ok {
        "stop_monitoring_success".into()
    } else {
        // Fallback: keep original action
        state.action.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RateLimitState;

    #[test]
    fn rate_limit_state_persists_retries_across_polls() {
        // simulate state.json round-trip behavior: 1 iteration で incremented した
        // retries が次 iteration で復元されることを検証
        let tmp = std::env::temp_dir().join(format!("test-rl-retries-{}.json", std::process::id()));
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.rate_limit_retries = 2;
        state.rate_limit = Some(RateLimitState {
            until_unix_secs: 1_735_689_600,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 13,
        });
        crate::state::write_state_to(&tmp, &state).unwrap();

        let loaded = crate::state::read_state_from(&tmp).unwrap();
        assert_eq!(loaded.rate_limit_retries, 2);
        assert_eq!(
            loaded.rate_limit.as_ref().unwrap().until_unix_secs,
            1_735_689_600
        );

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn rate_limit_default_config_allows_retry_within_limit() {
        let cfg = RateLimitConfig::default();
        assert!(cfg.auto_retry_enabled);
        assert_eq!(cfg.max_retries, 3);
        // 2 retries 後: 2 < 3 で auto_retry_enabled パスを通る
        assert!(2 < cfg.max_retries);
        // 3 retries 後: 3 >= 3 で max 到達 → action_required で抜ける
        assert!(3 >= cfg.max_retries);
    }

    /// 同じ rate-limit comment が iteration 跨ぎで残った場合に dedup が働くことを検証する。
    ///
    /// シナリオ (advisor 発見のバグ):
    /// - Iter 1: comment A, retries=0, last_retriggered=None → handle 対象
    /// - Iter 2: 同じ comment A still in PR, last_retriggered=A → 即時 retrigger を skip
    /// - Iter 3: CR が新たな rate-limit comment B を投稿, last_retriggered=A != B → 再 handle 対象
    ///
    /// dedup なしだと Iter 2/3 で sleep_secs=0 となり数秒で max_retries を消費する。
    #[test]
    fn rate_limit_dedup_skips_repeated_comment() {
        let comment_a = "2026-04-30T00:00:00Z";
        let comment_b = "2026-04-30T00:30:00Z";

        // Iter 1: 初回 detection (last_retriggered=None)
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        let rl_a = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: comment_a.into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let already_handled_iter1 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_a.comment_event_time.as_str());
        assert!(
            !already_handled_iter1,
            "Iter 1: 初回 detection は handle されるべき"
        );

        // Iter 1 で handle した結果を simulate
        state.rate_limit_retries = 1;
        state.rate_limit_last_retriggered_at = Some(comment_a.into());

        // Iter 2: 同じ comment が PR に残っている (CR レビュー再開待ち)
        let already_handled_iter2 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_a.comment_event_time.as_str());
        assert!(
            already_handled_iter2,
            "Iter 2: 同じ comment は dedup で skip されるべき"
        );

        // Iter 3: CR が新たな rate-limit comment を投稿
        let rl_b = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: comment_b.into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let already_handled_iter3 = state.rate_limit_last_retriggered_at.as_deref()
            == Some(rl_b.comment_event_time.as_str());
        assert!(
            !already_handled_iter3,
            "Iter 3: 新 comment は再度 handle 対象"
        );
    }

    /// state.json round-trip で rate_limit_last_retriggered_at が persistence される。
    #[test]
    fn rate_limit_last_retriggered_at_persists_across_polls() {
        let tmp =
            std::env::temp_dir().join(format!("test-rl-last-handled-{}.json", std::process::id()));
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.rate_limit_last_retriggered_at = Some("2026-04-30T00:00:00Z".into());
        crate::state::write_state_to(&tmp, &state).unwrap();

        let loaded = crate::state::read_state_from(&tmp).unwrap();
        assert_eq!(
            loaded.rate_limit_last_retriggered_at.as_deref(),
            Some("2026-04-30T00:00:00Z")
        );

        let _ = std::fs::remove_file(&tmp);
    }

    /// Bb-1: reset 時刻が未来の場合、`handle_rate_limit_retry` は Parked を返し
    /// state.rate_limit_retries を変更しない (実 retry 計上は wakeup 経由で post 投稿後)。
    #[test]
    fn rate_limit_retry_returns_parked_when_reset_in_future() {
        let future_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let rl = RateLimitState {
            until_unix_secs: future_unix,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 10,
            wait_seconds: 0,
        };
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
        };

        let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info, 3);
        match outcome {
            RateLimitOutcome::Parked { wakeup_at_unix } => {
                assert_eq!(wakeup_at_unix, future_unix);
            }
            _ => panic!("expected Parked outcome for future reset, got other variant"),
        }
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// Bb-1: PR 番号未確定の場合、`handle_rate_limit_retry` は Failed を返し
    /// state を変更しない (caller は action_required で抜ける)。
    #[test]
    fn rate_limit_retry_returns_failed_when_pr_number_missing() {
        let past_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 60;
        let rl = RateLimitState {
            until_unix_secs: past_unix,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 0,
            wait_seconds: 0,
        };
        let mut state = PrMonitorState::new(None, None, "t".into());
        let pr_info = crate::util::PrInfo {
            pr_number: None,
            repo: None,
            push_time: None,
        };

        let outcome = handle_rate_limit_retry(&rl, &mut state, &pr_info, 3);
        assert!(matches!(outcome, RateLimitOutcome::Failed(_)));
        assert_eq!(state.rate_limit_retries, 0);
        assert!(state.rate_limit_last_retriggered_at.is_none());
    }

    /// Bb-1: PARK signal は CronCreate 呼び出しに必要な構造化情報を含む。
    #[test]
    fn format_park_signal_includes_required_fields() {
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.rate_limit_retries = 0;
        let rl = RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "2026-05-01T00:00:00Z".into(),
            wait_minutes: 47,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: None,
        };

        let signal = format_park_signal(&state, &rl, &pr_info, 3);
        assert!(signal.starts_with("[PR_MONITOR_PARK]"));
        assert!(signal.contains("[/PR_MONITOR_PARK]"));
        assert!(signal.contains("pr: 42"));
        assert!(signal.contains("repo: o/r"));
        assert!(signal.contains("reset_at_unix: 1775088000"));
        assert!(signal.contains("wait_total_seconds: 2820"));
        assert!(signal.contains("retry_count: 1"));
        assert!(signal.contains("max_retries: 3"));
        assert!(signal.contains("CronCreate("));
        assert!(signal.contains("durable: true"));
        assert!(signal.contains("recurring: false"));
        assert!(signal.contains("--monitor-only"));
    }

    /// Bb-1: PR 番号 / repo が None でも format_park_signal は panic せず "?" を出す。
    #[test]
    fn format_park_signal_handles_missing_pr_info() {
        let state = PrMonitorState::new(None, None, "t".into());
        let rl = RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "2026-05-01T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 30,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: None,
            repo: None,
            push_time: None,
        };

        let signal = format_park_signal(&state, &rl, &pr_info, 3);
        assert!(signal.contains("pr: ?"));
        assert!(signal.contains("repo: ?"));
        assert!(signal.contains("wait_total_seconds: 330"));
    }
}
