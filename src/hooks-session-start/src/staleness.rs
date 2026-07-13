//! 順位 136 案 A: working copy staleness 検出。
//!
//! `[session_start.staleness]` を見て `@-..<default_branch>` の commit 数を
//! `jj git fetch` 後にカウントし、ahead なら nudge を返す。

use std::path::Path;

use crate::hooks_config::StalenessConfig;
use crate::jj_helpers::{
    count_commits_in_revset, fetch_head_is_recent, run_jj_with_timeout, working_copy_is_stale,
};

const STALENESS_DEFAULT_FETCH_TIMEOUT_SECS: u64 = 3;
const STALENESS_DEFAULT_FETCH_CACHE_SECS: u64 = 300;
const STALENESS_DEFAULT_BRANCH: &str = "master";
const STALE_CHECK_TIMEOUT_SECS: u64 = 5;

pub(crate) fn build_staleness_nudge_message(default_branch: &str, behind: usize) -> String {
    format!(
        "[working-copy-freshness]\n\
         {0} は @- より {1} commits ahead です (working copy が {0} に遅れています)。\n\
         推奨: `jj git fetch && jj rebase -d {0}` で最新化、または `jj new {0} -m \"WIP: <description>\"` で新規 commit を {0} 直下に作成",
        default_branch, behind
    )
}

pub(crate) fn compute_staleness_nudge(
    repo_root: &Path,
    config: &StalenessConfig,
) -> Option<String> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let default_branch = config
        .default_branch
        .as_deref()
        .unwrap_or(STALENESS_DEFAULT_BRANCH);
    let fetch_timeout = config
        .fetch_timeout_secs
        .unwrap_or(STALENESS_DEFAULT_FETCH_TIMEOUT_SECS);
    let fetch_cache = config
        .fetch_cache_secs
        .unwrap_or(STALENESS_DEFAULT_FETCH_CACHE_SECS);

    if !fetch_head_is_recent(repo_root, fetch_cache) {
        let _ = run_jj_with_timeout(&["git", "fetch", "--quiet"], fetch_timeout);
    }

    let revset = format!("@-..{}", default_branch);
    let behind = count_commits_in_revset(&revset)?;
    if behind == 0 {
        return None;
    }
    Some(build_staleness_nudge_message(default_branch, behind))
}

pub(crate) fn build_workspace_stale_nudge_message() -> String {
    "[workspace-stale]\n\
     この workspace の working copy は stale です (別 workspace の操作で repo view から取り残されています)。\n\
     対処: `jj workspace update-stale` を実行してください。recovery commit が作られ、working copy 上の変更は失われません (ADR-045 § Known operational risks)。"
        .to_string()
}

/// workspace stale 検知 nudge (ADR-045 事故 follow-up、C2)。
///
/// 並列 workspace 運用で別 workspace の操作によりこちらの working copy が stale に
/// なったとき、セッション開始時点で `jj workspace update-stale` を促す。検知は
/// `working_copy_is_stale` (stderr の stale エラー文言) による。自動実行はしない
/// (recovery commit を勝手に作らない)。
///
/// opt-in (ADR-039 § 1): `[session_start.staleness] stale_check_enabled = true` で有効化。
pub(crate) fn compute_workspace_stale_nudge(config: &StalenessConfig) -> Option<String> {
    if !config.stale_check_enabled.unwrap_or(false) {
        return None;
    }
    if !working_copy_is_stale(STALE_CHECK_TIMEOUT_SECS) {
        return None;
    }
    Some(build_workspace_stale_nudge_message())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "staleness-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn staleness_nudge_message_includes_branch_and_count() {
        let msg = build_staleness_nudge_message("master", 3);
        assert!(msg.contains("[working-copy-freshness]"));
        assert!(msg.contains("master"));
        assert!(msg.contains("3 commits ahead"));
        assert!(msg.contains("jj git fetch"));
        assert!(msg.contains("jj rebase -d master"));
    }

    #[test]
    fn staleness_nudge_message_supports_main_branch_alias() {
        let msg = build_staleness_nudge_message("main", 1);
        assert!(msg.contains("main"));
        assert!(msg.contains("1 commits ahead"));
        assert!(!msg.contains("master"));
    }

    fn config_with(enabled: Option<bool>, stale_check_enabled: Option<bool>) -> StalenessConfig {
        StalenessConfig {
            enabled,
            fetch_timeout_secs: None,
            fetch_cache_secs: None,
            default_branch: None,
            stale_check_enabled,
        }
    }

    #[test]
    fn compute_staleness_nudge_returns_none_when_disabled() {
        let config = config_with(Some(false), None);
        let root = unique_temp_root("disabled");
        let result = compute_staleness_nudge(&root, &config);
        assert!(result.is_none());
    }

    #[test]
    fn compute_staleness_nudge_returns_none_when_enabled_field_missing() {
        let config = config_with(None, None);
        let root = unique_temp_root("default-off");
        let result = compute_staleness_nudge(&root, &config);
        assert!(result.is_none(), "ADR-039 § 1 準拠で default-OFF 動作");
    }

    #[test]
    fn workspace_stale_nudge_message_includes_recovery_command() {
        let msg = build_workspace_stale_nudge_message();
        assert!(msg.contains("[workspace-stale]"));
        assert!(msg.contains("jj workspace update-stale"));
        assert!(msg.contains("recovery commit"));
    }

    /// ADR-039 § 1: stale_check は default-OFF。未設定 / false では jj を呼ばず None。
    #[test]
    fn compute_workspace_stale_nudge_returns_none_when_disabled() {
        assert!(compute_workspace_stale_nudge(&config_with(Some(true), None)).is_none());
        assert!(compute_workspace_stale_nudge(&config_with(Some(true), Some(false))).is_none());
    }
}
