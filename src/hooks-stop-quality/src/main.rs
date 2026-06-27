//! Stop 品質ゲートフック (設定駆動型・統合版)
//!
//! Claude が応答を終了しようとする際に品質チェックを実行し、
//! 失敗があれば作業継続を強制します。
//!
//! .claude/hooks-config.toml の [stop_quality] セクションから
//! チェックステップとタイムアウトを読み込みます。
//!
//! 無限ループ防止:
//!   stop_hook_active が true の場合、品質ゲートをスキップして停止を許可します。
//!   これにより最大1回のリトライで収束します。
//!
//! takt subsession skip (ADR-004 § takt subsession skip):
//!   `.takt/runs/*/meta.json` で status: "running" の active takt run が存在する場合、
//!   品質ゲートを skip します。takt subsession は edit: false で起動される read-only
//!   分析セッションが多く (例: weekly-review whole-tree reviewer / post-merge-feedback
//!   analyzer)、Stop hook が「直せ」指示を出すと subsession が edit: false 制約に
//!   反して stray edit を試みる事故が発生する (PR #221 で実観測)。

use lib_subprocess::run_cmd_shell_capped;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

// --- 入力 ---

#[derive(Deserialize)]
struct HookInput {
    stop_hook_active: Option<bool>,
}

// --- 出力 ---

#[derive(Serialize)]
struct BlockDecision {
    decision: String,
    reason: String,
}

// --- 設定 ---

#[derive(Deserialize, Default)]
struct Config {
    stop_quality: Option<StopQualityConfig>,
}

#[derive(Deserialize, Default)]
struct StopQualityConfig {
    step_timeout: Option<u64>,
    steps: Option<Vec<QualityStepConfig>>,
}

#[derive(Deserialize, Clone)]
struct QualityStepConfig {
    name: String,
    cmd: String,
}

/// デフォルトのステップタイムアウト（秒）
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 60;

/// `.takt/runs/` の相対パス (repo root から)。hooks-session-start の reaper module と同値。
const TAKT_RUNS_DIR: &str = ".takt/runs";

/// takt meta.json の必要 field のみ部分デシリアライズ (status 判定のみ)。
#[derive(Deserialize)]
struct TaktMetaPartial {
    status: Option<String>,
}

/// `.takt/runs/<slug>/meta.json` を scan して active takt run があるか判定する。
///
/// 条件: いずれかの meta.json が `status: "running"` であれば true (= subsession active)。
/// 1 件以上見つかった時点で短絡 return する。malformed JSON / non-dir / read error は skip。
///
/// ADR-004 § takt subsession skip: takt subsession は `edit: false` で起動される
/// read-only 分析 session が多く、Stop hook が品質ゲート失敗の「直せ」指示を返すと
/// 制約に反して stray edit を試みる事故が発生する。本関数で active subsession を
/// 検知して品質ゲートを skip することで、ADR-004 の趣旨 (= 本対話セッションの品質担保)
/// と takt の `edit: false` 制約の整合を取る。
fn takt_subsession_active(repo_root: &Path) -> bool {
    let runs_dir = repo_root.join(TAKT_RUNS_DIR);
    let entries = match std::fs::read_dir(&runs_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if meta_status_is_running(&path.join("meta.json")) {
            return true;
        }
    }
    false
}

/// 単一の `meta.json` が `status: "running"` か判定する (test 用に切り出し)。
fn meta_status_is_running(meta_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(meta_path) else {
        return false;
    };
    let Ok(meta) = serde_json::from_str::<TaktMetaPartial>(&content) else {
        return false;
    };
    meta.status.as_deref() == Some("running")
}

/// block 判定を stdout に出力するヘルパー
fn emit_block(reason: &str) {
    let decision = BlockDecision {
        decision: "block".to_string(),
        reason: reason.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&decision) {
        println!("{}", json);
    }
}

/// 設定ファイルのパス解決
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む。(Config, ファイルが存在したか) を返す
fn load_config() -> (Config, bool) {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let config = toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "[stop-quality] Warning: Failed to parse {}: {}",
                    path.display(),
                    e
                );
                Config::default()
            });
            (config, true)
        }
        Err(_) => (Config::default(), false),
    }
}

/// パイプから最大 MAX_LINES 行を読み出すための上限値。超過分は読み捨てる。
const MAX_LINES: usize = 20;

fn main() {
    let (config, config_found) = load_config();

    let Some(input) = read_stdin_or_block() else {
        return;
    };
    let Some(hook_input) = parse_hook_input_or_block(&input) else {
        return;
    };

    if should_skip_quality_gate(&hook_input) {
        return;
    }

    let stop_config = config.stop_quality.unwrap_or_default();
    let steps = stop_config.steps.unwrap_or_default();
    let timeout = stop_config
        .step_timeout
        .unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);

    if steps.is_empty() {
        warn_no_steps_configured(config_found);
        return;
    }

    let failures = run_quality_steps(&steps, timeout);
    block_on_failures(&failures);
}

/// stdin を読み取る。失敗時は block 判定を emit して None を返す (fail-closed)。
fn read_stdin_or_block() -> Option<String> {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        emit_block(&format!(
            "品質ゲートエラー: stdin読み込みに失敗しました: {}",
            e
        ));
        return None;
    }
    Some(input)
}

/// HookInput を JSON 解析する。失敗時は block 判定を emit して None を返す (fail-closed)。
fn parse_hook_input_or_block(input: &str) -> Option<HookInput> {
    match serde_json::from_str(input) {
        Ok(v) => Some(v),
        Err(e) => {
            emit_block(&format!(
                "品質ゲートエラー: 入力JSONのパースに失敗しました: {}",
                e
            ));
            None
        }
    }
}

/// 品質ゲートを skip すべきか判定する。
///
/// 2 条件のいずれかで skip:
/// - `stop_hook_active = true`: 無限ループ防止 (最大 1 retry で収束、ADR-004)
/// - `takt_subsession_active = true`: ADR-004 § takt subsession skip (edit: false の
///   subsession に「直せ」指示を返さない)
fn should_skip_quality_gate(hook_input: &HookInput) -> bool {
    if hook_input.stop_hook_active.unwrap_or(false) {
        return true;
    }
    std::env::current_dir()
        .map(|cwd| takt_subsession_active(&cwd))
        .unwrap_or(false)
}

fn warn_no_steps_configured(config_found: bool) {
    if !config_found {
        eprintln!(
            "[stop-quality] Warning: hooks-config.toml not found. Quality gate is disabled."
        );
        eprintln!("[stop-quality] Place hooks-config.toml in the same directory as this exe.");
    } else {
        eprintln!(
            "[stop-quality] Warning: No quality steps configured. Quality gate is disabled."
        );
    }
}

fn run_quality_steps(steps: &[QualityStepConfig], timeout: u64) -> Vec<String> {
    let mut failures: Vec<String> = Vec::new();
    for step in steps {
        let (success, output) = run_cmd_shell_capped(&step.name, &step.cmd, timeout, MAX_LINES);
        if !success {
            failures.push(format!("**{}** failed:\n```\n{}\n```", step.name, output));
        }
    }
    failures
}

fn block_on_failures(failures: &[String]) {
    if failures.is_empty() {
        return;
    }
    let reason = format!(
        "品質ゲートが失敗しました。以下の問題を修正してください:\n\n{}",
        failures.join("\n\n")
    );
    emit_block(&reason);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_steps() {
        let config = Config::default();
        let stop = config.stop_quality.unwrap_or_default();
        let steps = stop.steps.unwrap_or_default();
        assert!(steps.is_empty());
    }

    #[test]
    fn config_parses_steps() {
        let toml_str = r#"
[stop_quality]
step_timeout = 120

[[stop_quality.steps]]
name = "lint"
cmd = "pnpm lint"

[[stop_quality.steps]]
name = "test"
cmd = "pnpm test"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let stop = config.stop_quality.unwrap();
        assert_eq!(stop.step_timeout.unwrap(), 120);
        let steps = stop.steps.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "lint");
        assert_eq!(steps[0].cmd, "pnpm lint");
        assert_eq!(steps[1].name, "test");
        assert_eq!(steps[1].cmd, "pnpm test");
    }

    #[test]
    fn config_default_timeout() {
        let config = Config::default();
        let stop = config.stop_quality.unwrap_or_default();
        let timeout = stop.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
        assert_eq!(timeout, 60);
    }

    #[test]
    fn stop_hook_active_true_allows_stop() {
        let input = r#"{"stop_hook_active": true}"#;
        let hook_input: HookInput = serde_json::from_str(input).unwrap();
        assert!(hook_input.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn stop_hook_active_false_runs_checks() {
        let input = r#"{"stop_hook_active": false}"#;
        let hook_input: HookInput = serde_json::from_str(input).unwrap();
        assert!(!hook_input.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn stop_hook_active_missing_runs_checks() {
        let input = r#"{}"#;
        let hook_input: HookInput = serde_json::from_str(input).unwrap();
        assert!(!hook_input.stop_hook_active.unwrap_or(false));
    }

    #[test]
    fn block_decision_serializes_correctly() {
        let decision = BlockDecision {
            decision: "block".to_string(),
            reason: "test failed".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        assert!(json.contains(r#""decision":"block""#));
        assert!(json.contains(r#""reason":"test failed""#));
    }

    #[test]
    fn step_timeout_default_is_reasonable() {
        const { assert!(DEFAULT_STEP_TIMEOUT_SECS >= 30) };
        const { assert!(DEFAULT_STEP_TIMEOUT_SECS <= 300) };
    }

    #[test]
    fn config_python_project() {
        let toml_str = r#"
[stop_quality]
step_timeout = 120

[[stop_quality.steps]]
name = "py-lint"
cmd = "pnpm py-lint"

[[stop_quality.steps]]
name = "py-test"
cmd = "pnpm py-test"

[[stop_quality.steps]]
name = "py-typecheck"
cmd = "pnpm py-typecheck"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let steps = config.stop_quality.unwrap().steps.unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].cmd, "pnpm py-lint");
    }

    use std::sync::atomic::{AtomicU32, Ordering};

    static UNIQUE_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let n = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("stop_quality_{}_{}_{}", prefix, pid, n));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_run_meta(root: &Path, slug: &str, status: &str) {
        let run_dir = root.join(".takt/runs").join(slug);
        std::fs::create_dir_all(&run_dir).unwrap();
        let json = serde_json::json!({ "status": status });
        std::fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn takt_subsession_active_returns_false_when_runs_dir_missing() {
        let root = unique_temp_root("no-runs-dir");
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_false_when_no_meta_json_files() {
        let root = unique_temp_root("empty-runs-dir");
        std::fs::create_dir_all(root.join(".takt/runs/orphan-slug")).unwrap();
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_false_when_all_status_completed() {
        let root = unique_temp_root("all-completed");
        write_run_meta(&root, "run-a", "completed");
        write_run_meta(&root, "run-b", "failed");
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_true_when_any_status_running() {
        let root = unique_temp_root("one-running");
        write_run_meta(&root, "completed-run", "completed");
        write_run_meta(&root, "active-run", "running");
        write_run_meta(&root, "failed-run", "failed");
        assert!(takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_returns_true_for_single_running_run() {
        let root = unique_temp_root("single-running");
        write_run_meta(&root, "active", "running");
        assert!(takt_subsession_active(&root));
    }

    #[test]
    fn takt_subsession_active_skips_malformed_meta_json() {
        let root = unique_temp_root("malformed");
        let run_dir = root.join(".takt/runs/malformed-run");
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::write(run_dir.join("meta.json"), "not-valid-json{").unwrap();
        assert!(!takt_subsession_active(&root));
    }

    #[test]
    fn meta_status_is_running_returns_true_for_running_status() {
        let root = unique_temp_root("status-running");
        write_run_meta(&root, "test", "running");
        let meta_path = root.join(".takt/runs/test/meta.json");
        assert!(meta_status_is_running(&meta_path));
    }

    #[test]
    fn meta_status_is_running_returns_false_for_other_statuses() {
        let root = unique_temp_root("status-other");
        for status in &["completed", "failed", "cancelled", "pending"] {
            write_run_meta(&root, status, status);
            let meta_path = root.join(format!(".takt/runs/{}/meta.json", status));
            assert!(
                !meta_status_is_running(&meta_path),
                "status {:?} must not be detected as running",
                status
            );
        }
    }

    #[test]
    fn meta_status_is_running_returns_false_when_file_missing() {
        let root = unique_temp_root("missing");
        let meta_path = root.join(".takt/runs/never-existed/meta.json");
        assert!(!meta_status_is_running(&meta_path));
    }
}
