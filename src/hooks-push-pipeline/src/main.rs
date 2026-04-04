//! Push Pipeline ランナー (スタンドアロン exe)
//!
//! pnpm push から呼び出され、push 前のパイプラインを実行します。
//! hooks-config.toml の [push_pipeline] セクションから設定を読み込みます。
//!
//! 処理フロー:
//!   1. command 型ステップを順次実行（失敗時は即座に終了）
//!   2. ai 型ステップは placeholder メッセージを出力（将来実装）
//!   3. 全 command ステップ成功 → push_cmd を実行
//!
//! 終了コード:
//!   0 - パイプライン成功 & push 完了
//!   1 - パイプライン失敗（テスト失敗等）
//!   2 - 設定エラー

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// ─── 設定 ───

/// hooks-config.toml のトップレベル構造
#[derive(Deserialize, Default)]
struct Config {
    push_pipeline: Option<PushPipelineConfig>,
}

/// `[push_pipeline]` セクションの設定
#[derive(Deserialize, Default)]
struct PushPipelineConfig {
    step_timeout: Option<u64>,
    push_cmd: Option<String>,
    steps: Option<Vec<PipelineStepConfig>>,
}

/// パイプラインの個別ステップ定義
#[derive(Deserialize, Clone)]
struct PipelineStepConfig {
    name: String,
    #[serde(rename = "type")]
    step_type: String,
    cmd: Option<String>,
    prompt: Option<String>,
}

/// デフォルトのステップタイムアウト（秒）
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;

/// デフォルトの push コマンド
const DEFAULT_PUSH_CMD: &str = "jj git push";

// ─── ログ出力ヘルパー ───

/// ステップ単位のログ出力 (`[push-pipeline] [label] STATUS — message`)
fn log_step(name: &str, status: &str, message: &str) {
    if message.is_empty() {
        eprintln!("[push-pipeline] [{}] {}", name, status);
    } else {
        eprintln!("[push-pipeline] [{}] {} — {}", name, status, message);
    }
}

/// パイプライン全体のログ出力
fn log_info(message: &str) {
    eprintln!("[push-pipeline] {}", message);
}

// ─── パイプ排出 (hooks-stop-quality から移植) ───

/// サブプロセス出力の最大収集行数
const MAX_LINES: usize = 40;

/// サブプロセスの stdout/stderr を別スレッドで収集する（最大 MAX_LINES 行）
fn drain_pipe(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut collected = Vec::with_capacity(MAX_LINES);
        let mut buf = Vec::new();

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < MAX_LINES {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    }
                }
                Err(_) => break,
            }
        }
        collected.join("\n")
    })
}

// ─── コマンド実行 (hooks-stop-quality から移植) ───

/// シェルコマンドを実行し、タイムアウト付きで結果を返す
fn run_cmd(name: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain_pipe(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe(child.stderr.take().unwrap());

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
            Err(e) => return (false, format!("Failed to wait for {}: {}", name, e)),
        }
    };

    if timed_out {
        let stdout = stdout_handle.join().unwrap_or_default();
        let stderr = stderr_handle.join().unwrap_or_default();
        let combined = combine_output(&stdout, &stderr);
        let mut msg = format!("timed out after {}s", timeout_secs);
        if !combined.is_empty() {
            msg = format!("{}\n{}", msg, combined);
        }
        return (false, msg);
    }

    let success = child.wait().map(|s| s.success()).unwrap_or(false);

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = combine_output(&stdout, &stderr);

    (success, combined)
}

/// stdout と stderr を結合する
fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

// ─── 設定ファイル読み込み ───

/// exe と同じディレクトリにある hooks-config.toml のパスを返す
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// hooks-config.toml を読み込みパースする
fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("hooks-config.toml の読み込みに失敗: {} ({})", path.display(), e))?;
    toml::from_str(&content)
        .map_err(|e| format!("hooks-config.toml のパースに失敗: {}", e))
}

// ─── パイプライン実行 ───

/// パイプラインのメインループ。全ステップ実行後に push を行う
fn run_pipeline() -> i32 {
    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            log_info(&format!("設定エラー: {}", e));
            return 2;
        }
    };

    let pipeline = match config.push_pipeline {
        Some(p) => p,
        None => {
            log_info("設定エラー: [push_pipeline] セクションが hooks-config.toml に見つかりません");
            return 2;
        }
    };

    let steps = pipeline.steps.unwrap_or_default();
    let timeout = pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
    let push_cmd = pipeline
        .push_cmd
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_PUSH_CMD)
        .to_string();

    if steps.is_empty() {
        log_info("警告: パイプラインステップが定義されていません。push のみ実行します。");
    }

    log_info(&format!(
        "パイプライン開始 ({} ステップ)",
        steps.len()
    ));

    // ステップを順次実行
    for (i, step) in steps.iter().enumerate() {
        let label = format!("{}/{} {}", i + 1, steps.len(), step.name);

        match step.step_type.as_str() {
            "command" => {
                let trimmed_cmd = step.cmd.as_deref().map(str::trim).filter(|c| !c.is_empty());
                let cmd = match trimmed_cmd {
                    Some(c) => c,
                    None => {
                        log_step(&label, "ERROR", "cmd が未定義または空です");
                        return 1;
                    }
                };

                log_step(&label, "RUN", cmd);

                let (success, output) = run_cmd(&step.name, cmd, timeout);

                if success {
                    log_step(&label, "PASS", "");
                } else {
                    log_step(&label, "FAIL", "");
                    if !output.is_empty() {
                        eprintln!("{}", output);
                    }
                    log_info(&format!(
                        "パイプライン中断: {} が失敗しました。問題を修正して pnpm push を再実行してください。",
                        step.name
                    ));
                    return 1;
                }
            }
            "ai" => {
                let prompt = step.prompt.as_deref().unwrap_or("(未定義)");
                log_step(
                    &label,
                    "SKIP",
                    &format!(
                        "AI ステップ (prompt: {}) — 将来実装予定。現在はスキップします。",
                        prompt
                    ),
                );
            }
            unknown => {
                log_step(
                    &label,
                    "ERROR",
                    &format!("未知のステップタイプ: {}", unknown),
                );
                return 1;
            }
        }
    }

    // 全ステップ成功 → push 実行
    log_info(&format!("全ステップ成功。push を実行します: {}", push_cmd));

    let (success, output) = run_cmd("push", &push_cmd, timeout);

    if success {
        log_info("push 完了");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        0
    } else {
        log_info("push 失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        1
    }
}

fn main() {
    std::process::exit(run_pipeline());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_push_pipeline() {
        let toml_str = r#"
[push_pipeline]
step_timeout = 60
push_cmd = "jj git push"

[[push_pipeline.steps]]
name = "test"
type = "command"
cmd = "pnpm test"

[[push_pipeline.steps]]
name = "review"
type = "ai"
prompt = "review_changes"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.push_pipeline.unwrap();
        assert_eq!(pipeline.step_timeout.unwrap(), 60);
        assert_eq!(pipeline.push_cmd.unwrap(), "jj git push");

        let steps = pipeline.steps.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "test");
        assert_eq!(steps[0].step_type, "command");
        assert_eq!(steps[0].cmd.as_deref(), Some("pnpm test"));
        assert_eq!(steps[1].name, "review");
        assert_eq!(steps[1].step_type, "ai");
        assert_eq!(steps[1].prompt.as_deref(), Some("review_changes"));
    }

    #[test]
    fn config_defaults_when_empty() {
        let toml_str = r#"
[push_pipeline]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.push_pipeline.unwrap();
        assert_eq!(
            pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS
        );
        assert_eq!(
            pipeline.push_cmd.unwrap_or_else(|| DEFAULT_PUSH_CMD.to_string()),
            DEFAULT_PUSH_CMD
        );
        assert!(pipeline.steps.unwrap_or_default().is_empty());
    }

    #[test]
    fn config_missing_push_pipeline_section() {
        let toml_str = r#"
[stop_quality]
step_timeout = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.push_pipeline.is_none());
    }

    #[test]
    fn combine_output_both_present() {
        let result = combine_output("stdout line", "stderr line");
        assert_eq!(result, "stdout line\nstderr line");
    }

    #[test]
    fn combine_output_only_stdout() {
        let result = combine_output("stdout line", "");
        assert_eq!(result, "stdout line");
    }

    #[test]
    fn combine_output_only_stderr() {
        let result = combine_output("", "stderr line");
        assert_eq!(result, "stderr line");
    }

    #[test]
    fn combine_output_both_empty() {
        let result = combine_output("", "");
        assert_eq!(result, "");
    }

    #[test]
    fn step_type_command_requires_cmd() {
        let toml_str = r#"
[push_pipeline]

[[push_pipeline.steps]]
name = "test"
type = "command"
cmd = "pnpm test"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let steps = config.push_pipeline.unwrap().steps.unwrap();
        assert!(steps[0].cmd.is_some());
    }

    #[test]
    fn step_type_ai_has_prompt() {
        let toml_str = r#"
[push_pipeline]

[[push_pipeline.steps]]
name = "review"
type = "ai"
prompt = "review_changes"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let steps = config.push_pipeline.unwrap().steps.unwrap();
        assert_eq!(steps[0].step_type, "ai");
        assert_eq!(steps[0].prompt.as_deref(), Some("review_changes"));
    }
}
