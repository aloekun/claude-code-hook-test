mod iteration;
mod rate_limit;

use lib_report_formatter::Finding;

use crate::config::{Config, MonitorConfig, RateLimitConfig};
use crate::log::log_info;
use crate::runner::checker_exe_path;
use crate::state::{read_state_from, write_state_to, CiState, CodeRabbitState, RateLimitState};
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
    pub(super) skip_ci: bool,
    pub(super) skip_coderabbit: bool,
}

/// single-shot check モデル (WP-17 PR 3、ADR-018 amendment)。
///
/// checker を 1 回呼び、terminal action か「未確定 (pending)」の 2 択で必ず return する。
/// 旧 park モデル (Bb-1/Bb-2: 未確定なら state に wakeup 時刻を書き PARK signal を出して
/// CronCreate 再 invoke を待つ) は廃止した — PR イベントの後続処理は GitHub Actions 経路
/// (pr-monitor workflow の Phase A/B) が常時引き受け、ローカルの時限 wakeup は不要。
/// PR #237 で観測した「セッション終了による wakeup 失効 = 監視の取りこぼし」も機構ごと消える。
pub(crate) fn run_poll_loop(full_config: &Config, pr_info: &PrInfo) -> PollResult {
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
        skip_ci: !config.check_ci,
        skip_coderabbit: !config.check_coderabbit,
    };

    if let Some(terminal) = iteration::run_one_iteration(&ctx) {
        return terminal;
    }
    finalize_pending_review(&ctx)
}

/// checker が CodeRabbit のスキップ (`not_started`、順位 585) を返したか。
///
/// スキップは依頼しない限りレビューが来ないので、他の未確定と同じ「GitHub Actions 経路が
/// 処理」という案内にしない。summary ([`finalize_pending_review`]) と判定文 (monitor) が使う。
pub(crate) fn is_review_not_started(cr: Option<&CodeRabbitState>) -> bool {
    cr.is_some_and(|c| c.review_state == "not_started")
}

/// checker が未確定 (continue_monitoring 相当) を返した場合の terminal 化。
///
/// 旧 park モデルではここで wakeup を予約したが、single-shot モデルでは
/// 「未確定である」ことを loud に報告して終了する (ADR-064 の陽性証拠原則:
/// pending を silent success に見せない)。後続の PR イベント (レビュー到着 /
/// CodeRabbit コメント) は GitHub Actions 経路が処理する。再確認したい場合は
/// `cli-pr-monitor --monitor-only` を再実行すればよい (時刻窓は state.started_at
/// アンカーで継続する)。
fn finalize_pending_review(ctx: &PollContext<'_>) -> PollResult {
    let mut state = read_state_from(ctx.state_path).unwrap_or_else(|| {
        crate::state::PrMonitorState::new(
            ctx.pr_info.pr_number,
            ctx.pr_info.repo.clone(),
            ctx.push_time.to_string(),
        )
    });
    state.action = "pending_review".into();
    state.summary = if is_review_not_started(state.coderabbit.as_ref()) {
        "CodeRabbit がレビューをスキップした (未開始)。PR の Trigger review で依頼するまでレビューは来ない"
            .to_string()
    } else {
        "review 未確定。後続の PR イベントは GitHub Actions 経路 (pr-monitor workflow) が処理"
            .to_string()
    };
    state.record_head_commit(ctx.pr_info.head_commit.as_deref());
    state.fix_push_time = state
        .fix_push_time
        .or_else(|| ctx.fix_push_time.map(String::from));
    if let Err(e) = write_state_to(ctx.state_path, &state) {
        log_info(&format!(
            "state 書き込み失敗 (pending_review 確定後、続行): {}",
            e
        ));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClassifierConfig;
    use crate::state::PrMonitorState;

    fn make_ctx<'a>(
        checker: &'a std::path::Path,
        state_path: &'a std::path::Path,
        pr_info: &'a crate::util::PrInfo,
        rate_limit_config: &'a RateLimitConfig,
        classifier_config: &'a ClassifierConfig,
    ) -> PollContext<'a> {
        PollContext {
            checker,
            state_path,
            push_time: "2026-05-01T00:00:00Z",
            fix_push_time: None,
            pr_info,
            rate_limit_config,
            classifier_config,
            skip_ci: false,
            skip_coderabbit: false,
        }
    }

    /// 順位 585: checker がスキップ (`not_started`) を返していたら、summary は「GitHub Actions
    /// 経路が処理」ではなく依頼が要ることを伝える。依頼しない限りレビューは来ない。
    #[test]
    fn finalize_pending_review_summary_asks_for_trigger_when_not_started() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let mut seeded =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        seeded.coderabbit = Some(CodeRabbitState {
            review_state: "not_started".into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(0),
        });
        crate::state::write_state_to(&state_path, &seeded).unwrap();

        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = make_ctx(
            &checker,
            &state_path,
            &pr_info,
            &rate_limit_config,
            &classifier_config,
        );

        let outcome = finalize_pending_review(&ctx);

        assert_eq!(outcome.action, "pending_review");
        assert!(
            outcome.summary.contains("Trigger review") && !outcome.summary.contains("GitHub Actions"),
            "スキップは依頼が要ることを伝える: {}",
            outcome.summary
        );
    }

    /// PR 3 (wakeup 廃止): 未確定時は park ではなく terminal な `pending_review` を返し、
    /// state にも同 action が永続化されること。summary は GitHub Actions 経路への
    /// 引き継ぎを明示する (ADR-064: pending を silent success に見せない)。
    #[test]
    fn finalize_pending_review_returns_terminal_pending_action() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let mut seeded =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-01T00:00:00Z".into());
        seeded.fix_push_time = Some("2026-05-01T00:05:00Z".into());
        crate::state::write_state_to(&state_path, &seeded).unwrap();

        let pr_info = crate::util::PrInfo {
            pr_number: Some(42),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: Some("abc1234".into()),
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = make_ctx(
            &checker,
            &state_path,
            &pr_info,
            &rate_limit_config,
            &classifier_config,
        );

        let outcome = finalize_pending_review(&ctx);
        let persisted = crate::state::read_state_from(&state_path).unwrap();

        assert_eq!(outcome.action, "pending_review");
        assert!(
            outcome.summary.contains("GitHub Actions"),
            "summary は後続処理の引き継ぎ先を明示すること: {}",
            outcome.summary
        );
        assert_eq!(persisted.action, "pending_review");
        assert_eq!(
            persisted.head_commit.as_deref(),
            Some("abc1234"),
            "head_commit は state 継続判定 (should_continue_state) 用に保存されること"
        );
        assert_eq!(
            persisted.fix_push_time.as_deref(),
            Some("2026-05-01T00:05:00Z"),
            "write-once: 既存 fix_push_time を上書きしないこと (順位 141)"
        );
    }

    /// state 書き込み失敗でも panic せず pending_review を返すこと (fail-open:
    /// 監視は助言層であり、state 永続化失敗で監視結果自体を失わない)。
    #[test]
    fn finalize_pending_review_survives_write_failure() {
        let bad_path = std::env::temp_dir()
            .join(format!("pr-monitor-pr3-{}", std::process::id()))
            .join("nonexistent-dir")
            .join("state.json");

        let pr_info = crate::util::PrInfo {
            pr_number: Some(1),
            repo: Some("o/r".into()),
            push_time: Some("2026-05-01T00:00:00Z".into()),
            head_commit: None,
            fix_push_time: None,
        };
        let checker = std::path::PathBuf::from("dummy");
        let rate_limit_config = RateLimitConfig::default();
        let classifier_config = ClassifierConfig::default();
        let ctx = make_ctx(
            &checker,
            &bad_path,
            &pr_info,
            &rate_limit_config,
            &classifier_config,
        );

        let outcome = finalize_pending_review(&ctx);
        assert_eq!(outcome.action, "pending_review");
    }
}
