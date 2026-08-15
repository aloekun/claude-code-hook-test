use serde::Deserialize;

/// 台帳タスクの実装完了を push 前に検証する stage の config。
///
/// commit description の `Ledger-Rank: N` trailer が宣言する順位について、台帳の
/// 「対象ファイル」列が挙げる成果物すべてが PR 範囲で変更されているかを
/// `cli-ledger-cleanup` に判定させる。
///
/// [ADR-039](../../../../docs/adr/adr-039-experimental-feature-standard-pattern.md)
/// § 1 Config opt-in 準拠: section 不在 / `enabled` 未設定 / `enabled = false` はいずれも
/// **skip**。trailer が無い push も skip なので、既存の push は挙動不変で通る。
///
/// `exe` は `cli-ledger-cleanup` の実行パス。deploy 済み exe の場所は派生プロジェクトで
/// 異なるため config-driven にする (他 stage が `pnpm` script を呼ぶのと同じ理由)。
#[derive(Deserialize)]
pub(crate) struct LedgerCompletionConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) exe: Option<String>,
    pub(crate) ledger: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    fn config_with(section: &str) -> Config {
        let toml_str = format!(
            r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

{section}

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#
        );
        toml::from_str(&toml_str).expect("config should parse")
    }

    #[test]
    fn config_parses_with_all_fields() {
        let config = config_with(
            r#"
[ledger_completion]
enabled = true
exe = ".claude/cli-ledger-cleanup.exe"
ledger = "docs/claude-code-web-tasks.md"
"#,
        );
        let section = config
            .ledger_completion
            .expect("[ledger_completion] should parse to Some");
        assert_eq!(section.enabled, Some(true));
        assert_eq!(section.exe.unwrap(), ".claude/cli-ledger-cleanup.exe");
        assert_eq!(section.ledger.unwrap(), "docs/claude-code-web-tasks.md");
    }

    #[test]
    fn config_parses_with_only_enabled_false() {
        let config = config_with(
            r#"
[ledger_completion]
enabled = false
"#,
        );
        let section = config.ledger_completion.unwrap();
        assert_eq!(section.enabled, Some(false));
        assert!(section.exe.is_none());
        assert!(section.ledger.is_none());
    }

    /// section 不在は `None`。既存 config を触らずに push が通ることを固定する。
    #[test]
    fn absent_section_yields_none() {
        let config = config_with("");
        assert!(config.ledger_completion.is_none());
    }
}
