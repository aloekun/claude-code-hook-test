mod iteration;
mod rate_limit;
mod rate_limit_signal;
mod review_recheck;
mod review_recheck_signal;

use review_recheck::{finalize_initial_review_park, finalize_review_recheck_park};

use lib_report_formatter::Finding;

use crate::config::{Config, MonitorConfig, RateLimitConfig};
use crate::log::log_info;
use crate::runner::checker_exe_path;
use crate::state::{CiState, CodeRabbitState, PrMonitorState, RateLimitState};
use crate::util::PrInfo;

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

pub(super) struct PollContext<'a> {
    pub(super) checker: &'a std::path::Path,
    /// state file の保存先 (順位 229: テストは自前 path を注入し env var 競合を排除)。
    pub(super) state_path: &'a std::path::Path,
    pub(super) push_time: &'a str,
    /// 順位 141: fresh push 時刻の固定値 (CR rate-limit detection bug 修正)。
    /// 設定されていれば `build_checker_args` で `--push-time` に優先採用される。
    /// None なら `push_time` (= state.started_at fallback) を使う legacy 互換。
    pub(super) fix_push_time: Option<&'a str>,
    pub(super) pr_info: &'a PrInfo,
    pub(super) rate_limit_config: &'a RateLimitConfig,
    pub(super) classifier_config: &'a crate::config::ClassifierConfig,
    pub(super) start: std::time::Instant,
    pub(super) max_duration: u64,
    pub(super) skip_ci: bool,
    pub(super) skip_coderabbit: bool,
    /// fresh push 経路 (initial park) の wait 秒数 (Bb-3 順位 55: config 由来)
    pub(super) initial_review_wait_secs: u64,
    /// wakeup 経路で次回 wakeup までの wait 秒数 (Bb-3 順位 55: config 由来)
    pub(super) review_recheck_wait_secs: u64,
    /// recheck 上限 (Bb-3 順位 55: config 由来)
    pub(super) max_review_rechecks: u32,
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

    let state_path = crate::state::state_file_path();
    let ctx = PollContext {
        checker: &checker,
        state_path: &state_path,
        push_time: pr_info
            .push_time
            .as_deref()
            .unwrap_or("1970-01-01T00:00:00Z"),
        fix_push_time: pr_info.fix_push_time.as_deref(),
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

    if let Some(terminal) = iteration::run_one_iteration(&ctx) {
        return terminal;
    }
    finalize_review_recheck_park(&ctx)
}

pub(super) fn error_poll_result(summary: &str) -> PollResult {
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

/// review_recheck park / initial park の戻り値生成 helper (check_output=None)。
pub(super) fn make_park_poll_result(state: PrMonitorState) -> PollResult {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClassifierConfig;
    use rate_limit::finalize_parked;
    use review_recheck::{finalize_initial_review_park, schedule_next_review_recheck_park};

    /// 書き込み先がディレクトリ不在のため write が必ず失敗する path を返す。
    fn unwritable_state_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pr-monitor-T2-2-{}", std::process::id()))
            .join("nonexistent-dir")
            .join("state.json")
    }

    fn invoke_finalize_parked_with_bad_path(
        pr_info: &crate::util::PrInfo,
        state_path: &std::path::Path,
    ) -> PollResult {
        let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
        let rl = RateLimitState {
            until_unix_secs: 1_775_088_000,
            comment_event_time: "x".into(),
            wait_minutes: 5,
            wait_seconds: 0,
            wait_time_parsed: true,
        };
        let result = serde_json::json!({});
        finalize_parked(
            &mut state,
            &rl,
            pr_info,
            1_775_088_000,
            3,
            &result,
            state_path,
        )
    }

    fn invoke_review_park_with_bad_path(
        pr_info: &crate::util::PrInfo,
        state_path: &std::path::Path,
    ) -> PollResult {
        let mut state =
            PrMonitorState::new(Some(1), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        state.review_recheck_count = 1;
        let checker_path = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker_path,
            state_path,
            push_time: "2026-05-01T00:00:00Z",
            fix_push_time: None,
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
        state_path: &std::path::Path,
    ) -> PollResult {
        let checker_path = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = PollContext {
            checker: &checker_path,
            state_path,
            push_time: "2026-05-01T00:00:00Z",
            fix_push_time: None,
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

    /// Bb-2 (T2-2) + Bb-3 follow-up: 3 つの finalize_* park sibling
    /// (`finalize_parked` / `schedule_next_review_recheck_park` / `finalize_initial_review_park`)
    /// は全て write_state 失敗で `action_required` を返す invariant を 1 テストで
    /// machine-enforce する。新 finalize_* 関数を追加する際、本テストが落ちて
    /// invariant 維持を強制する。
    #[test]
    fn finalize_park_siblings_have_symmetric_write_state_handling() {
        let bad_path = unwritable_state_path();

        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
            fix_push_time: None,
        };

        let outcome_rate_limit = invoke_finalize_parked_with_bad_path(&pr_info, &bad_path);
        let outcome_review = invoke_review_park_with_bad_path(&pr_info, &bad_path);
        let outcome_initial =
            invoke_finalize_initial_review_park_with_bad_path(&pr_info, &bad_path);

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
}
