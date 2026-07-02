//! auto-push 前の品質 gate (PR #224 gate-bypass 対策の B1 層)。
//!
//! takt fix 後の auto-push (`jj git push` 直 push) が cli-push-runner の
//! quality_gate を迂回し、`#[ignore]` 統合テストを壊す回帰が無検証で PR に
//! 到達した実害 (PR #224) への対策。push 前に push-runner-config.toml の
//! quality_gate group を実行し、FAIL なら push せず `action_required` に
//! 倒す (fail-closed、ADR-043)。
//!
//! - gate コマンド定義は push-runner-config.toml を単一ソースとして参照する
//!   (本 crate へ複製するとコメント契約だけの同値必須になり drift する)
//! - fix diff が docs-only (ADR-035 path 基準) の場合は gate を skip して
//!   push を続行する (docs auto-fix の速度維持)
//! - 判定不能 (diff 取得失敗 / config 読込失敗 / 分類不能) はすべて
//!   source 扱い / FAIL 方向に倒す

use serde::Deserialize;

use crate::log::log_info;
use crate::runner::{run_cmd_direct, JJ_CMD_TIMEOUT_SECS};

/// kill-switch: この環境変数が "1" のとき gate を skip する (緊急バイパス用)。
pub(crate) const GATE_DISABLE_ENV: &str = "PR_MONITOR_GATE_DISABLE";

const PUSH_RUNNER_CONFIG_PATH: &str = "push-runner-config.toml";

/// gate FAIL 理由に含める失敗出力の最大文字数 (末尾から保持)
const FAIL_OUTPUT_MAX_CHARS: usize = 500;

/// push-runner-config.toml に step_timeout が無い場合の既定値 (秒)
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 600;

/// auto-push 前の品質 gate 設定 (`pr-monitor-config.toml` の `[fix.gate]`)。
/// `crate::config::FixConfig` の field として deserialize される。
#[derive(Deserialize, Clone)]
pub(crate) struct GateConfig {
    /// 品質ゲートは fail-closed 原則 (ADR-043) により default 有効。
    /// 緊急バイパスは環境変数 `PR_MONITOR_GATE_DISABLE=1` (kill-switch)。
    #[serde(default = "default_gate_enabled")]
    pub(crate) enabled: bool,
    /// 実行する push-runner-config.toml の [[quality_gate.groups]] name。
    #[serde(default = "default_gate_group")]
    pub(crate) group: String,
}

fn default_gate_enabled() -> bool {
    true
}
fn default_gate_group() -> String {
    "rust-lint-test".into()
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            enabled: default_gate_enabled(),
            group: default_gate_group(),
        }
    }
}

/// gate の評価結果。
#[derive(Debug, PartialEq)]
pub(crate) enum GateOutcome {
    /// config / kill-switch で無効化されている (push 続行)
    SkippedDisabled,
    /// fix diff が docs-only のため gate 不要 (push 続行)
    SkippedDocsOnly,
    /// 全コマンド PASS (push 続行)
    Passed,
    /// FAIL: コマンド失敗 / config 読込不能 / 判定不能を含む (push 中止)
    Failed { reason: String },
}

/// gate を無効化すべきか。(config の enabled, kill-switch env 値) から判定する。
/// env 読取は呼び出し側で行い、本関数は注入値で判定する (DI over ambient global)。
pub(crate) fn gate_disabled(config_enabled: bool, env_value: Option<&str>) -> bool {
    if env_value == Some("1") {
        return true;
    }
    !config_enabled
}

/// `jj diff --summary` 出力が docs-only か判定する (ADR-035 path 基準)。
///
/// fail-closed: 空出力・パース不能な行 (rename 等の非 M/A/D 行)・除外パス・
/// 非 docs パスのいずれかがあれば false (= source 扱いで gate 実行)。
/// ADR-035 の diff 内容基準 (doc comment のみの .rs 変更等) は path だけでは
/// 判定できないため対象外 — その場合は gate が実行されるだけで安全側に倒れる。
pub(crate) fn is_docs_only_summary(summary: &str) -> bool {
    let mut saw_any = false;
    for line in summary.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        saw_any = true;
        let Some((status, path)) = line.split_once(' ') else {
            return false;
        };
        if !matches!(status, "M" | "A" | "D") {
            return false;
        }
        if !is_docs_only_path(path) {
            return false;
        }
    }
    saw_any
}

/// 単一パスが ADR-035 の docs-only path 基準を満たすか。
///
/// `.takt/` / `.claude/` は形式上 md/yaml でも code-equivalent (ADR-035 除外パス)。
/// Windows の `jj diff --summary` はバックスラッシュ区切りで出力するため正規化する。
fn is_docs_only_path(path: &str) -> bool {
    let p = path.trim().replace('\\', "/");
    if p.is_empty() {
        return false;
    }
    if p.starts_with(".takt/") || p.starts_with(".claude/") {
        return false;
    }
    p.starts_with("docs/") || p.ends_with(".md")
}

/// 末尾 max_chars 文字を返す (cargo test の失敗一覧は出力末尾に出るため tail を残す)。
/// multi-byte 安全のため char_indices で境界を求める。
fn tail_chars(s: &str, max_chars: usize) -> &str {
    let count = s.chars().count();
    if count <= max_chars {
        return s;
    }
    match s.char_indices().nth(count - max_chars) {
        Some((idx, _)) => &s[idx..],
        None => s,
    }
}

#[derive(Deserialize)]
struct PushRunnerConfig {
    quality_gate: Option<QualityGateSection>,
}

#[derive(Deserialize)]
struct QualityGateSection {
    #[serde(default = "default_step_timeout")]
    step_timeout: u64,
    #[serde(default)]
    groups: Vec<QualityGateGroup>,
}

#[derive(Deserialize)]
struct QualityGateGroup {
    name: String,
    #[serde(default)]
    commands: Vec<String>,
}

fn default_step_timeout() -> u64 {
    DEFAULT_STEP_TIMEOUT_SECS
}

/// push-runner-config.toml の内容文字列から、指定 group のコマンド列と
/// step_timeout を抽出する。見つからない / 空の場合は Err (fail-closed)。
pub(crate) fn parse_gate_commands(
    toml_text: &str,
    group_name: &str,
) -> Result<(Vec<String>, u64), String> {
    let config: PushRunnerConfig = toml::from_str(toml_text)
        .map_err(|e| format!("push-runner-config.toml パース失敗: {}", e))?;
    let Some(qg) = config.quality_gate else {
        return Err("push-runner-config.toml に [quality_gate] がありません".into());
    };
    let Some(group) = qg.groups.iter().find(|g| g.name == group_name) else {
        return Err(format!(
            "[[quality_gate.groups]] name=\"{}\" が見つかりません",
            group_name
        ));
    };
    if group.commands.is_empty() {
        return Err(format!(
            "quality_gate group \"{}\" の commands が空です",
            group_name
        ));
    }
    Ok((group.commands.clone(), qg.step_timeout))
}

/// gate コマンド列を順次実行する。1 つでも失敗したら即 Failed (fail fast)。
pub(crate) fn run_gate_commands(commands: &[String], step_timeout_secs: u64) -> GateOutcome {
    for command in commands {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let Some((program, args)) = parts.split_first() else {
            return GateOutcome::Failed {
                reason: "gate コマンドが空です".into(),
            };
        };
        log_info(&format!("[gate] 実行: {}", command));
        let started = std::time::Instant::now();
        let (ok, output) = run_cmd_direct(program, args, &[], step_timeout_secs);
        let elapsed = started.elapsed().as_secs();
        if !ok {
            log_info(&format!("[gate] FAIL ({}s): {}", elapsed, command));
            return GateOutcome::Failed {
                reason: format!(
                    "`{}` が失敗: {}",
                    command,
                    tail_chars(output.trim(), FAIL_OUTPUT_MAX_CHARS)
                ),
            };
        }
        log_info(&format!("[gate] PASS ({}s): {}", elapsed, command));
    }
    GateOutcome::Passed
}

/// fix diff (pre_cid → @) の summary を取得して docs-only 判定する。
/// pre_cid 不明・jj 失敗は false (source 扱い = gate 実行) に倒す。
fn fix_diff_is_docs_only(pre_cid: Option<&str>) -> bool {
    let Some(pre) = pre_cid else {
        return false;
    };
    let (ok, out) = run_cmd_direct(
        "jj",
        &["diff", "--from", pre, "--to", "@", "--summary"],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        log_info(&format!(
            "[gate] fix diff summary 取得失敗 (source 扱いで gate を実行): {}",
            out.trim()
        ));
        return false;
    }
    is_docs_only_summary(&out)
}

/// auto-push 直前の gate 評価 (副作用: env / config / jj diff / コマンド実行)。
///
/// 判定順:
/// 1. kill-switch env or config disabled → SkippedDisabled
/// 2. fix diff (pre_cid → @) が docs-only → SkippedDocsOnly
/// 3. push-runner-config.toml から gate コマンド取得 → 失敗は Failed (fail-closed)
/// 4. コマンド順次実行 → Passed / Failed
pub(crate) fn evaluate_gate(config: &GateConfig, pre_cid: Option<&str>) -> GateOutcome {
    let env_value = std::env::var(GATE_DISABLE_ENV).ok();
    if gate_disabled(config.enabled, env_value.as_deref()) {
        log_info("[gate] 無効化されている (config or kill-switch)、gate なしで push 続行");
        return GateOutcome::SkippedDisabled;
    }

    if fix_diff_is_docs_only(pre_cid) {
        log_info("[gate] fix diff は docs-only (ADR-035 path 基準)、gate を skip して push 続行");
        return GateOutcome::SkippedDocsOnly;
    }

    let toml_text = match std::fs::read_to_string(PUSH_RUNNER_CONFIG_PATH) {
        Ok(t) => t,
        Err(e) => {
            return GateOutcome::Failed {
                reason: format!("{} 読み込み失敗: {}", PUSH_RUNNER_CONFIG_PATH, e),
            }
        }
    };
    let (commands, step_timeout) = match parse_gate_commands(&toml_text, &config.group) {
        Ok(v) => v,
        Err(e) => return GateOutcome::Failed { reason: e },
    };
    run_gate_commands(&commands, step_timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_disabled_by_kill_switch_env() {
        assert!(gate_disabled(true, Some("1")));
        assert!(gate_disabled(false, Some("1")));
    }

    #[test]
    fn gate_enabled_when_config_on_and_env_absent_or_other() {
        assert!(!gate_disabled(true, None));
        assert!(!gate_disabled(true, Some("0")));
        assert!(!gate_disabled(true, Some("")));
    }

    #[test]
    fn gate_disabled_when_config_off() {
        assert!(gate_disabled(false, None));
    }

    #[test]
    fn docs_only_accepts_all_docs_paths() {
        assert!(is_docs_only_summary(
            "M docs/notes.md\nA docs/foo/bar.md\nD docs/old.md\n"
        ));
    }

    #[test]
    fn docs_only_accepts_root_md() {
        assert!(is_docs_only_summary("M README.md"));
    }

    #[test]
    fn docs_only_rejects_source_path() {
        assert!(!is_docs_only_summary("M src/cli-pr-monitor/src/main.rs"));
    }

    #[test]
    fn docs_only_rejects_mixed_docs_and_source() {
        assert!(!is_docs_only_summary("M docs/a.md\nM src/lib.rs"));
    }

    #[test]
    fn docs_only_rejects_excluded_code_equivalent_paths() {
        assert!(!is_docs_only_summary("M .takt/facets/instructions/fix.md"));
        assert!(!is_docs_only_summary("M .claude/hooks-config.toml"));
        assert!(!is_docs_only_summary("M .takt/workflows/post-pr-review.yaml"));
    }

    #[test]
    fn docs_only_rejects_empty_summary() {
        assert!(!is_docs_only_summary(""));
        assert!(!is_docs_only_summary("  \n"));
    }

    #[test]
    fn docs_only_rejects_unparseable_lines() {
        assert!(!is_docs_only_summary("R docs/a.md docs/b.md"));
        assert!(!is_docs_only_summary("docs/a.md"));
    }

    #[test]
    fn docs_only_normalizes_windows_backslash_paths() {
        assert!(is_docs_only_summary("M docs\\notes.md"));
        assert!(!is_docs_only_summary("M .takt\\facets\\instructions\\fix.md"));
    }

    const SAMPLE: &str = r#"
[quality_gate]
step_timeout = 300

[[quality_gate.groups]]
name = "lint"
commands = ["pnpm lint"]

[[quality_gate.groups]]
name = "rust-lint-test"
commands = [
  "cargo clippy --workspace -- -D warnings",
  "cargo test",
  "cargo test -- --ignored --test-threads=1",
]
"#;

    #[test]
    fn parse_gate_commands_extracts_group_and_timeout() {
        let (commands, timeout) = parse_gate_commands(SAMPLE, "rust-lint-test").unwrap();
        assert_eq!(commands.len(), 3);
        assert!(commands[2].contains("--ignored"));
        assert_eq!(timeout, 300);
    }

    #[test]
    fn parse_gate_commands_missing_group_is_err() {
        assert!(parse_gate_commands(SAMPLE, "no-such-group").is_err());
    }

    #[test]
    fn parse_gate_commands_missing_section_is_err() {
        assert!(parse_gate_commands("[monitor]\nenabled = true\n", "rust-lint-test").is_err());
    }

    #[test]
    fn parse_gate_commands_empty_commands_is_err() {
        let toml = "[quality_gate]\n[[quality_gate.groups]]\nname = \"g\"\ncommands = []\n";
        assert!(parse_gate_commands(toml, "g").is_err());
    }

    #[test]
    fn parse_gate_commands_parse_error_is_err() {
        assert!(parse_gate_commands("not toml [", "g").is_err());
    }

    #[test]
    fn parse_gate_commands_default_step_timeout() {
        let toml = "[quality_gate]\n[[quality_gate.groups]]\nname = \"g\"\ncommands = [\"x\"]\n";
        let (_, timeout) = parse_gate_commands(toml, "g").unwrap();
        assert_eq!(timeout, DEFAULT_STEP_TIMEOUT_SECS);
    }

    #[test]
    fn tail_chars_keeps_tail_of_long_output() {
        assert_eq!(tail_chars("abcdef", 3), "def");
        assert_eq!(tail_chars("abc", 5), "abc");
    }

    #[test]
    fn tail_chars_multibyte_safe() {
        assert_eq!(tail_chars("あいうえお", 2), "えお");
    }

    #[test]
    fn run_gate_commands_blank_command_fails_without_spawn() {
        let out = run_gate_commands(&["   ".to_string()], 5);
        assert!(matches!(out, GateOutcome::Failed { .. }));
    }

    #[test]
    #[ignore = "integration: spawns real processes; run via `cargo test -- --ignored --test-threads=1`"]
    fn run_gate_commands_pass_and_fail_with_real_processes() {
        let pass = run_gate_commands(&["jj --version".to_string()], 30);
        assert_eq!(pass, GateOutcome::Passed);

        let fail = run_gate_commands(&["cmd-that-does-not-exist-gate-b1 --x".to_string()], 30);
        assert!(matches!(fail, GateOutcome::Failed { .. }));
    }
}
