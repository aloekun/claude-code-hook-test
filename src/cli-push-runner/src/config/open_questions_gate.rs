use serde::Deserialize;

/// 機2 (open-questions gate) の config。ADR-077 / defect-convergence-plan.md § Phase 2。
///
/// ADR-039 (Experimental feature 標準パターン) 3 点セット:
/// - **Config opt-in**: 本 gate は**既定で有効** (section 不在 / `enabled` 未設定 = 有効)。
///   機1 (warning から始める新検出) と違い、判定は「ファイルに問いが書いてあるか」だけで
///   誤検出の余地が無く、書いた本人が消せば通るためである。
/// - **Kill-switch**: `enabled = false` で恒久停止 + env `OPEN_QUESTIONS_GATE_OVERRIDE=1` で
///   個別 push の意図的バイパス (確認より先に push する必要があるとき)。
/// - **Bounded lifetime**: 3〜5 PR の dogfood で「問いが実際に書かれるか」「バイパスの
///   頻度」を観測し、本採用 / 修正 / 却下を判定する。判定結果は ADR-077 に書く。
///   **問いが 1 件も書かれないまま dogfood 期間が終わった場合は却下**し、機構を物理削除する
///   (ADR-042 § Mechanism graveyard prevention)。
#[derive(Deserialize)]
pub(crate) struct OpenQuestionsGateConfig {
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
    fn config_parses_open_questions_gate_section() {
        let toml_str = format!("{BASE}\n[open_questions_gate]\nenabled = false\n");
        let config: Config = toml::from_str(&toml_str).unwrap();
        let section = config.open_questions_gate.expect("section should parse");
        assert_eq!(section.enabled, Some(false));
    }

    /// section 不在でも gate は有効 (既定 ON レーン)。
    #[test]
    fn absent_section_yields_none_and_stays_enabled() {
        let config: Config = toml::from_str(BASE).unwrap();
        assert!(config.open_questions_gate.is_none());
    }
}
