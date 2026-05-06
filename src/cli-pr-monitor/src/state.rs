use crate::classifier_runner::ClassifiedFinding;
use lib_report_formatter::Finding;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct PrMonitorState {
    pub(crate) pr: Option<u64>,
    pub(crate) repo: Option<String>,
    pub(crate) started_at: String,
    pub(crate) last_checked: Option<String>,
    pub(crate) ci: Option<CiState>,
    pub(crate) coderabbit: Option<CodeRabbitState>,
    pub(crate) action: String,
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) findings: Vec<Finding>,
    /// classifier (ADR-038, Phase 5) で enrich した findings。
    ///
    /// `config.classifier.enabled = false` または classifier 失敗時は空 Vec で残る。
    /// 既存 consumers (takt facets / Claude) が `findings` のみ参照する経路を破壊しない
    /// よう、独立 field として保持する。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) classified_findings: Vec<ClassifiedFinding>,
    pub(crate) notified: bool,
    pub(crate) daemon_pid: Option<u32>,
    pub(crate) daemon_status: String,
    /// CodeRabbit rate-limit 検出時の制御情報 (PR #89 T2-1)
    /// 検出されない監視ターンでは None。再 trigger 後の poll で消える。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rate_limit: Option<RateLimitState>,
    /// rate-limit 自動再 trigger の累積回数 (PR #89 T2-1)
    /// 上限 (config.rate_limit.max_retries) 超過で自動 retry を停止し action_required で抜ける。
    #[serde(default)]
    pub(crate) rate_limit_retries: u32,
    /// 直近で retrigger した rate-limit comment の event_time (dedup 用)
    ///
    /// 同一 comment が iteration 跨ぎで polling 結果に残るため、これを check しないと
    /// 同じ rate-limit comment に対して max_retries 回まで秒単位で retrigger を走らせてしまう。
    /// CR が新たな rate-limit comment を投稿したら event_time が変わり再度 retrigger 対象になる。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rate_limit_last_retriggered_at: Option<String>,
    /// 次回 wakeup の予定時刻 (unix epoch 秒)。Bb-1 (Bundle b PR-1) で導入。
    ///
    /// rate-limit 等で長時間待機が必要な場合、cli-pr-monitor は同プロセス内で sleep せず
    /// この field に reset 時刻を保存して exit する。Claude Code 側が stdout の
    /// PARK signal を読み、CronCreate (`durable: true`) で wakeup を予約する。
    /// wakeup 発火時に `cli-pr-monitor.exe --monitor-only` が再 invoke される。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) next_wakeup_at_unix: Option<i64>,
    /// wakeup の理由ラベル (e.g. "rate_limit_retry" / "review_recheck"). Bb-1 で導入、Bb-2 で値追加。
    ///
    /// Bb-2 (review 完了待ち) で `"review_recheck"` 経路を追加。Bb-3 (SessionStart catch-up) で
    /// 複数の wakeup 経路を識別するための discriminator。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) wakeup_reason: Option<String>,
    /// review 完了待ちの recheck 回数 (Bb-2 で導入)。
    ///
    /// `parked_review_recheck` 経路で wakeup が発火するたびにインクリメントされる。
    /// `max_review_rechecks` (config) 到達で `action_required` 経路に抜ける (review が想定時間内に
    /// 完了していない通知)。新規 push で `PrMonitorState::new` により 0 にリセット、wakeup 経路で
    /// は build_state_for_iteration が既存値を保持する。
    #[serde(default)]
    pub(crate) review_recheck_count: u32,
    /// park 時点の PR head commit OID (CR Major #1 fix, Bb-2 PR #114 review)。
    ///
    /// `detect_wakeup_resume` が wakeup 判定時に「同 PR への新 push で head が変わって
    /// いないか」を検証するために使う。state の pr / repo / next_wakeup_at_unix が
    /// 一致しても head が変われば fresh push として扱い、stale state (started_at /
    /// review_recheck_count) を新 commit に持ち込まない。値は `gh pr view --json
    /// headRefOid` で取得した SHA。legacy state (本フィールド未設定) は wakeup 不一致
    /// 扱い (= fresh push 経路) で安全側に倒す。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) head_commit: Option<String>,
}

/// check-ci-coderabbit から伝播する rate-limit 制御情報
///
/// `until_unix_secs` は「rate limit reset 予測 + 60s buffer」の unix epoch 秒。
/// poll loop はこれと現在時刻を比較し sleep 量を決定する。
///
/// `comment_event_time` は計算に使った event_time (CR comment の updated_at が
/// 存在すればそれ、なければ created_at)。dedup key として使用される。CR が
/// rate-limit comment を編集すると updated_at が変わるため、編集後の新 wait
/// 時間で正しく retrigger できる (PR #97 round 3 Finding 1)。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct RateLimitState {
    pub(crate) until_unix_secs: i64,
    /// JSON キーは "comment_created_at" (check-ci-coderabbit wire format / 既存 state ファイルとの互換維持)
    #[serde(rename = "comment_created_at")]
    pub(crate) comment_event_time: String,
    pub(crate) wait_minutes: u64,
    pub(crate) wait_seconds: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct CiState {
    pub(crate) overall: String,
    pub(crate) runs: Vec<CiRunState>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct CiRunState {
    pub(crate) name: String,
    pub(crate) conclusion: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct CodeRabbitState {
    pub(crate) review_state: String,
    pub(crate) new_comments: usize,
    pub(crate) actionable_comments: Option<usize>,
    pub(crate) unresolved_threads: Option<usize>,
}

impl PrMonitorState {
    pub(crate) fn new(pr: Option<u64>, repo: Option<String>, started_at: String) -> Self {
        Self {
            pr,
            repo,
            started_at,
            last_checked: None,
            ci: None,
            coderabbit: None,
            action: "continue_monitoring".to_string(),
            summary: "監視開始...".to_string(),
            findings: Vec::new(),
            classified_findings: Vec::new(),
            notified: false,
            daemon_pid: None,
            daemon_status: "running".to_string(),
            rate_limit: None,
            rate_limit_retries: 0,
            rate_limit_last_retriggered_at: None,
            next_wakeup_at_unix: None,
            wakeup_reason: None,
            review_recheck_count: 0,
            head_commit: None,
        }
    }
}

/// state file の保存パスを返す。
///
/// 通常は `<exe>/pr-monitor-state.json`。
/// 環境変数 `PR_MONITOR_STATE_FILE_OVERRIDE` がセットされていればそのパスを優先する
/// (T2-2 / fault injection test 用)。本番コードは env を設定しないため挙動変化なし。
pub(crate) fn state_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("PR_MONITOR_STATE_FILE_OVERRIDE") {
        return PathBuf::from(path);
    }
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("pr-monitor-state.json")
}

pub(crate) fn write_state_to(path: &Path, state: &PrMonitorState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("state シリアライズ失敗: {}", e))?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("state 一時ファイル書き込み失敗: {}", e))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("state ファイル rename 失敗: {}", e))?;
    Ok(())
}

pub(crate) fn read_state_from(path: &Path) -> Option<PrMonitorState> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub(crate) fn write_state(state: &PrMonitorState) -> Result<(), String> {
    write_state_to(&state_file_path(), state)
}

pub(crate) fn read_state() -> Option<PrMonitorState> {
    read_state_from(&state_file_path())
}

/// check-ci-coderabbit の JSON 出力から state を更新する
pub(crate) fn update_state_from_check_result(
    state: &mut PrMonitorState,
    result: &serde_json::Value,
) {
    if let Some(action) = result.get("action").and_then(|v| v.as_str()) {
        state.action = action.to_string();
    }
    if let Some(summary) = result.get("summary").and_then(|v| v.as_str()) {
        state.summary = summary.to_string();
    }
    if let Some(ci_val) = result.get("ci") {
        if let Ok(ci) = serde_json::from_value(ci_val.clone()) {
            state.ci = Some(ci);
        }
    }
    if let Some(cr_val) = result.get("coderabbit") {
        if let Ok(cr) = serde_json::from_value(cr_val.clone()) {
            state.coderabbit = Some(cr);
        }
    }
    if let Some(findings_val) = result.get("findings") {
        if let Ok(findings) = serde_json::from_value::<Vec<Finding>>(findings_val.clone()) {
            state.findings = findings;
        }
    }
    state.rate_limit = result
        .get("rate_limit")
        .and_then(|v| serde_json::from_value::<RateLimitState>(v.clone()).ok());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_new_defaults() {
        let state = PrMonitorState::new(
            Some(42),
            Some("owner/repo".into()),
            "2026-04-04T12:00:00Z".into(),
        );
        assert_eq!(state.pr, Some(42));
        assert_eq!(state.repo.as_deref(), Some("owner/repo"));
        assert_eq!(state.action, "continue_monitoring");
        assert_eq!(state.daemon_status, "running");
        assert!(!state.notified);
        assert!(state.ci.is_none());
        assert!(state.coderabbit.is_none());
        assert!(state.last_checked.is_none());
    }

    #[test]
    fn state_serialize_roundtrip() {
        let state = PrMonitorState {
            pr: Some(123),
            repo: Some("owner/repo".into()),
            started_at: "2026-04-04T12:00:00Z".into(),
            last_checked: Some("2026-04-04T12:02:00Z".into()),
            ci: Some(CiState {
                overall: "success".into(),
                runs: vec![CiRunState {
                    name: "test".into(),
                    conclusion: "success".into(),
                }],
            }),
            coderabbit: Some(CodeRabbitState {
                review_state: "success".into(),
                new_comments: 2,
                actionable_comments: Some(1),
                unresolved_threads: Some(0),
            }),
            action: "action_required".into(),
            summary: "CI成功。CodeRabbit: 指摘2件".into(),
            findings: vec![Finding {
                severity: "Critical".into(),
                file: "main.rs".into(),
                line: "641".into(),
                issue: "race condition".into(),
                suggestion: "write first".into(),
                source: "CodeRabbit".into(),
            }],
            classified_findings: Vec::new(),
            notified: false,
            daemon_pid: Some(12345),
            daemon_status: "running".into(),
            rate_limit: None,
            rate_limit_retries: 0,
            rate_limit_last_retriggered_at: None,
            next_wakeup_at_unix: None,
            wakeup_reason: None,
            review_recheck_count: 0,
            head_commit: None,
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: PrMonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }

    #[test]
    fn state_serialize_roundtrip_with_head_commit() {
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.head_commit = Some("abc1234deadbeef".into());

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("head_commit"));

        let deserialized: PrMonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.head_commit.as_deref(), Some("abc1234deadbeef"));
    }

    #[test]
    fn state_legacy_json_without_new_fields_deserializes_with_defaults() {
        let legacy_json = r#"{
            "pr": 42,
            "repo": "owner/repo",
            "started_at": "2026-04-01T00:00:00Z",
            "last_checked": null,
            "ci": null,
            "coderabbit": null,
            "action": "continue_monitoring",
            "summary": "legacy",
            "findings": [],
            "notified": false,
            "daemon_pid": null,
            "daemon_status": "running"
        }"#;

        let state: PrMonitorState = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(state.review_recheck_count, 0);
        assert!(state.head_commit.is_none());
        assert!(state.next_wakeup_at_unix.is_none());
        assert!(state.wakeup_reason.is_none());
        assert_eq!(state.rate_limit_retries, 0);
    }

    #[test]
    fn state_serialize_roundtrip_with_review_recheck_count() {
        let mut state = PrMonitorState::new(Some(42), Some("o/r".into()), "t".into());
        state.review_recheck_count = 2;
        state.next_wakeup_at_unix = Some(1_775_088_000);
        state.wakeup_reason = Some("review_recheck".into());

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("review_recheck_count"));
        assert!(json.contains("review_recheck"));

        let deserialized: PrMonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.review_recheck_count, 2);
        assert_eq!(
            deserialized.wakeup_reason.as_deref(),
            Some("review_recheck")
        );
    }

    #[test]
    fn state_default_review_recheck_count_is_zero() {
        let state = PrMonitorState::new(Some(1), None, "t".into());
        assert_eq!(state.review_recheck_count, 0);
    }

    #[test]
    fn state_serialize_roundtrip_with_wakeup_fields() {
        let mut state =
            PrMonitorState::new(Some(42), Some("o/r".into()), "2026-05-05T12:00:00Z".into());
        state.next_wakeup_at_unix = Some(1_775_088_000);
        state.wakeup_reason = Some("rate_limit_retry".into());

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("next_wakeup_at_unix"));
        assert!(json.contains("rate_limit_retry"));

        let deserialized: PrMonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
        assert_eq!(deserialized.next_wakeup_at_unix, Some(1_775_088_000));
        assert_eq!(
            deserialized.wakeup_reason.as_deref(),
            Some("rate_limit_retry")
        );
    }

    #[test]
    fn state_omits_wakeup_fields_when_none() {
        let state = PrMonitorState::new(Some(1), None, "t".into());
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("next_wakeup_at_unix"));
        assert!(!json.contains("wakeup_reason"));
    }

    #[test]
    fn state_default_wakeup_fields_are_none() {
        let state = PrMonitorState::new(Some(1), None, "t".into());
        assert!(state.next_wakeup_at_unix.is_none());
        assert!(state.wakeup_reason.is_none());
    }

    #[test]
    fn state_write_read_roundtrip() {
        let tmp =
            std::env::temp_dir().join(format!("test-state-roundtrip-{}.json", std::process::id()));
        let state = PrMonitorState::new(Some(1), Some("o/r".into()), "2026-01-01T00:00:00Z".into());

        write_state_to(&tmp, &state).unwrap();
        let loaded = read_state_from(&tmp).unwrap();
        assert_eq!(state, loaded);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn state_read_nonexistent_returns_none() {
        let result = read_state_from(Path::new("/tmp/nonexistent-state-file-xyz.json"));
        assert!(result.is_none());
    }

    #[test]
    fn update_state_success() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "status": "complete",
            "action": "stop_monitoring_success",
            "ci": { "overall": "success", "runs": [{"name": "test", "conclusion": "success"}] },
            "coderabbit": { "review_state": "success", "new_comments": 0, "actionable_comments": null, "unresolved_threads": null },
            "summary": "CI成功、指摘なし"
        });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "stop_monitoring_success");
        assert_eq!(state.summary, "CI成功、指摘なし");
        assert!(state.ci.is_some());
        assert_eq!(state.ci.as_ref().unwrap().overall, "success");
    }

    #[test]
    fn update_state_action_required() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "action": "action_required",
            "coderabbit": { "review_state": "changes_requested", "new_comments": 3, "actionable_comments": 2, "unresolved_threads": 1 },
            "summary": "CodeRabbit: 3件の新規コメント"
        });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "action_required");
        let cr = state.coderabbit.as_ref().unwrap();
        assert_eq!(cr.new_comments, 3);
        assert_eq!(cr.actionable_comments, Some(2));
    }

    #[test]
    fn update_state_ci_failure() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "action": "stop_monitoring_failure",
            "ci": { "overall": "failure", "runs": [{"name": "build", "conclusion": "failure"}] },
            "summary": "CI失敗: build"
        });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "stop_monitoring_failure");
        assert_eq!(state.ci.as_ref().unwrap().overall, "failure");
    }

    #[test]
    fn update_state_partial_json() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({ "action": "continue_monitoring" });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.action, "continue_monitoring");
        assert!(state.ci.is_none());
    }

    #[test]
    fn update_state_invalid_ci_preserves_existing() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        state.ci = Some(CiState {
            overall: "success".into(),
            runs: vec![],
        });
        // "ci" キーは存在するが不正な型 (文字列) → デシリアライズ失敗
        let result = serde_json::json!({ "ci": "invalid" });
        update_state_from_check_result(&mut state, &result);
        // 既存の ci が保持されること
        assert_eq!(state.ci.as_ref().unwrap().overall, "success");
    }

    #[test]
    fn update_state_invalid_coderabbit_preserves_existing() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        state.coderabbit = Some(CodeRabbitState {
            review_state: "approved".into(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: None,
        });
        let result = serde_json::json!({ "coderabbit": 42 });
        update_state_from_check_result(&mut state, &result);
        assert_eq!(state.coderabbit.as_ref().unwrap().review_state, "approved");
    }

    #[test]
    fn update_state_populates_rate_limit() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        let result = serde_json::json!({
            "action": "continue_monitoring",
            "rate_limit": {
                "until_unix_secs": 1735689600_i64,
                "comment_created_at": "2026-04-30T00:00:00Z",
                "wait_minutes": 5,
                "wait_seconds": 13
            }
        });
        update_state_from_check_result(&mut state, &result);
        let rl = state.rate_limit.expect("rate_limit must be populated");
        assert_eq!(rl.until_unix_secs, 1_735_689_600);
        assert_eq!(rl.wait_minutes, 5);
        assert_eq!(rl.wait_seconds, 13);
    }

    #[test]
    fn update_state_clears_rate_limit_when_absent() {
        let mut state = PrMonitorState::new(Some(1), None, "t".into());
        // 前回 iteration で rate_limit が設定された状態を再現
        state.rate_limit = Some(RateLimitState {
            until_unix_secs: 1_735_689_600,
            comment_event_time: "2026-04-30T00:00:00Z".into(),
            wait_minutes: 5,
            wait_seconds: 13,
        });
        // rate_limit field を含まない正常 polling 結果
        let result = serde_json::json!({ "action": "continue_monitoring" });
        update_state_from_check_result(&mut state, &result);
        assert!(
            state.rate_limit.is_none(),
            "rate_limit should be cleared when JSON omits the field"
        );
    }

    #[test]
    fn mark_notified_updates_flag() {
        let tmp =
            std::env::temp_dir().join(format!("test-mark-notified-{}.json", std::process::id()));
        let state = PrMonitorState::new(Some(1), None, "t".into());
        write_state_to(&tmp, &state).unwrap();

        let mut loaded = read_state_from(&tmp).unwrap();
        assert!(!loaded.notified);
        loaded.notified = true;
        write_state_to(&tmp, &loaded).unwrap();

        let final_state = read_state_from(&tmp).unwrap();
        assert!(final_state.notified);

        let _ = std::fs::remove_file(&tmp);
    }
}
