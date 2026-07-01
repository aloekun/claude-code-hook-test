use serde::Deserialize;

/// 順位 1 (PR #85 T1-4) — scratch ファイル (`__*` 等) が `@` commit に
/// 混入していないか push 前に検査する stage の config。
///
/// ADR-039 (Experimental feature 標準パターン) § 1 Config opt-in 準拠:
/// `[scratch_file_warning]` section 不在 / `enabled` 未設定 / `enabled = false`
/// のいずれも検査を **skip** (= default `enabled = false`)。明示的に `enabled = true`
/// にしたときのみ検査実行 (3-5 PR の dogfood 後に default-ON 昇格 or 却下を判定)。
///
/// `patterns` は順位 5 (AI 生成一時スクリプト pattern の pre-push 検出) で
/// `_tmp_*` 等の追加 pattern を config-driven で拡張可能 (= 補完アプローチ)。
/// `patterns` 未設定時の default は `["__*"]` (= stage 側 `DEFAULT_PATTERN`)。
#[derive(Deserialize)]
pub(crate) struct ScratchFileWarningConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) patterns: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn config_parses_with_scratch_file_warning_full() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[scratch_file_warning]
enabled = true
patterns = ["__*", "_tmp_*"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let s = config
            .scratch_file_warning
            .expect("[scratch_file_warning] should parse to Some");
        assert_eq!(s.enabled, Some(true));
        assert_eq!(
            s.patterns.unwrap(),
            vec!["__*".to_string(), "_tmp_*".to_string()]
        );
    }

    #[test]
    fn config_parses_with_scratch_file_warning_only_enabled_false() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[scratch_file_warning]
enabled = false

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let s = config.scratch_file_warning.unwrap();
        assert_eq!(s.enabled, Some(false));
        assert!(s.patterns.is_none());
    }

    #[test]
    fn config_scratch_file_warning_absent_yields_none() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(
            config.scratch_file_warning.is_none(),
            "absent [scratch_file_warning] should yield None (default-ON 動作は stage 側で解決)"
        );
    }
}
