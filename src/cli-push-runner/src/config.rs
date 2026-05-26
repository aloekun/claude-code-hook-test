use serde::Deserialize;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;
pub(crate) const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;
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

#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) quality_gate: QualityGateConfig,
    pub(crate) diff: Option<DiffConfig>,
    pub(crate) lint_screen: Option<LintScreenConfig>,
    pub(crate) takt: TaktConfig,
    pub(crate) push: PushConfig,
    pub(crate) scratch_file_warning: Option<ScratchFileWarningConfig>,
}

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

#[derive(Deserialize)]
pub(crate) struct QualityGateConfig {
    pub(crate) parallel: Option<bool>,
    pub(crate) step_timeout: Option<u64>,
    pub(crate) groups: Vec<GroupConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct GroupConfig {
    pub(crate) name: String,
    pub(crate) pre: Option<String>,
    pub(crate) commands: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct TaktConfig {
    pub(crate) workflow: String,
    pub(crate) task: String,
    pub(crate) extra_args: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(crate) struct DiffConfig {
    pub(crate) command: String,
    pub(crate) output_path: String,
}

#[derive(Deserialize)]
pub(crate) struct PushConfig {
    pub(crate) command: String,
    pub(crate) timeout: Option<u64>,
}

pub(crate) fn config_path() -> PathBuf {
    let filename = "push-runner-config.toml";

    // 1. カレントディレクトリを優先（pnpm scripts はリポジトリルートで実行される）
    let cwd_path = Path::new(filename).to_path_buf();
    if cwd_path.exists() {
        return cwd_path;
    }

    // 2. exe と同じディレクトリ（.claude/ 配置パターン）
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(filename)
}

pub(crate) fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("設定ファイルの読み込みに失敗: {} ({})", path.display(), e))?;
    let mut config: Config =
        toml::from_str(&content).map_err(|e| format!("設定ファイルのパースに失敗: {}", e))?;
    apply_lint_screen_env_override(&mut config, std::env::var(ENV_LINT_SCREEN_ENABLED).ok());
    validate_config(&config)?;
    Ok(config)
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
fn apply_lint_screen_env_override(config: &mut Config, raw: Option<String>) {
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

fn validate_config(config: &Config) -> Result<(), String> {
    if config.quality_gate.groups.is_empty() {
        return Err("設定ファイルエラー: quality_gate.groups が空です".into());
    }
    for group in &config.quality_gate.groups {
        if group.commands.is_empty() {
            return Err(format!(
                "設定ファイルエラー: group '{}' の commands が空です",
                group.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_full_without_diff() {
        let toml_str = r#"
[quality_gate]
parallel = true
step_timeout = 60

[[quality_gate.groups]]
name = "lint"
commands = ["pnpm lint"]

[[quality_gate.groups]]
name = "test"
pre = "pnpm install"
commands = ["pnpm test", "pnpm test:e2e"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"
extra_args = ["--pipeline", "--skip-git"]

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.quality_gate.parallel, Some(true));
        assert_eq!(config.quality_gate.step_timeout, Some(60));
        assert_eq!(config.quality_gate.groups.len(), 2);
        assert!(config.diff.is_none());

        assert_eq!(config.takt.workflow, "pre-push-review");
        assert_eq!(config.takt.task, "pre-push review");
        assert_eq!(config.takt.extra_args.as_ref().unwrap().len(), 2);

        assert_eq!(config.push.command, "jj git push");
        assert!(config.push.timeout.is_none());
    }

    #[test]
    fn config_push_timeout_explicit() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "jj git push"
timeout = 600
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.push.timeout, Some(600));
        assert_eq!(
            config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
            600,
        );
    }

    #[test]
    fn config_push_timeout_defaults() {
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
        assert!(config.push.timeout.is_none());
        assert_eq!(
            config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
            DEFAULT_PUSH_TIMEOUT_SECS,
        );
    }

    #[test]
    fn config_parses_with_diff() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        let diff = config.diff.unwrap();
        assert_eq!(diff.command, "jj diff -r @");
        assert_eq!(diff.output_path, ".takt/review-diff.txt");
    }

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
            lint.timeout_secs.unwrap_or(DEFAULT_LINT_SCREEN_TIMEOUT_SECS),
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
    fn config_quality_gate_defaults() {
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
        assert!(config.quality_gate.parallel.unwrap_or(true));
        assert_eq!(
            config
                .quality_gate
                .step_timeout
                .unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS,
        );
        assert!(config.takt.extra_args.is_none());
    }

    #[test]
    fn config_pre_field_optional() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "no-pre"
commands = ["echo test"]

[[quality_gate.groups]]
name = "with-pre"
pre = "echo install"
commands = ["echo test"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.quality_gate.groups[0].pre.is_none());
        assert!(config.quality_gate.groups[1].pre.is_some());
    }

    #[test]
    fn validate_rejects_empty_groups() {
        let config = Config {
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![],
            },
            diff: None,
            lint_screen: None,
            scratch_file_warning: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("groups が空"));
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
        for value in ["true", "TRUE", "1", "yes", "YES", "on", "On", " true ", "\tyes\n"] {
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

    #[test]
    fn validate_rejects_empty_commands() {
        let config = Config {
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![GroupConfig {
                    name: "empty".into(),
                    pre: None,
                    commands: vec![],
                }],
            },
            diff: None,
            lint_screen: None,
            scratch_file_warning: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'empty'"));
    }
}
