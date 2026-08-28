use serde::Deserialize;

/// 機1 (testability gate) の config。ADR-076 / defect-convergence-plan.md § Phase 1。
///
/// ADR-039 (Experimental feature 標準パターン) 3 点セット準拠:
/// - **Config opt-in**: 試験運用のため default `enabled = false`。section 不在 /
///   `enabled` 未設定 / `false` のいずれも完全 skip。
/// - **Kill-switch**: `enabled = false` (恒久停止) + env `TESTABILITY_GATE_OVERRIDE=1`
///   (個別 push の意図的バイパス)。
/// - **Bounded lifetime**: 導入時は `mode = "warning"` 固定で 4 週間発火を観測し、
///   FP 率 < 10% なら `mode = "deny"` へ昇格、超えるなら検出条件を絞るか機構を物理削除する。
///   判定は monthly-review に載せ、結果は ADR-076 に記録する。
#[derive(Deserialize)]
pub(crate) struct TestabilityGateConfig {
    pub(crate) enabled: Option<bool>,
    /// `"warning"` (既定) なら push を通し、`"deny"` なら止める。
    pub(crate) mode: Option<String>,
}

/// `[testability_gate] mode` に許す値。
pub(crate) const TESTABILITY_GATE_MODES: &[&str] = &["warning", "deny"];

/// 導入時の既定モード。昇格判定までは warning から変えない。
pub(crate) const DEFAULT_TESTABILITY_GATE_MODE: &str = "warning";

/// `mode` の値を config-load 時に検証する。
///
/// **未知の値を warning へ倒さない** (CodeRabbit #456)。`mode` は文字列なので `"denny"` の
/// ような typo も parse は通る。「`"deny"` 以外は warning」と解釈すると、deny を意図した
/// 設定が**黙って無効**になる — 昇格後にこれが起きると gate が在るのに何も止めない状態を
/// 誰も気づけない。config エラーとして即座に落とす ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
pub(crate) fn validate_testability_gate_mode(
    config: Option<&TestabilityGateConfig>,
) -> Result<(), String> {
    let Some(mode) = config.and_then(|c| c.mode.as_deref()) else {
        return Ok(());
    };
    if TESTABILITY_GATE_MODES.contains(&mode) {
        return Ok(());
    }
    Err(format!(
        "設定ファイルエラー: [testability_gate] mode は {} のいずれか (got: {mode})",
        TESTABILITY_GATE_MODES.join(" / ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const BASE: &str = r#"
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

    #[test]
    fn config_parses_testability_gate_section() {
        let toml_str = format!("{BASE}\n[testability_gate]\nenabled = true\nmode = \"deny\"\n");
        let config: Config = toml::from_str(&toml_str).unwrap();
        let s = config.testability_gate.expect("section should parse");
        assert_eq!(s.enabled, Some(true));
        assert_eq!(s.mode.as_deref(), Some("deny"));
    }

    #[test]
    fn absent_section_yields_none() {
        let config: Config = toml::from_str(BASE).unwrap();
        assert!(
            config.testability_gate.is_none(),
            "section 不在は default OFF レーン"
        );
    }

    #[test]
    fn unknown_mode_is_a_config_error() {
        let config = TestabilityGateConfig {
            enabled: Some(true),
            mode: Some("denny".to_string()),
        };
        let err = validate_testability_gate_mode(Some(&config)).unwrap_err();
        assert!(err.contains("denny"), "{err}");
        assert!(err.contains("warning"), "{err}");
    }

    #[test]
    fn known_modes_and_absent_mode_are_accepted() {
        for mode in ["warning", "deny"] {
            let config = TestabilityGateConfig {
                enabled: Some(true),
                mode: Some(mode.to_string()),
            };
            assert!(validate_testability_gate_mode(Some(&config)).is_ok(), "{mode}");
        }
        let no_mode = TestabilityGateConfig {
            enabled: Some(true),
            mode: None,
        };
        assert!(validate_testability_gate_mode(Some(&no_mode)).is_ok());
        assert!(validate_testability_gate_mode(None).is_ok());
    }
}
