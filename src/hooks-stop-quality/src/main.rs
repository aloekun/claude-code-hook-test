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

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

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
                eprintln!("[stop-quality] Warning: Failed to parse {}: {}", path.display(), e);
                Config::default()
            });
            (config, true)
        }
        Err(_) => (Config::default(), false),
    }
}

/// パイプから最大 MAX_LINES 行を読み出すヘルパー
/// 残りの行は読み捨てる（パイプバッファの排出を継続するため）
const MAX_LINES: usize = 20;

fn drain_pipe(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut collected = Vec::with_capacity(MAX_LINES);
        let mut buf = Vec::new();

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if collected.len() < MAX_LINES {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    }
                    // MAX_LINES 超過分も読み捨てて排出を継続
                }
                Err(_) => break,
            }
        }
        collected.join("\n")
    })
}

/// cmd /c 経由でコマンドを実行し、(成功, 出力) を返す
/// stdout/stderr を別スレッドで排出し、パイプデッドロックを防止する
/// タイムアウト超過時はプロセスを kill して失敗扱いにする
fn run_step(name: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    // パイプを別スレッドで排出（デッドロック防止）
    let stdout_handle = drain_pipe(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe(child.stderr.take().unwrap());

    // タイムアウト付きでプロセス終了を待つ
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return (false, format!("Failed to wait for {}: {}", cmd, e)),
        }
    };

    if timed_out {
        // スレッドから出力を回収してタイムアウトメッセージに含める
        let stdout = stdout_handle.join().unwrap_or_default();
        let stderr = stderr_handle.join().unwrap_or_default();
        let mut msg = format!("{} timed out after {}s", name, timeout_secs);
        if !stdout.is_empty() || !stderr.is_empty() {
            let combined = if stdout.is_empty() {
                stderr
            } else if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            msg = format!("{}\n{}", msg, combined);
        }
        return (false, msg);
    }

    // 終了ステータスを取得
    let success = child.wait().map(|s| s.success()).unwrap_or(false);

    // スレッドから出力を回収
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = if stdout.is_empty() {
        stderr
    } else if stderr.is_empty() {
        stdout
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    (success, combined)
}

fn main() {
    let (config, config_found) = load_config();

    // stdin を消費（fail-closed: エラー時は block）
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        emit_block(&format!("品質ゲートエラー: stdin読み込みに失敗しました: {}", e));
        return;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            emit_block(&format!("品質ゲートエラー: 入力JSONのパースに失敗しました: {}", e));
            return;
        }
    };

    // 無限ループ防止: stop_hook_active が true なら品質ゲートをスキップ
    if hook_input.stop_hook_active.unwrap_or(false) {
        return;
    }

    // 設定からステップとタイムアウトを取得
    let stop_config = config.stop_quality.unwrap_or_default();
    let steps = stop_config.steps.unwrap_or_default();
    let timeout = stop_config.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);

    // ステップが無い場合は警告を出して停止許可
    if steps.is_empty() {
        if !config_found {
            eprintln!("[stop-quality] Warning: hooks-config.toml not found. Quality gate is disabled.");
            eprintln!("[stop-quality] Place hooks-config.toml in the same directory as this exe.");
        } else {
            eprintln!("[stop-quality] Warning: No quality steps configured. Quality gate is disabled.");
        }
        return;
    }

    // 品質チェックを順番に実行
    let mut failures: Vec<String> = Vec::new();

    for step in &steps {
        let (success, output) = run_step(&step.name, &step.cmd, timeout);
        if !success {
            failures.push(format!("**{}** failed:\n```\n{}\n```", step.name, output));
        }
    }

    // 失敗があれば block を出力
    if !failures.is_empty() {
        let reason = format!(
            "品質ゲートが失敗しました。以下の問題を修正してください:\n\n{}",
            failures.join("\n\n")
        );
        emit_block(&reason);
    }
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
        assert!(DEFAULT_STEP_TIMEOUT_SECS >= 30);
        assert!(DEFAULT_STEP_TIMEOUT_SECS <= 300);
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
}
