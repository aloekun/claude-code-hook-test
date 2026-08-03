//! `[telemetry_report]` 設定 (exe 隣接 `hooks-config.toml`) のパース。
//!
//! ADR-039 opt-in 契約に従い section 不在ではレポート生成のみ・削除系 (retention) は
//! default OFF (`retention_days` 未設定 = 削除無効)。判定閾値 `zero_streak_months` は既定 2
//! (ユーザー決定事項 1)。機構マッピング [`MechanismConfig`] は「機構 → 監視 id 群 →
//! enabled 判定 key → 配備 exe → 提案」の静的対応で、判定候補 promote の素になる。

use serde::Deserialize;

/// 非アクティブ化提案の既定閾値 (連続 N か月発火 0)。ユーザー決定事項 1。
pub const DEFAULT_ZERO_STREAK_MONTHS: u64 = 2;
/// レポートの発火 0 リスト / 月別表が遡る既定月数。
pub const DEFAULT_TREND_MONTHS: u64 = 6;

/// hooks-config.toml のトップレベル (telemetry_report section のみ関心)。
#[derive(Debug, Default, Deserialize)]
pub struct RootConfig {
    pub telemetry_report: Option<TelemetryReportConfig>,
}

/// `[telemetry_report]` の設定値。すべて任意で、欠落時は保守側 (削除無効・既定閾値) に倒す。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct TelemetryReportConfig {
    /// N 日超過の raw firing partition を削除する retention。未設定 = 削除無効 (ADR-039 opt-in)。
    pub retention_days: Option<u64>,
    /// 非アクティブ化候補として promote する連続発火 0 月数 (既定 2)。
    pub zero_streak_months: Option<u64>,
    /// レポートが遡る月数 (既定 6)。
    pub trend_months: Option<u64>,
    /// `jj workspace list` で発見できない root を補う追加 root (絶対パス)。
    #[serde(default)]
    pub extra_roots: Vec<String>,
    /// 発火 0 リストの母集合を与える機構レジストリ (設計決定 1、Phase A)。section 不在でも
    /// rule / preset は自動列挙されるため、hook / nudge の静的 id リストのみを持つ (ADR-039 additive)。
    #[serde(default)]
    pub registry: RegistryConfig,
    /// 判定候補マッピング (機構ごと)。
    #[serde(default)]
    pub mechanisms: Vec<MechanismConfig>,
}

/// `[telemetry_report.registry]` の設定値。自動列挙元が無い hook / nudge 発火 id を静的に列挙する
/// (設計決定 1 § hook / nudge)。id は hook 名と一致しない例がある (`jj-op-verify` /
/// `weekly_review_reminder` / `hooks-stop-tool-call-leak/prompt-recovery` 等、各 hook の
/// `lib_telemetry::record` 実装で確認)。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RegistryConfig {
    /// hook / nudge 発火 id の静的リスト。
    #[serde(default)]
    pub hook_ids: Vec<String>,
}

impl TelemetryReportConfig {
    /// 有効閾値 (未設定は既定 2)。
    pub fn zero_streak_months(&self) -> u64 {
        self.zero_streak_months.unwrap_or(DEFAULT_ZERO_STREAK_MONTHS)
    }

    /// 有効トレンド窓月数 (未設定は既定 6、下限 1)。
    pub fn trend_months(&self) -> u64 {
        self.trend_months.unwrap_or(DEFAULT_TREND_MONTHS).max(1)
    }
}

/// 1 機構の判定マッピング。設計決定 2 § 判定候補 の静的対応を表す。
#[derive(Debug, Clone, Deserialize)]
pub struct MechanismConfig {
    /// 機構名 (例 `stop_tool_call_leak`)。snapshot / verdict のキー。
    pub name: String,
    /// 由来 ADR (例 `ADR-053/061`)。レポート表示用。
    #[serde(default)]
    pub adr: String,
    /// 監視対象の firing id 群 (この全 id が連続発火 0 で候補成立)。
    pub ids: Vec<String>,
    /// enabled 判定に使う hooks-config.toml の `"section.key"` 群。
    #[serde(default)]
    pub enabled_config_keys: Vec<String>,
    /// 配備確認する exe 名 (`.claude/<name>` の存在)。
    #[serde(default)]
    pub exe_names: Vec<String>,
    /// 成立時にレポートへ出す提案文。
    #[serde(default)]
    pub proposal: String,
}

/// TOML 文字列から `[telemetry_report]` を取り出す。parse 失敗・section 不在は
/// `TelemetryReportConfig::default()` (レポートのみ・削除無効) に fail-open する。
pub fn parse_config(content: &str) -> TelemetryReportConfig {
    toml::from_str::<RootConfig>(content)
        .ok()
        .and_then(|c| c.telemetry_report)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_section_defaults_to_report_only() {
        let cfg = parse_config("[other]\nx = 1\n");
        assert!(cfg.retention_days.is_none(), "retention は default OFF");
        assert_eq!(cfg.zero_streak_months(), 2);
        assert_eq!(cfg.trend_months(), 6);
        assert!(cfg.mechanisms.is_empty());
        assert!(cfg.extra_roots.is_empty());
    }

    #[test]
    fn malformed_toml_fails_open_to_default() {
        let cfg = parse_config("this is not toml =[");
        assert!(cfg.retention_days.is_none());
        assert_eq!(cfg.zero_streak_months(), 2);
    }

    #[test]
    fn parses_full_mechanism_mapping() {
        let toml = r#"
[telemetry_report]
retention_days = 90
zero_streak_months = 2
trend_months = 6
extra_roots = ["C:\\work\\main"]

[[telemetry_report.mechanisms]]
name = "stop_tool_call_leak"
adr = "ADR-053/061"
ids = ["hooks-stop-tool-call-leak", "hooks-stop-tool-call-leak/prompt-recovery"]
enabled_config_keys = ["stop_tool_call_leak.enabled", "stop_tool_call_leak.prompt_recovery_enabled"]
exe_names = ["hooks-stop-tool-call-leak"]
proposal = "enabled = false"
"#;
        let cfg = parse_config(toml);
        assert_eq!(cfg.retention_days, Some(90));
        assert_eq!(cfg.extra_roots, vec!["C:\\work\\main".to_string()]);
        assert_eq!(cfg.mechanisms.len(), 1);
        let m = &cfg.mechanisms[0];
        assert_eq!(m.name, "stop_tool_call_leak");
        assert_eq!(m.ids.len(), 2);
        assert_eq!(m.enabled_config_keys.len(), 2);
        assert_eq!(m.exe_names, vec!["hooks-stop-tool-call-leak".to_string()]);
        assert_eq!(m.proposal, "enabled = false");
    }

    #[test]
    fn trend_months_has_floor_of_one() {
        let cfg = parse_config("[telemetry_report]\ntrend_months = 0\n");
        assert_eq!(cfg.trend_months(), 1);
    }

    #[test]
    fn registry_hook_ids_default_empty_when_absent() {
        let cfg = parse_config("[telemetry_report]\n");
        assert!(cfg.registry.hook_ids.is_empty());
    }

    #[test]
    fn parses_registry_hook_ids() {
        let toml = r#"
[telemetry_report]

[telemetry_report.registry]
hook_ids = ["file-length", "reaper", "hooks-stop-tool-call-leak/prompt-recovery"]
"#;
        let cfg = parse_config(toml);
        assert_eq!(
            cfg.registry.hook_ids,
            vec![
                "file-length".to_string(),
                "reaper".to_string(),
                "hooks-stop-tool-call-leak/prompt-recovery".to_string(),
            ]
        );
    }
}
