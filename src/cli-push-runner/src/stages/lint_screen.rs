//! Phase c (§8.E lint screen facet) stage
//!
//! diff を mistral:7b に流して lint 一次フィルタの所見を markdown として出力する。
//! 設計詳細: `docs/adr/adr-038-local-llm-finding-classification.md`
//!
//! 動作モード:
//! - `enabled = false` (default): 完全 no-op、push pipeline は影響を受けない
//! - `enabled = true`: cli-finding-classifier.exe を subprocess で起動、stdin に diff を流し
//!   stdout の LintScreenResult JSON を markdown に整形して output_path に書き出す
//!
//! Phase b' で agreement 75% (conditional GO) のため、本 stage は **gating しない**。
//! Ollama down / timeout / diff 過大 / JSON parse 失敗 等のエラーは全て skip + warn で処理し、
//! push pipeline をブロックしない。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::config::{
    LintScreenConfig, DEFAULT_LINT_SCREEN_ENDPOINT, DEFAULT_LINT_SCREEN_EXE_PATH,
    DEFAULT_LINT_SCREEN_MAX_DIFF_LINES, DEFAULT_LINT_SCREEN_MODEL, DEFAULT_LINT_SCREEN_OUTPUT_PATH,
    DEFAULT_LINT_SCREEN_TIMEOUT_SECS,
};
use crate::log::log_stage;
use crate::runner::wait_with_timeout;

const STAGE: &str = "lint-screen";

struct InvokeParams<'a> {
    exe: &'a str,
    model: &'a str,
    endpoint: &'a str,
    timeout_secs: u64,
}

fn resolve_invoke_params(config: &LintScreenConfig) -> InvokeParams<'_> {
    InvokeParams {
        exe: config
            .exe_path
            .as_deref()
            .unwrap_or(DEFAULT_LINT_SCREEN_EXE_PATH),
        model: config.model.as_deref().unwrap_or(DEFAULT_LINT_SCREEN_MODEL),
        endpoint: config
            .endpoint
            .as_deref()
            .unwrap_or(DEFAULT_LINT_SCREEN_ENDPOINT),
        timeout_secs: config
            .timeout_secs
            .unwrap_or(DEFAULT_LINT_SCREEN_TIMEOUT_SECS),
    }
}

pub(crate) fn run_lint_screen(config: &LintScreenConfig, diff_path: &str) {
    if !config.enabled {
        return;
    }

    let started = Instant::now();
    log_stage(STAGE, "実行中 (試験運用、エラーは skip + warn)");

    let diff = match read_diff(diff_path, config) {
        Ok(d) => d,
        Err(reason) => {
            log_stage(STAGE, &format!("skip: {}", reason));
            return;
        }
    };

    let params = resolve_invoke_params(config);
    let output = match invoke_classifier(&params, &diff) {
        Ok(o) => o,
        Err(reason) => {
            log_stage(STAGE, &format!("skip: classifier {}", reason));
            return;
        }
    };

    let output_path = config
        .output_path
        .as_deref()
        .unwrap_or(DEFAULT_LINT_SCREEN_OUTPUT_PATH);
    match write_report(output_path, &output.stdout, &output.stderr) {
        Ok(()) => log_stage(
            STAGE,
            &format!(
                "出力: {} ({:.0}s)",
                output_path,
                started.elapsed().as_secs_f64()
            ),
        ),
        Err(e) => log_stage(STAGE, &format!("skip: report 書き出し失敗: {}", e)),
    }
}

fn read_diff(diff_path: &str, config: &LintScreenConfig) -> Result<String, String> {
    let raw = std::fs::read_to_string(diff_path)
        .map_err(|e| format!("diff 読み込み失敗 ({}): {}", diff_path, e))?;
    let max_lines = config
        .max_diff_lines
        .unwrap_or(DEFAULT_LINT_SCREEN_MAX_DIFF_LINES);
    let lines = raw.lines().count();
    if lines > max_lines {
        return Err(format!("diff 過大 ({} 行 > 上限 {})", lines, max_lines));
    }
    if raw.trim().is_empty() {
        return Err("diff が空".to_string());
    }
    Ok(raw)
}

struct ClassifierOutput {
    stdout: String,
    stderr: String,
}

fn invoke_classifier(params: &InvokeParams<'_>, diff: &str) -> Result<ClassifierOutput, String> {
    if !Path::new(params.exe).exists() {
        return Err(format!("exe 不在 ({})", params.exe));
    }

    let timeout_str = params.timeout_secs.to_string();
    let mut child = Command::new(params.exe)
        .args([
            "--mode",
            "lint-screen",
            "--model",
            params.model,
            "--endpoint",
            params.endpoint,
            "--timeout-secs",
            &timeout_str,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn 失敗: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(diff.as_bytes())
            .map_err(|e| format!("stdin 書き込み失敗: {}", e))?;
    }

    let stdout_handle = crate::runner::drain_pipe(child.stdout.take().expect("stdout piped"));
    let stderr_handle = crate::runner::drain_pipe(child.stderr.take().expect("stderr piped"));

    let exit = wait_with_timeout(STAGE, &mut child, params.timeout_secs + 5)
        .map_err(|e| format!("wait 失敗: {}", e))?;
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match exit {
        None => Err(format!("timeout ({}s)", params.timeout_secs + 5)),
        Some(status) if !status.success() => Err(format!("非 0 終了: {}", stderr)),
        Some(_) if stdout.trim().is_empty() => Err("stdout 空".to_string()),
        Some(_) => Ok(ClassifierOutput { stdout, stderr }),
    }
}

fn write_report(output_path: &str, classifier_json: &str, stderr: &str) -> Result<(), String> {
    let path = Path::new(output_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("ディレクトリ作成失敗: {}", e))?;
    }
    let markdown = format_report(classifier_json, stderr);
    std::fs::write(path, markdown).map_err(|e| format!("write: {}", e))
}

const REPORT_PREAMBLE: &str = "# Lint Screen Report (mistral:7b, Phase b' agreement 75%)\n\n\
> **試験運用**: 本 report は ADR-038 Phase c lint screen facet による mistral:7b の AI 所見。\n\
> agreement 75% (conditional GO) のため誤指摘あり。reviewer が独立判断する前提で参考情報として扱う。\n\n";

fn render_parse_error(err: &serde_json::Error, raw: &str) -> String {
    let mut out = String::from(REPORT_PREAMBLE);
    out.push_str(&format!("## JSON parse 失敗\n\nerror: {}\n\n", err));
    out.push_str("```\n");
    out.push_str(raw);
    out.push_str("\n```\n");
    out
}

fn render_summary(decision: &str, findings_count: usize, fallback_reason: &str) -> String {
    let mut s = format!(
        "## Summary\n\n- screen_decision: `{}`\n- findings: {}\n",
        decision, findings_count
    );
    if !fallback_reason.is_empty() {
        s.push_str(&format!("- fallback_reason: `{}`\n", fallback_reason));
    }
    s.push('\n');
    s
}

fn render_findings_table(findings: &[serde_json::Value]) -> String {
    if findings.is_empty() {
        return "## Findings\n\n(なし)\n".to_string();
    }
    let mut out =
        String::from("## Findings\n\n| severity | rule | file | line | issue | suggestion |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for f in findings {
        let s = f.get("severity").and_then(|v| v.as_str()).unwrap_or("?");
        let r = f.get("rule").and_then(|v| v.as_str()).unwrap_or("?");
        let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("?");
        let line = f
            .get("line")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string());
        let issue = f.get("issue").and_then(|v| v.as_str()).unwrap_or("");
        let sug = f.get("suggestion").and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            sanitize_cell(s),
            sanitize_cell(r),
            sanitize_cell(file),
            sanitize_cell(&line),
            sanitize_cell(issue),
            sanitize_cell(sug),
        ));
    }
    out
}

fn format_report(classifier_json: &str, stderr: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(classifier_json) {
        Ok(v) => v,
        Err(e) => {
            let mut out = render_parse_error(&e, classifier_json);
            out.push_str(&render_diagnostic(stderr));
            return out;
        }
    };
    let decision = value
        .get("screen_decision")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let fallback_reason = value
        .get("fallback_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let findings = value
        .get("lint_findings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut out = String::from(REPORT_PREAMBLE);
    out.push_str(&render_summary(decision, findings.len(), fallback_reason));
    out.push_str(&render_findings_table(&findings));
    out.push_str(&render_diagnostic(stderr));
    out
}

fn render_diagnostic(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n## Diagnostic\n\n");
    out.push_str("classifier exe からの stderr 出力 (Phase A 順位 98 の num_ctx overflow 診断 log 等):\n\n");
    out.push_str("```text\n");
    out.push_str(trimmed);
    out.push_str("\n```\n");
    out
}

/// markdown table cell 用に `|` と改行を escape する。
fn sanitize_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_report_renders_findings_table() {
        let json = r#"{
            "lint_findings": [
                {"severity":"minor","rule":"unused-import","file":"src/a.rs","line":1,"issue":"x","suggestion":"y"}
            ],
            "screen_decision":"auto_fix"
        }"#;
        let md = format_report(json, "");
        assert!(md.contains("auto_fix"));
        assert!(md.contains("unused-import"));
        assert!(md.contains("src/a.rs"));
        assert!(md.contains("| severity | rule | file | line | issue | suggestion |"));
    }

    #[test]
    fn format_report_handles_empty_findings() {
        let json = r#"{"lint_findings":[],"screen_decision":"informational"}"#;
        let md = format_report(json, "");
        assert!(md.contains("informational"));
        assert!(md.contains("(なし)"));
    }

    #[test]
    fn format_report_recovers_from_invalid_json() {
        let md = format_report("not json", "");
        assert!(md.contains("JSON parse 失敗"));
        assert!(md.contains("not json"));
    }

    #[test]
    fn format_report_includes_diagnostic_section_when_stderr_non_empty() {
        let json = r#"{
            "lint_findings": [],
            "screen_decision": "human_review",
            "fallback_reason": "ollama error: JSON parse error"
        }"#;
        let stderr = "[lib-ollama-client] WARN: Ollama JSON output may be truncated.\n  prompt_eval_count: 8192 (vs num_ctx: 8192)";
        let md = format_report(json, stderr);
        assert!(md.contains("## Diagnostic"));
        assert!(md.contains("prompt_eval_count: 8192"));
        assert!(md.contains("num_ctx: 8192"));
    }

    #[test]
    fn format_report_skips_diagnostic_section_when_stderr_empty() {
        let json = r#"{
            "lint_findings": [],
            "screen_decision": "informational"
        }"#;
        let md = format_report(json, "");
        assert!(!md.contains("## Diagnostic"));
        assert!(!md.contains("classifier exe からの stderr"));
    }

    #[test]
    fn format_report_skips_diagnostic_section_when_stderr_whitespace_only() {
        let json = r#"{
            "lint_findings": [],
            "screen_decision": "informational"
        }"#;
        let md = format_report(json, "   \n\n  ");
        assert!(!md.contains("## Diagnostic"));
    }

    #[test]
    fn format_report_appends_diagnostic_to_parse_error_path() {
        let stderr = "[lib-ollama-client] WARN: truncated";
        let md = format_report("not json", stderr);
        assert!(md.contains("JSON parse 失敗"));
        assert!(md.contains("## Diagnostic"));
        assert!(md.contains("[lib-ollama-client] WARN"));
    }

    #[test]
    fn format_report_includes_fallback_reason_when_present() {
        let json = r#"{
            "lint_findings":[],
            "screen_decision":"human_review",
            "fallback_reason":"ollama error: empty"
        }"#;
        let md = format_report(json, "");
        assert!(md.contains("fallback_reason"));
        assert!(md.contains("ollama error"));
    }

    #[test]
    fn sanitize_cell_escapes_pipe_and_newline() {
        assert_eq!(sanitize_cell("a|b"), "a\\|b");
        assert_eq!(sanitize_cell("a\nb"), "a b");
    }

    #[test]
    fn render_findings_table_sanitizes_pipe_in_all_columns() {
        let json = r#"{
            "lint_findings": [
                {"severity":"mi|nor","rule":"un|used","file":"src/a|b.rs","line":1,"issue":"x|y","suggestion":"y|z"}
            ],
            "screen_decision":"auto_fix"
        }"#;
        let md = format_report(json, "");
        assert!(!md.contains("mi|nor"), "severity must be sanitized");
        assert!(md.contains("mi\\|nor"));
        assert!(!md.contains("un|used"), "rule must be sanitized");
        assert!(md.contains("un\\|used"));
        assert!(!md.contains("a|b.rs"), "file must be sanitized");
        assert!(md.contains("a\\|b.rs"));
    }

    #[test]
    fn run_lint_screen_is_noop_when_disabled() {
        let cfg = LintScreenConfig {
            enabled: false,
            exe_path: None,
            model: None,
            endpoint: None,
            timeout_secs: None,
            max_diff_lines: None,
            output_path: None,
        };
        run_lint_screen(&cfg, "/nonexistent/diff/path");
    }

    #[test]
    fn read_diff_returns_error_on_missing_file() {
        let cfg = LintScreenConfig {
            enabled: true,
            exe_path: None,
            model: None,
            endpoint: None,
            timeout_secs: None,
            max_diff_lines: None,
            output_path: None,
        };
        let result = read_diff("/nonexistent/path.txt", &cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("diff 読み込み失敗"));
    }

    #[test]
    fn read_diff_returns_error_when_diff_exceeds_limit() {
        let path = std::env::temp_dir().join("test-lint-screen-large-diff.txt");
        let large = "x\n".repeat(100);
        std::fs::write(&path, large).unwrap();

        let cfg = LintScreenConfig {
            enabled: true,
            exe_path: None,
            model: None,
            endpoint: None,
            timeout_secs: None,
            max_diff_lines: Some(50),
            output_path: None,
        };
        let result = read_diff(path.to_str().unwrap(), &cfg);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("diff 過大"));
        assert!(err.contains("100"));
    }

    #[test]
    fn read_diff_returns_error_on_empty_diff() {
        let path = std::env::temp_dir().join("test-lint-screen-empty-diff.txt");
        std::fs::write(&path, "").unwrap();
        let cfg = LintScreenConfig {
            enabled: true,
            exe_path: None,
            model: None,
            endpoint: None,
            timeout_secs: None,
            max_diff_lines: None,
            output_path: None,
        };
        let result = read_diff(path.to_str().unwrap(), &cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("空"));
    }
}
