//! `--check-modified-files` batch mode (PR-W5 Stop hook gate、Option C-2)。
//!
//! Phase 1 (PR-W1〜W4) で 800 行以下に整えた clean state を恒久維持するための
//! 強制層。Stop hook `[stop_quality.steps]` の 1 step として起動され、PR 範囲
//! (base branch から working copy まで) で変更された `.rs` file の行数を検査し、
//! 800 行超が 1 件でもあれば exit 1 で session 終了を block する。
//!
//! PostToolUse の [`crate::file_length`] soft-nag (additionalContext のみ、block しない)
//! と異なり、本 batch mode は Stop を block する **強制 gate** である。
//!
//! # ADR-039 3 点セット (experimental feature 標準パターン)
//!
//! - **Config opt-in (default OFF)**: `[file_length_gate]` section の `enabled = true`
//!   のときのみ検査を実行。section 不在 / `enabled = false` は完全 skip
//!   ([`gate_enabled`] が `unwrap_or(false)`)。本 gate は Stop を block するため
//!   § 1.b mechanical lint 例外 (default ON) には該当せず、§ 1 opt-in を適用する。
//! - **Kill-switch**: 緊急バイパスは env `FILE_LENGTH_CHECK_OVERRIDE` (truthy 値で skip、
//!   順位 151 `pr_size_check` と同 pattern)。恒久停止は `enabled = false`。
//! - **Bounded lifetime**: 採否判定基準は `docs/file-length-enforcement-plan.md`
//!   削除条件 3 (override 未使用で 1-2 セッション通過) を trigger とする。
//!
//! # Fail-closed (ADR-043)
//!
//! jj による変更検出が失敗 (jj 起動失敗 / revset 解決失敗) した場合、判定不能として
//! block 側にデフォルトする (exit 1)。ADR-043 § 原則 1 は hooks-stop-quality を
//! fail-closed 適用対象として明示している。`stop_hook_active` retry-skip
//! (ADR-004) が永続 lock を防ぐため、fail-closed でも session が詰まることはない。

use crate::file_length::{count_source_lines, MAX_FILE_LINES};
use crate::line_filter::is_rust_file;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 緊急バイパス用 env var (kill-switch)。truthy 値で検査を skip する。
const OVERRIDE_ENV_VAR: &str = "FILE_LENGTH_CHECK_OVERRIDE";

/// PR 範囲を求める diff の default base branch。`base` config 未指定時に使用する。
/// ADR-021 § Revset Composability: revset range は引数化し、alternative branch
/// (`main` 等) でも silent breakage しないようにする。
const DEFAULT_BASE_BRANCH: &str = "master";

/// `hooks-config.toml` のうち本 gate が参照する section のみ部分デシリアライズ。
#[derive(Deserialize, Default)]
struct GateConfigFile {
    file_length_gate: Option<FileLengthGateConfig>,
}

/// `[file_length_gate]` section。ADR-039 § 1 opt-in: `enabled` default OFF。
#[derive(Deserialize, Default)]
struct FileLengthGateConfig {
    enabled: Option<bool>,
    base: Option<String>,
}

/// `--check-modified-files` mode の entry point。process exit code を返す。
///
/// 0 = 通過 (disabled / override / 違反なし)、1 = block (違反あり / jj 失敗)。
pub(crate) fn run_check_modified_files() -> i32 {
    let config = load_gate_config();
    if !gate_enabled(&config) {
        return 0;
    }
    if let Some(raw) = override_value() {
        println!(
            "[file-length-gate] {}={} を検出、検査を skip します (意図的バイパス)",
            OVERRIDE_ENV_VAR, raw
        );
        return 0;
    }
    let base = effective_base(&config);
    let violations = match find_violations(&base) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "[file-length-gate] {} (fail-closed / ADR-043)\n\
                 緊急時は {}=1 で bypass してください。",
                e, OVERRIDE_ENV_VAR
            );
            return 1;
        }
    };
    if violations.is_empty() {
        return 0;
    }
    print!("{}", format_violation_report(&base, &violations));
    1
}

/// `enabled = Some(true)` のときのみ true。section 不在 / `None` / `Some(false)` は
/// すべて false (ADR-039 § 1 default OFF)。
fn gate_enabled(config: &GateConfigFile) -> bool {
    config
        .file_length_gate
        .as_ref()
        .and_then(|g| g.enabled)
        .unwrap_or(false)
}

/// `base` config を解決する。未指定 / 空文字なら [`DEFAULT_BASE_BRANCH`]。
fn effective_base(config: &GateConfigFile) -> String {
    config
        .file_length_gate
        .as_ref()
        .and_then(|g| g.base.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BASE_BRANCH)
        .to_string()
}

/// `FILE_LENGTH_CHECK_OVERRIDE` が truthy であればその生値を返す。
///
/// ADR-039 § 2: kill-switch 診断メッセージは実受理値を反映する。生値を返し呼び出し側で
/// `"{}={} を検出"` と表示することで、`1` / `true` / `on` 等どの値で bypass したかを
/// user が確認できる。
fn override_value() -> Option<String> {
    let raw = std::env::var(OVERRIDE_ENV_VAR).ok()?;
    is_truthy(&raw).then_some(raw)
}

/// override env の受理値判定 (順位 151 `pr_size_check::parse_override_env` と同 pattern)。
fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// exe と同じ directory の `hooks-config.toml` を読み込む (hooks-stop-quality と同方式)。
/// 読み込み / parse 失敗時は default (= gate disabled) を返す。
fn load_gate_config() -> GateConfigFile {
    let Ok(content) = std::fs::read_to_string(config_path()) else {
        return GateConfigFile::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// `hooks-config.toml` のパス解決。current_exe の親 directory を優先し、
/// 取得不能時は cwd 相対にフォールバック。
fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("hooks-config.toml")))
        .unwrap_or_else(|| PathBuf::from("hooks-config.toml"))
}

/// `jj diff -r '<base>..@' --name-only` で PR 範囲の変更 file を取得し `.rs` のみ返す。
///
/// working copy (@) の変更も含む。jj 起動失敗 / 非 0 exit は `Err` (fail-closed 側で処理)。
fn list_changed_rust_files(base: &str) -> Result<Vec<String>, String> {
    let revset = format!("{}..@", base);
    let output = Command::new("jj")
        .args(["diff", "-r", &revset, "--name-only"])
        .output()
        .map_err(|e| format!("jj 起動失敗: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(parse_changed_rust_files(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// `jj diff --name-only` の stdout から `.rs` file path のみ抽出する (pure)。
fn parse_changed_rust_files(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && is_rust_file(line))
        .map(str::to_string)
        .collect()
}

/// 変更 file を検出しサイズ違反を収集する。jj 失敗 / file 読み取り失敗は `Err` (fail-closed)。
fn find_violations(base: &str) -> Result<Vec<(String, usize)>, String> {
    let files = list_changed_rust_files(base)?;
    collect_oversize_files(&files)
}

/// 各 file の行数を数え、`MAX_FILE_LINES` 超を `(path, line_count)` で列挙する。
///
/// 削除された file は検査対象外 (skip): `jj diff --name-only` は削除 file も列挙するが、
/// file split refactor で元 file が消えるのは正常であり、存在しない path を block すると
/// 本 plan が促進する分割作業自体を誤 block してしまう。存在するのに読み取り不能な file
/// のみ fail-closed で `Err` を返す (ADR-043 § 原則1 / CodeRabbit #234-1 は「読み取り不能な
/// *既存* `.rs`」を対象と明記)。
fn collect_oversize_files(files: &[String]) -> Result<Vec<(String, usize)>, String> {
    files
        .iter()
        .filter(|path| Path::new(path).exists())
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .map_err(|e| format!("既存 .rs の読み取り失敗 {}: {}", path, e))?;
            let lines = count_source_lines(&source);
            Ok((lines > MAX_FILE_LINES).then_some((path.clone(), lines)))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|v| v.into_iter().flatten().collect())
}

/// block 時に stdout へ出力する診断メッセージを組み立てる (pure)。
///
/// Stop hook (`run_cmd_shell_capped`) は stdout+stderr を捕捉して block reason に埋め込む
/// ため、対処法と override hint を含める。
fn format_violation_report(base: &str, violations: &[(String, usize)]) -> String {
    let mut out = format!(
        "[file-length-gate] PR 範囲 ({}..@) に {} 行超の Rust file が {} 件あります (順位 147 / PR-W5 Stop gate):\n",
        base,
        MAX_FILE_LINES,
        violations.len()
    );
    for (path, lines) in violations {
        out.push_str(&format!(
            "  - {} ({} 行 > {} 行)\n",
            path, lines, MAX_FILE_LINES
        ));
    }
    out.push_str(&format!(
        "対処: file を責務ごとに module 分割してください。\
         mechanical refactor 等で一時的に超過するのが意図的なら {}=1 で bypass 可能です。\n",
        OVERRIDE_ENV_VAR
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn gate_disabled_when_section_absent() {
        let config: GateConfigFile = toml::from_str("").unwrap();
        assert!(!gate_enabled(&config));
    }

    #[test]
    fn gate_disabled_when_enabled_false() {
        let config: GateConfigFile =
            toml::from_str("[file_length_gate]\nenabled = false\n").unwrap();
        assert!(!gate_enabled(&config));
    }

    #[test]
    fn gate_disabled_when_enabled_omitted() {
        let config: GateConfigFile = toml::from_str("[file_length_gate]\n").unwrap();
        assert!(!gate_enabled(&config));
    }

    #[test]
    fn gate_enabled_only_when_enabled_true() {
        let config: GateConfigFile =
            toml::from_str("[file_length_gate]\nenabled = true\n").unwrap();
        assert!(gate_enabled(&config));
    }

    #[test]
    fn effective_base_defaults_to_master() {
        let config: GateConfigFile =
            toml::from_str("[file_length_gate]\nenabled = true\n").unwrap();
        assert_eq!(effective_base(&config), "master");
    }

    #[test]
    fn effective_base_honors_configured_branch() {
        let config: GateConfigFile =
            toml::from_str("[file_length_gate]\nenabled = true\nbase = \"main\"\n").unwrap();
        assert_eq!(effective_base(&config), "main");
    }

    #[test]
    fn effective_base_falls_back_when_blank() {
        let config: GateConfigFile =
            toml::from_str("[file_length_gate]\nenabled = true\nbase = \"  \"\n").unwrap();
        assert_eq!(effective_base(&config), "master");
    }

    #[test]
    fn is_truthy_accepts_documented_values() {
        for v in ["1", "true", "TRUE", "True", "yes", "on", "  on  "] {
            assert!(is_truthy(v), "{:?} should be truthy", v);
        }
    }

    #[test]
    fn is_truthy_rejects_falsey_values() {
        for v in ["0", "false", "no", "off", "", "   ", "2", "enable"] {
            assert!(!is_truthy(v), "{:?} should be falsey", v);
        }
    }

    #[test]
    fn parse_changed_rust_files_filters_non_rust_and_blanks() {
        let stdout = "src/a.rs\ndocs/readme.md\n\nsrc/nested/b.rs\nCargo.toml\n";
        assert_eq!(
            parse_changed_rust_files(stdout),
            vec!["src/a.rs".to_string(), "src/nested/b.rs".to_string()]
        );
    }

    #[test]
    fn parse_changed_rust_files_trims_surrounding_whitespace() {
        assert_eq!(
            parse_changed_rust_files("  src/a.rs  \n"),
            vec!["src/a.rs".to_string()]
        );
    }

    #[test]
    fn parse_changed_rust_files_empty_when_no_rust() {
        assert!(parse_changed_rust_files("README.md\npackage.json\n").is_empty());
    }

    fn write_rs_file(dir: &std::path::Path, name: &str, line_count: usize) -> String {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..line_count {
            writeln!(f, "let _x{} = {};", i, i).unwrap();
        }
        path.to_string_lossy().to_string()
    }

    #[test]
    fn collect_oversize_files_flags_over_threshold_only() {
        let dir = tempfile::tempdir().unwrap();
        let big = write_rs_file(dir.path(), "big.rs", MAX_FILE_LINES + 5);
        let small = write_rs_file(dir.path(), "small.rs", 10);
        let violations = collect_oversize_files(&[big.clone(), small]).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, big);
        assert_eq!(violations[0].1, MAX_FILE_LINES + 5);
    }

    #[test]
    fn collect_oversize_files_at_threshold_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let exact = write_rs_file(dir.path(), "exact.rs", MAX_FILE_LINES);
        assert!(collect_oversize_files(&[exact]).unwrap().is_empty());
    }

    #[test]
    fn collect_oversize_files_skips_deleted_file() {
        let dir = tempfile::tempdir().unwrap();
        let deleted = dir.path().join("gone.rs").to_string_lossy().to_string();
        assert!(
            collect_oversize_files(&[deleted]).unwrap().is_empty(),
            "削除 file (非存在) は検査対象外 = skip (file split refactor を誤 block しない)"
        );
    }

    #[test]
    fn collect_oversize_files_errors_on_present_but_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let unreadable = dir.path().to_string_lossy().to_string();
        assert!(
            collect_oversize_files(&[unreadable]).is_err(),
            "存在するのに読み取り不能 (directory 等) は fail-closed で Err (ADR-043 / CodeRabbit #234-1)"
        );
    }

    #[test]
    fn format_violation_report_lists_files_and_override_hint() {
        let violations = vec![
            ("src/big.rs".to_string(), 950),
            ("src/huge.rs".to_string(), 1200),
        ];
        let report = format_violation_report("master", &violations);
        assert!(report.contains("src/big.rs"));
        assert!(report.contains("950"));
        assert!(report.contains("src/huge.rs"));
        assert!(report.contains("1200"));
        assert!(report.contains(OVERRIDE_ENV_VAR));
        assert!(report.contains("2 件"));
    }
}
