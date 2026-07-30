//! incident 由来ルール id の抽出 (設計決定 2d、ADR-049)。
//!
//! `.claude/custom-lint-rules.toml` の `[[rules]]` のうち `[rules.incident]` サブテーブルを
//! 持つルールは「実 incident 由来」であり、発火 0 でも抑止力として維持推奨とする (ADR-049 の
//! 思想)。真実源を custom-lint-rules.toml 側に一本化し、本 exe には id リストを複製しない。

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

#[derive(Deserialize)]
struct RulesFile {
    #[serde(default)]
    rules: Vec<RuleEntry>,
}

#[derive(Deserialize)]
struct RuleEntry {
    id: String,
    incident: Option<toml::Value>,
}

/// `config_base/custom-lint-rules.toml` から incident 由来ルール id 集合を読む。
/// ファイル不在 / parse 失敗は空集合 (fail-open、維持推奨マークが付かないだけ)。
pub fn incident_rule_ids(config_base: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(config_base.join("custom-lint-rules.toml"))
        .ok()
        .map(|c| incident_ids_from_str(&c))
        .unwrap_or_default()
}

/// TOML 文字列から `[rules.incident]` を持つルール id を抽出する (pure)。
pub fn incident_ids_from_str(content: &str) -> BTreeSet<String> {
    let Ok(parsed) = toml::from_str::<RulesFile>(content) else {
        return BTreeSet::new();
    };
    parsed
        .rules
        .into_iter()
        .filter(|r| r.incident.is_some())
        .map(|r| r.id)
        .collect()
}

/// `config_base/custom-lint-rules.toml` から**全** rule id を読む (レジストリの rule 供給源、
/// 設計決定 1)。incident 判定と同じ読み口 ([`RulesFile`]) を共有する。
///
/// ファイル不在 / parse 失敗は `None` を返す。[`incident_ids_from_str`] の fail-open (空集合) とは
/// 異なり、レジストリは供給源欠落を「never-fired 判定不能」としてレポートに明示するため、
/// 「読めなかった」と「rule が 0 本」を区別する必要がある。
pub fn read_all_rule_ids(config_base: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(config_base.join("custom-lint-rules.toml")).ok()?;
    all_rule_ids_from_str(&content)
}

/// TOML 文字列から全 rule id を抽出する (pure)。parse 失敗は `None`。
pub fn all_rule_ids_from_str(content: &str) -> Option<Vec<String>> {
    let parsed = toml::from_str::<RulesFile>(content).ok()?;
    Some(parsed.rules.into_iter().map(|r| r.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_rules_with_incident_subtable() {
        let toml = r#"
[[rules]]
id = "no-console-log"

[[rules]]
id = "no-personal-paths"
[rules.incident]
pr = "PR #200"

[[rules]]
id = "no-mutable-anchor"
[rules.incident]
pr = "PR #x"
"#;
        let ids = incident_ids_from_str(toml);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("no-personal-paths"));
        assert!(ids.contains("no-mutable-anchor"));
        assert!(!ids.contains("no-console-log"), "incident 無しは含めない");
    }

    #[test]
    fn malformed_toml_yields_empty_set() {
        assert!(incident_ids_from_str("not = [ toml").is_empty());
    }

    #[test]
    fn all_rule_ids_lists_every_rule_regardless_of_incident() {
        let toml = r#"
[[rules]]
id = "no-console-log"

[[rules]]
id = "no-personal-paths"
[rules.incident]
pr = 75
"#;
        let ids = all_rule_ids_from_str(toml).unwrap();
        assert_eq!(ids, vec!["no-console-log".to_string(), "no-personal-paths".to_string()]);
    }

    #[test]
    fn all_rule_ids_returns_none_on_parse_failure() {
        assert!(all_rule_ids_from_str("not = [ toml").is_none());
    }
}
