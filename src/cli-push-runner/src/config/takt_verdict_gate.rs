use serde::Deserialize;

/// 順位 499 (takt verdict gate) の config。ADR-078。
///
/// ADR-039 (Experimental feature 標準パターン) 3 点セット:
/// - **Config**: **既定で有効** (section 不在 / `enabled` 未設定も有効)。塞ぐのは
///   「レビューが REJECT でも push される」既知の穴であり、warning 期間を置く意味がない。
/// - **Kill-switch**: `enabled = false` で恒久停止 + env `TAKT_VERDICT_GATE_OVERRIDE=1` で
///   個別 push のバイパス。**指摘が妥当でも fix step が権限上直せない場合**、人が手で直した
///   後に押し切る経路として要る (順位 499 の incident がまさにこの形だった)。
/// - **Bounded lifetime**: 3〜5 PR の dogfood で「REJECT を実際に止めた回数」と
///   「バイパス頻度」を観測し、ADR-078 に記録する。バイパスが常用されるなら、
///   fix step が編集できる zone を広げる案へ再検討する。
#[derive(Deserialize)]
pub(crate) struct TaktVerdictGateConfig {
    pub(crate) enabled: Option<bool>,
}

#[cfg(test)]
mod tests {
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
    fn config_parses_takt_verdict_gate_section() {
        let toml_str = format!("{BASE}\n[takt_verdict_gate]\nenabled = false\n");
        let config: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            config
                .takt_verdict_gate
                .expect("section should parse")
                .enabled,
            Some(false)
        );
    }

    #[test]
    fn absent_section_yields_none() {
        let config: Config = toml::from_str(BASE).unwrap();
        assert!(config.takt_verdict_gate.is_none());
    }
}
