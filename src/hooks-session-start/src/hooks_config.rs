//! `.claude/hooks-config.toml` の deserialization (session_start section)。
//!
//! 各 feature の config struct (StalenessConfig / WeeklyReviewReminderConfig) と
//! repo root からの読込関数を提供する。`[features].enabled` allow-list は
//! 本 crate ではなく `lib-hooks-config` で扱う (PR-3b で導入予定)。

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// 順位 136 案 A: working copy staleness 検出設定 (ADR-039 experimental pattern)。
///
/// `[session_start.staleness]` section 不在 / `enabled` 未設定 / `enabled = false`
/// では完全 skip (default-OFF in source、repo config で明示 enable する)。
///
/// fail-open: `jj git fetch` / `jj log` の失敗時は warning ログを出さず通過する
/// (network 異常 / fetch timeout で session 起動を阻害しない)。
#[derive(Deserialize)]
pub(crate) struct StalenessConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) fetch_timeout_secs: Option<u64>,
    pub(crate) fetch_cache_secs: Option<u64>,
    pub(crate) default_branch: Option<String>,
    /// workspace stale 検知 nudge (ADR-045 事故 follow-up、C2)。default-OFF (ADR-039 § 1)。
    pub(crate) stale_check_enabled: Option<bool>,
}

/// ADR-031 Phase C: `/weekly-review` skill 起動 promote 設定 (試験運用、ADR-039 experimental pattern)。
///
/// `[session_start.weekly_review_reminder]` section 不在 / `enabled` 未設定 /
/// `enabled = false` では完全 skip (default-OFF in source、repo config で明示 enable する)。
///
/// 2 種類の reminder を発火:
///   - last-run staleness: メイン workspace の `.claude/weekly-review-last-run.json` の
///     `last_run_at` (内容 timestamp。mtime ではない — CR #233 / ADR-045 PR-N2 で canonical 化) が
///     `reminder_threshold_days` を超えていれば「`/weekly-review` の実行を検討」を nudge
///   - failed marker: `.claude/weekly-reviews/*.md.failed` が 1 件以上存在すれば
///     「前回 weekly-review が失敗、`/weekly-review` で resume」を nudge
///
/// fail-open: ファイル読込失敗時は warning なしで通過する (session 起動阻害しない)。
#[derive(Deserialize)]
pub(crate) struct WeeklyReviewReminderConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) reminder_threshold_days: Option<u64>,
    pub(crate) failed_marker_check_enabled: Option<bool>,
    /// systemMessage (ユーザー可視 1 行、ADR-059) を出すか。source default OFF
    /// (`unwrap_or(false)`)。`false` でも additionalContext の nudge は継続する
    /// (systemMessage のみを止める kill-switch)。`enabled = false` は nudge 自体を止める。
    pub(crate) system_message_enabled: Option<bool>,
}

#[derive(Deserialize, Default)]
pub(crate) struct SessionStartConfig {
    pub(crate) staleness: Option<StalenessConfig>,
    pub(crate) weekly_review_reminder: Option<WeeklyReviewReminderConfig>,
}

#[derive(Deserialize, Default)]
pub(crate) struct HooksConfig {
    pub(crate) session_start: Option<SessionStartConfig>,
}

fn hooks_config_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".claude").join("hooks-config.toml")
}

pub(crate) fn read_hooks_config(repo_root: &Path) -> HooksConfig {
    match std::fs::read_to_string(hooks_config_path(repo_root)) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => HooksConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "hooks-config-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn hooks_config_returns_default_when_file_missing() {
        let root = unique_temp_root("missing");
        let config = read_hooks_config(&root);
        assert!(config.session_start.is_none());
    }

    #[test]
    fn hooks_config_parses_session_start_staleness_section() {
        use std::io::Write;
        let root = unique_temp_root("staleness");
        let claude_dir = root.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let toml_str = r#"
[session_start.staleness]
enabled = true
fetch_timeout_secs = 5
default_branch = "main"
"#;
        let mut f = std::fs::File::create(claude_dir.join("hooks-config.toml")).unwrap();
        f.write_all(toml_str.as_bytes()).unwrap();
        drop(f);
        let config = read_hooks_config(&root);
        let staleness = config
            .session_start
            .as_ref()
            .and_then(|s| s.staleness.as_ref())
            .expect("staleness section should parse");
        assert_eq!(staleness.enabled, Some(true));
        assert_eq!(staleness.fetch_timeout_secs, Some(5));
        assert_eq!(staleness.default_branch.as_deref(), Some("main"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hooks_config_parses_session_start_weekly_review_reminder_section() {
        use std::io::Write;
        let root = unique_temp_root("weekly");
        let claude_dir = root.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let toml_str = r#"
[session_start.weekly_review_reminder]
enabled = true
reminder_threshold_days = 14
failed_marker_check_enabled = false
system_message_enabled = true
"#;
        let mut f = std::fs::File::create(claude_dir.join("hooks-config.toml")).unwrap();
        f.write_all(toml_str.as_bytes()).unwrap();
        drop(f);
        let config = read_hooks_config(&root);
        let weekly = config
            .session_start
            .as_ref()
            .and_then(|s| s.weekly_review_reminder.as_ref())
            .expect("weekly_review_reminder section should parse");
        assert_eq!(weekly.enabled, Some(true));
        assert_eq!(weekly.reminder_threshold_days, Some(14));
        assert_eq!(weekly.failed_marker_check_enabled, Some(false));
        assert_eq!(weekly.system_message_enabled, Some(true));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn weekly_review_system_message_enabled_defaults_to_none_when_omitted() {
        use std::io::Write;
        let root = unique_temp_root("weekly-no-sysmsg");
        let claude_dir = root.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let toml_str = r#"
[session_start.weekly_review_reminder]
enabled = true
"#;
        let mut f = std::fs::File::create(claude_dir.join("hooks-config.toml")).unwrap();
        f.write_all(toml_str.as_bytes()).unwrap();
        drop(f);
        let config = read_hooks_config(&root);
        let weekly = config
            .session_start
            .as_ref()
            .and_then(|s| s.weekly_review_reminder.as_ref())
            .expect("weekly_review_reminder section should parse");
        assert_eq!(
            weekly.system_message_enabled, None,
            "system_message_enabled 未設定は None (source default OFF、ADR-059)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
