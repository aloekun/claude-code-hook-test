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
    pub(crate) notified: bool,
    pub(crate) daemon_pid: Option<u32>,
    pub(crate) daemon_status: String,
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
            notified: false,
            daemon_pid: None,
            daemon_status: "running".to_string(),
        }
    }
}

pub(crate) fn state_file_path() -> PathBuf {
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

#[allow(dead_code)]
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
        state.ci = serde_json::from_value(ci_val.clone()).ok();
    }
    if let Some(cr_val) = result.get("coderabbit") {
        state.coderabbit = serde_json::from_value(cr_val.clone()).ok();
    }
    if let Some(findings_val) = result.get("findings") {
        if let Ok(findings) = serde_json::from_value::<Vec<Finding>>(findings_val.clone()) {
            state.findings = findings;
        }
    }
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
            notified: false,
            daemon_pid: Some(12345),
            daemon_status: "running".into(),
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: PrMonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
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
