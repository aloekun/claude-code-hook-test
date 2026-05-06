//! cli-finding-classifier.exe を subprocess invoke する runner (ADR-038、Phase 5)
//!
//! 設計方針:
//! - subprocess で疎結合 (cli-pr-monitor の依存ツリーに ureq / Ollama を持ち込まない)
//! - 失敗 (exe 不在 / spawn 失敗 / timeout / parse 失敗) は **空の Vec を返す**:
//!   classifier 自体が internal で fallback (human_review) するため、cli-pr-monitor 側は
//!   classifier が一切呼べなかった場合のみ「enrichment なし」として扱えば十分
//! - stdin 経由で findings JSON を渡し、stdout で classified findings JSON を受ける
//!
//! 関連: ADR-038 §「失敗時の振る舞い (ブロックしない設計)」

use lib_report_formatter::Finding;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::config::ClassifierConfig;
use crate::log::{log_info, truncate_safe};

/// classifier の出力 schema (cli-finding-classifier::ClassifiedFinding と一致)
///
/// 別 crate を build dep に引き入れず schema だけ複製する。乖離防止のため、
/// ADR-038 で schema 変更があった際は両方を同期する責務を持つ。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct ClassifiedFinding {
    #[serde(flatten)]
    pub(crate) finding: Finding,
    pub(crate) action: String,
    pub(crate) action_confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) normalized_issue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fallback_reason: Option<String>,
}

/// cli-finding-classifier.exe のパスを解決する。
///
/// 通常は cli-pr-monitor.exe と同 dir に置かれる (.claude/ 配下デプロイ前提)。
pub(crate) fn classifier_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("cli-finding-classifier.exe")
}

/// findings を classifier に流して enrich する。
///
/// 戻り値: 成功時は `Vec<ClassifiedFinding>` (findings.len() と同じ長さ)、失敗時は空 Vec。
/// caller は `is_empty()` で「classifier が動かなかった」を判定し、
/// 元の findings をそのまま使えばよい。
pub(crate) fn classify_findings(
    config: &ClassifierConfig,
    findings: &[Finding],
) -> Vec<ClassifiedFinding> {
    if findings.is_empty() {
        return Vec::new();
    }

    let exe = classifier_exe_path();
    if !exe.exists() {
        log_info(&format!(
            "classifier exe が見つかりません (skip): {}",
            exe.display()
        ));
        return Vec::new();
    }

    let input = match serde_json::to_string(findings) {
        Ok(s) => s,
        Err(e) => {
            log_info(&format!(
                "classifier 入力 findings の JSON 化に失敗 (skip): {}",
                e
            ));
            return Vec::new();
        }
    };

    spawn_and_collect(&exe, config, &input)
}

fn spawn_and_collect(
    exe: &Path,
    config: &ClassifierConfig,
    stdin_payload: &str,
) -> Vec<ClassifiedFinding> {
    let timeout = Duration::from_secs(config.timeout_secs.saturating_add(5));
    let cmd = build_command(exe, config);

    let child = match cmd_spawn(cmd) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let child_with_stdin = match feed_stdin(child, stdin_payload) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let output = match wait_with_timeout(child_with_stdin, timeout) {
        Some(o) => o,
        None => {
            log_info(&format!(
                "classifier timeout ({}s, +5s buffer 含む) — skip",
                config.timeout_secs
            ));
            return Vec::new();
        }
    };

    parse_classifier_output(&output)
}

fn build_command(exe: &Path, config: &ClassifierConfig) -> Command {
    let mut cmd = Command::new(exe);
    cmd.arg("--model")
        .arg(&config.model)
        .arg("--endpoint")
        .arg(&config.endpoint)
        .arg("--timeout-secs")
        .arg(config.timeout_secs.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn cmd_spawn(mut cmd: Command) -> Option<std::process::Child> {
    match cmd.spawn() {
        Ok(c) => Some(c),
        Err(e) => {
            log_info(&format!("classifier spawn 失敗 (skip): {}", e));
            None
        }
    }
}

fn feed_stdin(
    mut child: std::process::Child,
    stdin_payload: &str,
) -> Option<std::process::Child> {
    if let Some(stdin) = child.stdin.as_mut() {
        if let Err(e) = stdin.write_all(stdin_payload.as_bytes()) {
            log_info(&format!("classifier stdin 書き込み失敗 (skip): {}", e));
            let _ = child.kill();
            return None;
        }
    }
    drop(child.stdin.take());
    Some(child)
}

fn parse_classifier_output(output: &Output) -> Vec<ClassifiedFinding> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log_info(&format!(
            "classifier non-zero exit ({}): {}",
            output.status,
            truncate_safe(&stderr, 200)
        ));
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    match serde_json::from_str::<Vec<ClassifiedFinding>>(&stdout) {
        Ok(v) => v,
        Err(e) => {
            log_info(&format!(
                "classifier 出力 JSON parse 失敗 (skip): {} (head: {})",
                e,
                truncate_safe(&stdout, 200)
            ));
            Vec::new()
        }
    }
}

/// child process を timeout 付きで待機する。
/// timeout 到達時は kill して None を返す。
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Option<Output> {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child.wait_with_output().ok();
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: &str, file: &str) -> Finding {
        Finding {
            severity: severity.into(),
            file: file.into(),
            line: "1".into(),
            issue: "x".into(),
            suggestion: "y".into(),
            source: "CodeRabbit".into(),
        }
    }

    #[test]
    fn empty_findings_returns_empty_without_spawning() {
        let cfg = ClassifierConfig::default();
        let result = classify_findings(&cfg, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn classifier_exe_path_resolves_to_sibling_of_current_exe() {
        let p = classifier_exe_path();
        assert!(p.to_string_lossy().ends_with("cli-finding-classifier.exe"));
    }

    #[test]
    fn classified_finding_serde_roundtrip() {
        let cf = ClassifiedFinding {
            finding: finding("Critical", "src/main.rs"),
            action: "human_review".into(),
            action_confidence: 0.85,
            normalized_issue: Some("test".into()),
            fallback_reason: None,
        };
        let json = serde_json::to_string(&cf).unwrap();
        assert!(json.contains("\"severity\":\"Critical\""));
        assert!(json.contains("\"action\":\"human_review\""));
        assert!(!json.contains("fallback_reason"));

        let parsed: ClassifiedFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cf);
    }

    #[test]
    fn classified_finding_parses_real_classifier_output_shape() {
        let json = r#"[{
            "severity": "Critical",
            "file": "src/main.rs",
            "line": "641",
            "issue": "issue text",
            "suggestion": "suggestion text",
            "source": "CodeRabbit",
            "action": "human_review",
            "action_confidence": 1.0,
            "normalized_issue": "summary"
        }]"#;
        let parsed: Vec<ClassifiedFinding> = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].action, "human_review");
        assert_eq!(parsed[0].finding.severity, "Critical");
    }
}
