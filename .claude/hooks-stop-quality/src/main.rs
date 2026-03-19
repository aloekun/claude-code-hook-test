//! Stop 品質ゲートフック
//!
//! Claude が応答を終了しようとする際に品質チェックを実行し、
//! 失敗があれば作業継続を強制します。
//!
//! 無限ループ防止:
//!   stop_hook_active が true の場合、品質ゲートをスキップして停止を許可します。
//!   これにより最大1回のリトライで収束します。

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
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

/// 品質チェックステップの定義
struct QualityStep {
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
}

/// 品質チェックステップ一覧
fn get_quality_steps() -> Vec<QualityStep> {
    vec![
        QualityStep {
            name: "lint",
            command: "pnpm",
            args: &["lint"],
        },
        QualityStep {
            name: "test",
            command: "pnpm",
            args: &["test"],
        },
        QualityStep {
            name: "test:e2e",
            command: "pnpm",
            args: &["test:e2e"],
        },
        QualityStep {
            name: "build",
            command: "pnpm",
            args: &["build"],
        },
    ]
}

/// ステップごとのタイムアウト（秒）
/// 4ステップ直列で最大 4×60=240s < hook timeout 300s（60s バッファ）
const STEP_TIMEOUT_SECS: u64 = 60;

/// cmd /c 経由でコマンドを実行し、(成功, 出力) を返す
/// タイムアウト超過時はプロセスを kill して失敗扱いにする
fn run_step(step: &QualityStep) -> (bool, String) {
    let mut cmd_args = vec!["/c", step.command];
    cmd_args.extend_from_slice(step.args);

    let mut child = match Command::new("cmd")
        .args(&cmd_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", step.command, e)),
    };

    let deadline = std::time::Instant::now() + Duration::from_secs(STEP_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    return (
                        false,
                        format!("{} timed out after {}s", step.name, STEP_TIMEOUT_SECS),
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return (false, format!("Failed to wait for {}: {}", step.command, e)),
        }
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return (false, format!("Failed to read output of {}: {}", step.command, e)),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.ends_with('\n') || stdout.is_empty() {
        format!("{}{}", stdout, stderr)
    } else {
        format!("{}\n{}", stdout, stderr)
    };
    // 先頭20行に制限
    let trimmed: String = combined.lines().take(20).collect::<Vec<_>>().join("\n");
    (output.status.success(), trimmed)
}

fn main() {
    // stdin を消費（fail-closed: エラー時は block）
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        emit_block(&format!("品質ゲートエラー: stdin読み込みに失敗しました: {}", e));
        return;
    }

    // JSON からフラグを取得（fail-closed: パース失敗時は block）
    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            emit_block(&format!("品質ゲートエラー: 入力JSONのパースに失敗しました: {}", e));
            return;
        }
    };

    // 無限ループ防止: stop_hook_active が true なら品質ゲートをスキップ
    if hook_input.stop_hook_active.unwrap_or(false) {
        return; // 何も出力しない → 停止許可
    }

    // 品質チェックを順番に実行
    let steps = get_quality_steps();
    let mut failures: Vec<String> = Vec::new();

    for step in &steps {
        let (success, output) = run_step(step);
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
    // 全て成功 → 何も出力しない → 停止許可
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_steps_count() {
        assert_eq!(get_quality_steps().len(), 4);
    }

    #[test]
    fn quality_steps_names() {
        let steps = get_quality_steps();
        let names: Vec<&str> = steps.iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["lint", "test", "test:e2e", "build"]);
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
    fn step_timeout_is_reasonable() {
        assert!(STEP_TIMEOUT_SECS >= 60);
        assert!(STEP_TIMEOUT_SECS <= 300);
    }
}
