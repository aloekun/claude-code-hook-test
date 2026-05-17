use lib_report_formatter::Finding;
use std::time::Duration;

use crate::classifier_runner::classify_findings;
use crate::config::{
    ClassifierConfig, Config, MonitorConfig, RateLimitConfig, DEFAULT_CHECK_TIMEOUT_SECS,
};
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
    classifier_config: &'a ClassifierConfig,
    start: std::time::Instant,
    max_duration: u64,
    skip_ci: bool,
    skip_coderabbit: bool,
    /// fresh push 経路 (initial park) の wait 秒数 (Bb-3 順位 55: config 由来)
    initial_review_wait_secs: u64,
    /// wakeup 経路で次回 wakeup までの wait 秒数 (Bb-3 順位 55: config 由来)
    review_recheck_wait_secs: u64,
    /// recheck 上限 (Bb-3 順位 55: config 由来)
    max_review_rechecks: u32,
}

/// single-iteration check + park-or-terminate モデル (Bb-2)。
///
/// `is_wakeup=false` (fresh push): checker は呼ばず、即 `initial_review_wait_secs` 後の
/// wakeup を予約して exit する (CR review 開始前の wasteful API call を回避、todo5.md spec)。
///
/// `is_wakeup=true` (CronCreate からの再 invoke): 1 回 checker を呼び、結果に応じて
/// (a) terminal action / (b) rate-limit park (Bb-1) / (c) review_recheck park (Bb-2)
/// のいずれかで return する。
pub(crate) fn run_poll_loop(full_config: &Config, pr_info: &PrInfo, is_wakeup: bool) -> PollResult {
    let config: &MonitorConfig = &full_config.monitor;

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
        classifier_config: &full_config.classifier,
        start: std::time::Instant::now(),
        max_duration: config.max_duration_secs,
        skip_ci: !config.check_ci,
        skip_coderabbit: !config.check_coderabbit,
        initial_review_wait_secs: full_config.review_recheck.initial_review_wait_secs,
        review_recheck_wait_secs: full_config.review_recheck.review_recheck_wait_secs,
        max_review_rechecks: full_config.review_recheck.max_review_rechecks,
    };

    if !is_wakeup {
        return finalize_initial_review_park(&ctx);
    }

    if let Some(terminal) = run_one_iteration(&ctx) {
        return terminal;
    }
    finalize_review_recheck_park(&ctx)
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
    enrich_with_classifier(&mut state, ctx.classifier_config);
    log_info(&format!(
        "ポーリング: action={}, summary={}",
        state.action, state.summary
    ));

    if state.action != "continue_monitoring" {
        return Some(make_terminal_result(state, result));
    }

    if let Some(terminal) = handle_rate_limit_branch(
        &mut state,
        ctx.rate_limit_config,
        ctx.pr_info,
        ctx.review_recheck_wait_secs,
        &result,
    ) {
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
        state.review_recheck_count = existing.review_recheck_count;
        state.head_commit = existing.head_commit;
        state.classified_findings = existing.classified_findings;
    }

    apply_skip_handling(&mut state, skip_ci, skip_coderabbit);
    state.last_checked = Some(utc_now_iso8601());
    if let Err(e) = write_state(&state) {
        log_info(&format!("state 書き込み失敗 (skip 反映後、続行): {}", e));
    }
    state
}

/// classifier (ADR-038, Phase 5) で findings を enrich する。
///
/// `config.classifier.enabled = false` または findings が空のときは何もしない。
/// 実行成功時は state.classified_findings を populate して state file を再書き出す。
/// 失敗時は state.classified_findings は空のまま (caller は findings をそのまま使えばよい)。
fn enrich_with_classifier(state: &mut PrMonitorState, config: &ClassifierConfig) {
    if !config.enabled || state.findings.is_empty() {
        return;
    }
    let classified = classify_findings(config, &state.findings);
    if classified.is_empty() {
        return;
    }
    log_info(&format!(
        "classifier: {} findings を分類完了",
        classified.len()
    ));
    state.classified_findings = classified;
    if let Err(e) = write_state(state) {
        log_info(&format!(
            "state 書き込み失敗 (classifier enrich 後、続行): {}",
            e
        ));
    }
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
    review_recheck_wait_secs: u64,
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

    dispatch_rate_limit_outcome(
        state,
        &rl,
        pr_info,
        rate_limit_config.max_retries,
        review_recheck_wait_secs,
        result,
    )
}

fn dispatch_rate_limit_outcome(
    state: &mut PrMonitorState,
    rl: &crate::state::RateLimitState,
    pr_info: &PrInfo,
    max_retries: u32,
    review_recheck_wait_secs: u64,
    result: &serde_json::Value,
) -> Option<PollResult> {
    match handle_rate_limit_retry(rl, state, pr_info, max_retries) {
        RateLimitOutcome::Posted => finalize_posted_retrigger(
            state,
            rl,
            pr_info,
            review_recheck_wait_secs,
            max_retries,
            result,
        ),
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
    pr_info: &PrInfo,
    review_recheck_wait_secs: u64,
    max_retries: u32,
    result: &serde_json::Value,
) -> Option<PollResult> {
    state.rate_limit_last_retriggered_at = Some(rl.comment_event_time.clone());

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let park_at_unix = now_unix + review_recheck_wait_secs as i64;

    state.action = "parked_review_recheck".into();
    state.next_wakeup_at_unix = Some(park_at_unix);
    state.wakeup_reason = Some("rate_limit_post_retrigger".into());
    state.head_commit = pr_info.head_commit.clone();
    state.summary = format!(
        "rate-limit retrigger 後の review 完了待ちを {}s 後に予約 (順位 80 fix: silent exit 防止)",
        review_recheck_wait_secs
    );

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

    let signal = format_park_signal(state, rl, pr_info, max_retries);
    println!("{}", signal);

    Some(make_park_poll_result(state.clone()))
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
    state.head_commit = pr_info.head_commit.clone();
    state.summary = format!(
        "CodeRabbit rate-limit: wakeup を {}m{}s 後に予約 (PARK signal 参照)",
        rl.wait_minutes, rl.wait_seconds
    );
    if let Err(e) = write_state(state) {
        let msg = format!("park state 永続化失敗のため PARK signal を中止 ({})。手動で `@coderabbitai review` を投稿してください", e);
        return make_action_required_result(state, result, &msg);
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
/// ため、CronCreate の cron 値は Claude が local timezone に変換した ISO 8601 形式で
/// 設定する (chrono 依存を避けるため Rust 側では変換しない)。
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
reason: rate_limit_retry
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
  cron: \"<reset_at_iso_utc を local timezone の ISO 8601 形式に変換, e.g. 2024-01-15T09:30:00>\",
  recurring: false,
  durable: true,
  prompt: \"Wakeup: rate-limit retry for PR #{pr} ({repo}). cd \\\"{cwd}\\\" && \\\"{exe}\\\" --monitor-only\"
}})
[/PR_MONITOR_PARK]",
        until = rl.until_unix_secs,
    )
}

struct ReviewParkSignalFields {
    pr: String,
    repo: String,
    wakeup_unix: i64,
    wakeup_iso: String,
    wait_secs: i64,
    exe: String,
    cwd: String,
    recheck: u32,
    max_rechecks: u32,
}

fn collect_review_park_fields(
    state: &PrMonitorState,
    ctx: &PollContext<'_>,
) -> ReviewParkSignalFields {
    let pr = ctx
        .pr_info
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let repo = ctx.pr_info.repo.clone().unwrap_or_else(|| "?".into());
    let wakeup_unix = state.next_wakeup_at_unix.unwrap_or(0);
    let wakeup_iso = if wakeup_unix > 0 {
        lib_pending_file::epoch_secs_to_iso8601(wakeup_unix as u64)
    } else {
        "?".into()
    };
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let wait_secs = (wakeup_unix - now_unix).max(0);
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "cli-pr-monitor.exe".into());
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into());

    ReviewParkSignalFields {
        pr,
        repo,
        wakeup_unix,
        wakeup_iso,
        wait_secs,
        exe,
        cwd,
        recheck: state.review_recheck_count,
        max_rechecks: ctx.max_review_rechecks,
    }
}

/// Bb-2: 初回 push 後の review_recheck park signal を生成する。
///
/// `format_park_signal` (rate_limit_retry) と同じ envelope `[PR_MONITOR_PARK]` を使い、
/// `reason: review_recheck` で discriminate する。Claude Code 側のパーサは両 signal を
/// 同じ format で読める。
fn format_review_park_signal(state: &PrMonitorState, ctx: &PollContext<'_>) -> String {
    let f = collect_review_park_fields(state, ctx);
    format!(
        "[PR_MONITOR_PARK]
reason: review_recheck
pr: {pr}
repo: {repo}
next_wakeup_at_unix: {wakeup_unix}
next_wakeup_at_iso_utc: {wakeup_iso}
wait_total_seconds: {wait_secs}
recheck_count: {recheck}
max_rechecks: {max}
exe: {exe}
cwd: {cwd}

ACTION REQUIRED: please schedule one-shot wakeup using CronCreate.

CronCreate({{
  cron: \"<next_wakeup_at_iso_utc を local timezone の ISO 8601 形式に変換, e.g. 2024-01-15T09:30:00>\",
  recurring: false,
  durable: true,
  prompt: \"Wakeup: review recheck for PR #{pr} ({repo}). cd \\\"{cwd}\\\" && \\\"{exe}\\\" --monitor-only\"
}})
[/PR_MONITOR_PARK]",
        pr = f.pr,
        repo = f.repo,
        wakeup_unix = f.wakeup_unix,
        wakeup_iso = f.wakeup_iso,
        wait_secs = f.wait_secs,
        recheck = f.recheck,
        max = f.max_rechecks,
        exe = f.exe,
        cwd = f.cwd,
    )
}

/// Bb-2: fresh push 経路で review_recheck park を行う (checker 呼び出しなし)。
///
/// 動機: push 直後は CR がまだ review を開始していない可能性が高く、即 check は wasteful。
/// `initial_review_wait_secs` 後に wakeup を予約 → 1 回 check という 2-step フローに分離する。
///
/// CR Major #2 fix (Bb-2 PR #114 review): 既存 state に残った `review_recheck_count` を
/// fresh push では 0 に明示リセット (前サイクルが MAX 到達等で残った count が新 push に
/// 持ち越されると summary "(initial wait, recheck=0/N)" と PARK signal の recheck_count が
/// 食い違い、最悪 max 到達状態で park される)。
/// CR Major #1 fix: head_commit を state に保存し detect_wakeup_resume の比較対象とする。
fn finalize_initial_review_park(ctx: &PollContext<'_>) -> PollResult {
    let mut state = read_state().unwrap_or_else(|| {
        PrMonitorState::new(
            ctx.pr_info.pr_number,
            ctx.pr_info.repo.clone(),
            ctx.push_time.to_string(),
        )
    });
    state.review_recheck_count = 0;
    state.head_commit = ctx.pr_info.head_commit.clone();
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    state.next_wakeup_at_unix = Some(now_unix + ctx.initial_review_wait_secs as i64);
    state.wakeup_reason = Some("review_recheck".into());
    state.action = "parked_review_recheck".into();
    state.summary = format!(
        "review check を {}s 後に予約 (initial wait, recheck=0/{})",
        ctx.initial_review_wait_secs, ctx.max_review_rechecks
    );

    if let Err(e) = write_state(&state) {
        log_info(&format!(
            "[review_recheck] initial park state 永続化失敗、action_required で抜ける: {}",
            e
        ));
        return make_action_required_result(
            &state,
            &serde_json::Value::Null,
            &format!("review park の state 永続化失敗 ({})。手動確認が必要", e),
        );
    }

    println!("{}", format_review_park_signal(&state, ctx));
    make_park_poll_result(state)
}

/// Bb-2: wakeup 経路の review_recheck park (checker check 後に continue_monitoring の場合)。
///
/// `review_recheck_count` をインクリメントし、`max_review_rechecks` 到達なら
/// `action_required` で抜ける (review が想定時間内に未完了を通知)。
/// 未到達なら `review_recheck_wait_secs` 後の wakeup を予約して return。
fn finalize_review_recheck_park(ctx: &PollContext<'_>) -> PollResult {
    let mut state = read_state().unwrap_or_else(|| {
        PrMonitorState::new(
            ctx.pr_info.pr_number,
            ctx.pr_info.repo.clone(),
            ctx.push_time.to_string(),
        )
    });
    state.review_recheck_count += 1;

    if state.review_recheck_count >= ctx.max_review_rechecks {
        return finalize_review_recheck_max_reached(&mut state, ctx.max_review_rechecks);
    }

    schedule_next_review_recheck_park(&mut state, ctx)
}

fn finalize_review_recheck_max_reached(
    state: &mut PrMonitorState,
    max_review_rechecks: u32,
) -> PollResult {
    log_info(&format!(
        "[review_recheck] max {} 回到達、action_required で抜ける",
        max_review_rechecks
    ));
    let summary = format!(
        "review が想定時間内に完了せず ({} recheck 後)。手動で PR を確認してください",
        state.review_recheck_count
    );
    state.action = "action_required".into();
    state.summary = summary.clone();
    if let Err(e) = write_state(state) {
        log_info(&format!(
            "state 書き込み失敗 (action_required 確定後、続行): {}",
            e
        ));
    }
    make_action_required_result(state, &serde_json::Value::Null, &summary)
}

fn schedule_next_review_recheck_park(
    state: &mut PrMonitorState,
    ctx: &PollContext<'_>,
) -> PollResult {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    state.next_wakeup_at_unix = Some(now_unix + ctx.review_recheck_wait_secs as i64);
    state.wakeup_reason = Some("review_recheck".into());
    state.action = "parked_review_recheck".into();
    state.head_commit = ctx.pr_info.head_commit.clone();
    state.summary = format!(
        "review check を {}s 後に予約 (recheck={}/{})",
        ctx.review_recheck_wait_secs, state.review_recheck_count, ctx.max_review_rechecks
    );

    if let Err(e) = write_state(state) {
        log_info(&format!(
            "[review_recheck] park state 永続化失敗、action_required で抜ける: {}",
            e
        ));
        return make_action_required_result(
            state,
            &serde_json::Value::Null,
            &format!("review park の state 永続化失敗 ({})。手動確認が必要", e),
        );
    }

    println!("{}", format_review_park_signal(state, ctx));
    make_park_poll_result(state.clone())
}

/// review_recheck park / initial park の戻り値生成 helper (check_output=None)。
fn make_park_poll_result(state: PrMonitorState) -> PollResult {
    PollResult {
        action: state.action,
        summary: state.summary,
        ci: state.ci,
        coderabbit: state.coderabbit,
        findings: state.findings,
        check_output: None,
        rate_limit: state.rate_limit,
    }
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
            head_commit: None,
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
            head_commit: None,
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
            head_commit: None,
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
            head_commit: None,
        };

        let signal = format_park_signal(&state, &rl, &pr_info, 3);
        assert!(signal.contains("pr: ?"));
        assert!(signal.contains("repo: ?"));
        assert!(signal.contains("wait_total_seconds: 330"));
    }

    /// PR_MONITOR_STATE_FILE_OVERRIDE は process-global env var のため、
    /// override 設定 / 解除を test 並行実行で race させない serial guard。
    fn env_override_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    /// 書き込み先がディレクトリ不在のため write が必ず失敗する override path を返す。
    fn unwritable_state_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pr-monitor-T2-2-{}", std::process::id()))
            .join("nonexistent-dir")
            .join("state.json")
    }

    /// Bb-1 (T2-2): `finalize_parked` は write_state 失敗時に PARK signal emit を中止し
    /// `action_required` を返却する fail-safe 経路を持つ (CodeRabbit Major #1 fix の固定化)。
    #[test]
    fn finalize_parked_returns_action_required_when_write_state_fails() {
        let _guard = env_override_lock();
        let bad_path = unwritable_state_path();
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &bad_path);

        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
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
            head_commit: None,
        };
        let result = serde_json::json!({});

        let outcome = finalize_parked(&mut state, &rl, &pr_info, 1_775_088_000, 3, &result);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert_eq!(
            outcome.action, "action_required",
            "T2-2: write_state 失敗 → action_required で抜ける fail-safe が必要"
        );
        assert!(
            outcome.summary.contains("PARK signal を中止")
                || outcome.summary.contains("永続化失敗"),
            "summary に永続化失敗の説明が含まれること: {}",
            outcome.summary
        );
    }

    /// Bb-2 (T2-2): `schedule_next_review_recheck_park` は write_state 失敗時に
    /// PARK signal emit を中止し `action_required` を返却する (sibling parity)。
    #[test]
    fn schedule_next_review_recheck_park_returns_action_required_when_write_state_fails() {
        let _guard = env_override_lock();
        let bad_path = unwritable_state_path();
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &bad_path);

        let mut state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        state.review_recheck_count = 1;
        let checker_path = std::path::PathBuf::from("dummy-checker");
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
        };
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker_path,
            push_time: "2026-05-01T00:00:00Z",
            pr_info: &pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 300,
            review_recheck_wait_secs: 300,
            max_review_rechecks: 3,
        };

        let outcome = schedule_next_review_recheck_park(&mut state, &ctx);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert_eq!(
            outcome.action, "action_required",
            "T2-2 sibling parity: review park も write_state 失敗 → action_required で抜けること"
        );
    }

    fn invoke_finalize_parked_with_bad_path(pr_info: &crate::util::PrInfo) -> PollResult {
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        let rl = RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "x".into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let result = serde_json::json!({});
        finalize_parked(&mut state, &rl, pr_info, 1_775_088_000, 3, &result)
    }

    fn invoke_review_park_with_bad_path(pr_info: &crate::util::PrInfo) -> PollResult {
        let mut state =
            PrMonitorState::new(Some(1), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        state.review_recheck_count = 1;
        let checker_path = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker_path,
            push_time: "2026-05-01T00:00:00Z",
            pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 300,
            review_recheck_wait_secs: 300,
            max_review_rechecks: 3,
        };
        schedule_next_review_recheck_park(&mut state, &ctx)
    }

    fn invoke_finalize_initial_review_park_with_bad_path(
        pr_info: &crate::util::PrInfo,
    ) -> PollResult {
        let checker_path = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker_path,
            push_time: "2026-05-01T00:00:00Z",
            pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 300,
            review_recheck_wait_secs: 300,
            max_review_rechecks: 3,
        };
        finalize_initial_review_park(&ctx)
    }

    fn seed_stale_recheck_state(tmp_path: &std::path::Path) {
        let mut stale_state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        stale_state.review_recheck_count = 3;
        stale_state.action = "action_required".into();
        crate::state::write_state_to(tmp_path, &stale_state).unwrap();
    }

    /// Bb-3 (順位 55): `max_review_rechecks` の config 化が実際に PARK signal に
    /// 反映されることを machine-enforce する (default 3 ではなく custom 値が出力されること)。
    #[test]
    fn format_review_park_signal_uses_configured_max_rechecks() {
        let state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker,
            push_time: "2026-05-01T00:00:00Z",
            pr_info: &pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 120,
            review_recheck_wait_secs: 240,
            max_review_rechecks: 7,
        };

        let signal = format_review_park_signal(&state, &ctx);

        assert!(
            signal.contains("max_rechecks: 7"),
            "PARK signal に config 値 (max_rechecks: 7) が反映されること: {}",
            signal
        );
        assert!(
            !signal.contains("max_rechecks: 3"),
            "default 値 3 が hard-coded で残っていないこと: {}",
            signal
        );
    }

    /// CR Major #2 fix (Bb-2 PR #114 review): fresh push 経路では `finalize_initial_review_park`
    /// が `review_recheck_count` を 0 に明示リセットすること。前サイクルが MAX 到達 (count=3)
    /// で残った state を持ち越さないことを machine-enforce する。
    #[test]
    fn finalize_initial_review_park_resets_recheck_count() {
        let _guard = env_override_lock();
        let tmp_path = std::env::temp_dir().join(format!(
            "pr-monitor-CR-M2-{}-state.json",
            std::process::id()
        ));
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &tmp_path);
        seed_stale_recheck_state(&tmp_path);

        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker,
            push_time: "2026-05-01T00:00:00Z",
            pr_info: &pr_info,
            rate_limit_config: &rate_limit_config,
            classifier_config: &classifier_config,
            start: std::time::Instant::now(),
            max_duration: 600,
            skip_ci: false,
            skip_coderabbit: false,
            initial_review_wait_secs: 300,
            review_recheck_wait_secs: 300,
            max_review_rechecks: 3,
        };

        let outcome = finalize_initial_review_park(&ctx);
        let persisted = crate::state::read_state_from(&tmp_path).unwrap();

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");
        let _ = std::fs::remove_file(&tmp_path);

        assert_eq!(outcome.action, "parked_review_recheck");
        assert_eq!(
            persisted.review_recheck_count, 0,
            "CR Major #2: fresh push 経路で count=3 が残らず 0 にリセットされること"
        );
        assert_eq!(
            persisted.head_commit.as_deref(),
            Some("abc1234"),
            "CR Major #1: fresh push 経路で head_commit が pr_info から保存されること"
        );
    }

    /// `enabled = false` のとき `enrich_with_classifier` は early return し
    /// `classified_findings` を変更しない。
    ///
    /// `classified_findings` を空のまま渡すことで `!config.enabled` ガードのみを純粋に分離する。
    #[test]
    fn enrich_with_classifier_skips_when_disabled() {
        use lib_report_formatter::Finding;

        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.findings = vec![Finding {
            severity: "Major".into(),
            file: "f.rs".into(),
            line: "1".into(),
            issue: "issue".into(),
            suggestion: "fix".into(),
            source: "coderabbit".into(),
        }];
        let disabled = ClassifierConfig { enabled: false, ..ClassifierConfig::default() };
        enrich_with_classifier(&mut state, &disabled);
        assert!(
            state.classified_findings.is_empty(),
            "disabled guard should prevent classification from running"
        );
    }

    /// Bb-2 (T2-2) + Bb-3 follow-up: 3 つの finalize_* park sibling
    /// (`finalize_parked` / `schedule_next_review_recheck_park` / `finalize_initial_review_park`)
    /// は全て write_state 失敗で `action_required` を返す invariant を 1 テストで
    /// machine-enforce する。新 finalize_* 関数を追加する際、本テストが落ちて
    /// invariant 維持を強制する。
    #[test]
    fn finalize_park_siblings_have_symmetric_write_state_handling() {
        let _guard = env_override_lock();
        let bad_path = unwritable_state_path();
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &bad_path);

        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
        };

        let outcome_rate_limit = invoke_finalize_parked_with_bad_path(&pr_info);
        let outcome_review = invoke_review_park_with_bad_path(&pr_info);
        let outcome_initial = invoke_finalize_initial_review_park_with_bad_path(&pr_info);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert_eq!(
            outcome_rate_limit.action, "action_required",
            "finalize_parked: write_state 失敗 → action_required"
        );
        assert_eq!(
            outcome_review.action, "action_required",
            "schedule_next_review_recheck_park: write_state 失敗 → action_required"
        );
        assert_eq!(
            outcome_initial.action, "action_required",
            "finalize_initial_review_park: write_state 失敗 → action_required"
        );
        assert_eq!(
            outcome_rate_limit.action, outcome_review.action,
            "sibling parity (rate_limit ↔ review_recheck)"
        );
        assert_eq!(
            outcome_review.action, outcome_initial.action,
            "sibling parity (review_recheck ↔ initial_review)"
        );
    }

    fn setup_posted_retrigger_fixture() -> (PrMonitorState, RateLimitState, crate::util::PrInfo) {
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.action = "continue_monitoring".into();
        state.rate_limit_retries = 1;
        let rl = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: "2026-05-08T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
        };
        (state, rl, pr_info)
    }

    #[test]
    fn finalize_posted_retrigger_schedules_park_after_post() {
        let _guard = env_override_lock();
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &state_path);

        let (mut state, rl, pr_info) = setup_posted_retrigger_fixture();
        let result = finalize_posted_retrigger(&mut state, &rl, &pr_info, 300, 3, &serde_json::Value::Null);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        let park_result = result.expect("順位 80 fix: Posted 後は必ず park を返し silent exit を防ぐ");
        assert_eq!(park_result.action, "parked_review_recheck");
        assert_eq!(state.wakeup_reason.as_deref(), Some("rate_limit_post_retrigger"));
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let wakeup = state.next_wakeup_at_unix.expect("next_wakeup_at_unix が設定される");
        assert!(wakeup > now_unix && wakeup <= now_unix + 301);
        assert_eq!(state.rate_limit_last_retriggered_at.as_deref(), Some("2026-05-08T00:00:00Z"));
    }

    #[test]
    fn finalize_posted_retrigger_action_required_when_write_state_fails() {
        let _guard = env_override_lock();
        let bad_path = unwritable_state_path();
        std::env::set_var("PR_MONITOR_STATE_FILE_OVERRIDE", &bad_path);

        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        state.action = "continue_monitoring".into();
        let rl = RateLimitState {
            until_unix_secs: 0,
            comment_event_time: "2026-05-08T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 0,
        };
        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
        };

        let result = finalize_posted_retrigger(&mut state, &rl, &pr_info, 300, 3, &serde_json::Value::Null);

        std::env::remove_var("PR_MONITOR_STATE_FILE_OVERRIDE");

        assert!(result.is_some());
        assert_eq!(
            result.unwrap().action,
            "action_required",
            "write_state 失敗時は action_required で抜ける (sibling parity with finalize_parked)"
        );
    }
}
