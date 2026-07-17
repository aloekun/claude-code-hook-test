use serde::Deserialize;

use super::Config;

pub(crate) const DEFAULT_LINT_SCREEN_TIMEOUT_SECS: u64 = 60;
pub(crate) const DEFAULT_LINT_SCREEN_MAX_DIFF_LINES: usize = 5000;
pub(crate) const DEFAULT_LINT_SCREEN_MODEL: &str = "mistral:7b";
pub(crate) const DEFAULT_LINT_SCREEN_ENDPOINT: &str = "http://localhost:11434";
pub(crate) const DEFAULT_LINT_SCREEN_EXE_PATH: &str = ".claude/cli-finding-classifier.exe";
pub(crate) const DEFAULT_LINT_SCREEN_OUTPUT_PATH: &str = ".takt/lint-screen-report.md";

/// `LINT_SCREEN_ENABLED` env var の名前 (順位 115、Phase D D-1 workflow gap 解消)。
///
/// 用途: session-only opt-in (jj auto-snapshot 環境で `push-runner-config.toml` を編集せずに
/// lint_screen を一時的に有効化する)。
///
/// **解釈** (todo entry 順位 115 設計決定に基づく):
/// - `"true"` / `"1"` / `"yes"` (case-insensitive、空白 trim) → **force enable** (TOML override)
/// - `"false"` / `"0"` / `"no"` / `""` / unset → **TOML 値を尊重** (override しない、no-op)
/// - その他の値 → warning emit + TOML 値を尊重 (= invalid 扱い、安全側に倒す)
///
/// **片方向設計の意図**: env を temporary に set すれば session opt-in、unset すれば TOML default
/// (= `enabled = false`) に自然復帰する。誤って commit しても remote PR は config 上 OFF のまま
/// (= dogfood は走らない) なので、Phase D guide §1 の「local enable / remote disable」が成立する。
pub(crate) const ENV_LINT_SCREEN_ENABLED: &str = "LINT_SCREEN_ENABLED";

/// Phase c (§8.E lint screen facet) — pre-push 時に diff を mistral:7b に流して
/// lint 一次フィルタの所見を `.takt/lint-screen-report.md` として出力する。
///
/// `enabled = false` の場合は完全 no-op (default OFF, 試験運用)。
/// Ollama down / timeout / diff 過大時は skip + warn (push を block しない)。
#[derive(Deserialize)]
pub(crate) struct LintScreenConfig {
    pub(crate) enabled: bool,
    pub(crate) exe_path: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) timeout_secs: Option<u64>,
    pub(crate) max_diff_lines: Option<usize>,
    pub(crate) output_path: Option<String>,
}

/// `LINT_SCREEN_ENABLED` env var を解釈した結果。
///
/// `parse_lint_screen_env` の戻り値で、`apply_lint_screen_env_override` が分岐する。
#[derive(Debug, PartialEq, Eq)]
enum LintScreenEnvOverride {
    /// env が `true`/`1`/`yes` 系 → TOML 値を上書きして `enabled = true` を強制。
    ForceEnable,
    /// env が `false`/`0`/`no`/`""`/unset → TOML 値を尊重 (no-op)。
    RespectToml,
    /// env が解釈不能な文字列 → warning emit 候補、安全側で TOML 値を尊重。
    InvalidValue,
}

/// `LINT_SCREEN_ENABLED` env var の生文字列を解釈する純粋関数 (test 容易性のため env 読み取りと分離)。
///
/// `None` (unset) は `RespectToml` として扱う。空白 trim + 小文字化して比較する。
fn parse_lint_screen_env(raw: Option<&str>) -> LintScreenEnvOverride {
    let Some(value) = raw else {
        return LintScreenEnvOverride::RespectToml;
    };
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "true" | "1" | "yes" | "on" => LintScreenEnvOverride::ForceEnable,
        "false" | "0" | "no" | "off" | "" => LintScreenEnvOverride::RespectToml,
        _ => LintScreenEnvOverride::InvalidValue,
    }
}

/// `LINT_SCREEN_ENABLED` env var の値を `config.lint_screen.enabled` に反映する。
///
/// 設計仕様は `ENV_LINT_SCREEN_ENABLED` の doc comment 参照。`[lint_screen]` section が
/// TOML に存在しなくても、env が `ForceEnable` の場合は default 値で `LintScreenConfig` を
/// 生成する (Phase D D-1 で発見した workflow gap の解消、順位 115)。
///
/// `raw` 引数は test 容易性のため caller が `std::env::var(...)` を解決して渡す。
pub(super) fn apply_lint_screen_env_override(config: &mut Config, raw: Option<String>) {
    let raw_ref = raw.as_deref();
    match parse_lint_screen_env(raw_ref) {
        LintScreenEnvOverride::ForceEnable => {
            match config.lint_screen.as_mut() {
                Some(lint) => {
                    lint.enabled = true;
                }
                None => {
                    config.lint_screen = Some(default_lint_screen_enabled());
                }
            }
            eprintln!(
                "[push-runner] {}: TOML override で [lint_screen] enabled を true に強制 (順位 115 env override)",
                ENV_LINT_SCREEN_ENABLED
            );
        }
        LintScreenEnvOverride::RespectToml => {}
        LintScreenEnvOverride::InvalidValue => {
            eprintln!(
                "[push-runner] WARN: {}='{}' を bool として解釈できません、TOML 値を尊重します。\
                 受容値: true/1/yes/on (enable) / false/0/no/off/\"\" (TOML 尊重)",
                ENV_LINT_SCREEN_ENABLED,
                raw_ref.unwrap_or("")
            );
        }
    }
}

/// env override で `[lint_screen]` section を新規生成する際の default 値。
///
/// 他の field は `None` のままで、`stages::lint_screen` の `resolve_invoke_params` / `run_lint_screen`
/// 側が `DEFAULT_LINT_SCREEN_*` 定数で fallback する。
fn default_lint_screen_enabled() -> LintScreenConfig {
    LintScreenConfig {
        enabled: true,
        exe_path: None,
        model: None,
        endpoint: None,
        timeout_secs: None,
        max_diff_lines: None,
        output_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupConfig, PushConfig, QualityGateConfig, TaktConfig};

    #[test]
    fn config_parses_with_lint_screen_section_full_fields() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[lint_screen]
enabled = true
exe_path = ".claude/cli-finding-classifier.exe"
model = "mistral:7b"
endpoint = "http://localhost:11434"
timeout_secs = 90
max_diff_lines = 4000
output_path = ".takt/lint-screen-report.md"

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        let lint = config
            .lint_screen
            .expect("[lint_screen] section should produce Some(LintScreenConfig)");
        assert!(lint.enabled);
        assert_eq!(
            lint.exe_path.as_deref(),
            Some(".claude/cli-finding-classifier.exe")
        );
        assert_eq!(lint.model.as_deref(), Some("mistral:7b"));
        assert_eq!(lint.endpoint.as_deref(), Some("http://localhost:11434"));
        assert_eq!(lint.timeout_secs, Some(90));
        assert_eq!(lint.max_diff_lines, Some(4000));
        assert_eq!(
            lint.output_path.as_deref(),
            Some(".takt/lint-screen-report.md")
        );
    }

    #[test]
    fn config_parses_with_lint_screen_section_minimal_only_enabled() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[lint_screen]
enabled = false

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        let lint = config
            .lint_screen
            .expect("section present even with only `enabled` should produce Some");
        assert!(!lint.enabled);
        assert!(lint.exe_path.is_none());
        assert!(lint.model.is_none());
        assert!(lint.endpoint.is_none());
        assert!(lint.timeout_secs.is_none());
        assert!(lint.max_diff_lines.is_none());
        assert!(lint.output_path.is_none());
    }

    #[test]
    fn config_lint_screen_section_absent_yields_none() {
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
            config.lint_screen.is_none(),
            "absent [lint_screen] should yield None (default OFF lane)"
        );
    }

    const LINT_SCREEN_ONLY_ENABLED_TOML: &str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[lint_screen]
enabled = true

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;

    #[test]
    fn config_lint_screen_numeric_defaults_resolve_via_constants() {
        let config: Config = toml::from_str(LINT_SCREEN_ONLY_ENABLED_TOML).unwrap();
        let lint = config.lint_screen.unwrap();
        assert_eq!(
            lint.timeout_secs
                .unwrap_or(DEFAULT_LINT_SCREEN_TIMEOUT_SECS),
            DEFAULT_LINT_SCREEN_TIMEOUT_SECS,
        );
        assert_eq!(
            lint.max_diff_lines
                .unwrap_or(DEFAULT_LINT_SCREEN_MAX_DIFF_LINES),
            DEFAULT_LINT_SCREEN_MAX_DIFF_LINES,
        );
    }

    #[test]
    fn config_lint_screen_string_defaults_resolve_via_constants() {
        let config: Config = toml::from_str(LINT_SCREEN_ONLY_ENABLED_TOML).unwrap();
        let lint = config.lint_screen.unwrap();
        assert_eq!(
            lint.model.as_deref().unwrap_or(DEFAULT_LINT_SCREEN_MODEL),
            DEFAULT_LINT_SCREEN_MODEL,
        );
        assert_eq!(
            lint.endpoint
                .as_deref()
                .unwrap_or(DEFAULT_LINT_SCREEN_ENDPOINT),
            DEFAULT_LINT_SCREEN_ENDPOINT,
        );
        assert_eq!(
            lint.exe_path
                .as_deref()
                .unwrap_or(DEFAULT_LINT_SCREEN_EXE_PATH),
            DEFAULT_LINT_SCREEN_EXE_PATH,
        );
        assert_eq!(
            lint.output_path
                .as_deref()
                .unwrap_or(DEFAULT_LINT_SCREEN_OUTPUT_PATH),
            DEFAULT_LINT_SCREEN_OUTPUT_PATH,
        );
    }

    #[test]
    fn parse_lint_screen_env_unset_yields_respect_toml() {
        assert_eq!(
            parse_lint_screen_env(None),
            LintScreenEnvOverride::RespectToml
        );
    }

    #[test]
    fn parse_lint_screen_env_force_enable_variants() {
        for value in [
            "true", "TRUE", "1", "yes", "YES", "on", "On", " true ", "\tyes\n",
        ] {
            assert_eq!(
                parse_lint_screen_env(Some(value)),
                LintScreenEnvOverride::ForceEnable,
                "value '{}' should map to ForceEnable",
                value
            );
        }
    }

    #[test]
    fn parse_lint_screen_env_respect_toml_variants() {
        for value in ["false", "FALSE", "0", "no", "NO", "off", "", "   "] {
            assert_eq!(
                parse_lint_screen_env(Some(value)),
                LintScreenEnvOverride::RespectToml,
                "value '{}' should map to RespectToml",
                value
            );
        }
    }

    #[test]
    fn parse_lint_screen_env_invalid_value() {
        for value in ["maybe", "2", "enable", "disabled", "yes please"] {
            assert_eq!(
                parse_lint_screen_env(Some(value)),
                LintScreenEnvOverride::InvalidValue,
                "value '{}' should map to InvalidValue",
                value
            );
        }
    }

    fn make_config_without_lint_screen() -> Config {
        Config {
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![GroupConfig {
                    name: "t".into(),
                    pre: None,
                    commands: vec!["echo".into()],
                }],
            },
            diff: None,
            lint_screen: None,
            scratch_file_warning: None,
            pr_size_check: None,
            pre_push_review: None,
            docs_only_routing: None,
            post_takt_regate: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        }
    }

    fn make_config_with_lint_screen(enabled: bool) -> Config {
        let mut config = make_config_without_lint_screen();
        config.lint_screen = Some(LintScreenConfig {
            enabled,
            exe_path: None,
            model: None,
            endpoint: None,
            timeout_secs: None,
            max_diff_lines: None,
            output_path: None,
        });
        config
    }

    #[test]
    fn apply_env_override_force_enable_on_absent_section_creates_lint_screen_config() {
        let mut config = make_config_without_lint_screen();
        apply_lint_screen_env_override(&mut config, Some("true".to_string()));
        let lint = config.lint_screen.expect(
            "env=true should construct default LintScreenConfig when [lint_screen] section absent",
        );
        assert!(lint.enabled);
        assert!(lint.exe_path.is_none());
        assert!(lint.model.is_none());
    }

    #[test]
    fn apply_env_override_force_enable_overwrites_toml_false() {
        let mut config = make_config_with_lint_screen(false);
        apply_lint_screen_env_override(&mut config, Some("1".to_string()));
        assert!(config.lint_screen.unwrap().enabled);
    }

    #[test]
    fn apply_env_override_respect_toml_keeps_toml_enabled_true() {
        let mut config = make_config_with_lint_screen(true);
        apply_lint_screen_env_override(&mut config, Some("false".to_string()));
        assert!(
            config.lint_screen.unwrap().enabled,
            "env=false should respect TOML (TOML had enabled=true, must remain true)"
        );
    }

    #[test]
    fn apply_env_override_respect_toml_keeps_toml_enabled_false() {
        let mut config = make_config_with_lint_screen(false);
        apply_lint_screen_env_override(&mut config, Some("".to_string()));
        assert!(!config.lint_screen.unwrap().enabled);
    }

    #[test]
    fn apply_env_override_unset_keeps_toml_section_absent() {
        let mut config = make_config_without_lint_screen();
        apply_lint_screen_env_override(&mut config, None);
        assert!(
            config.lint_screen.is_none(),
            "env unset + [lint_screen] absent should remain None"
        );
    }

    #[test]
    fn apply_env_override_invalid_value_respects_toml() {
        let mut config = make_config_with_lint_screen(false);
        apply_lint_screen_env_override(&mut config, Some("maybe".to_string()));
        assert!(
            !config.lint_screen.unwrap().enabled,
            "invalid env value should treat as RespectToml (TOML enabled=false preserved)"
        );
    }
}
