//! lint_screen の classifier 出力 (JSON) を markdown report に整形して書き出す層。
//!
//! docs-only diff の skip-report も本 module が担当する。

use std::path::Path;

use super::STAGE;
use crate::log::log_stage;

const REPORT_PREAMBLE: &str = "# Lint Screen Report (mistral:7b, Phase b' agreement 75%)\n\n\
> **試験運用**: 本 report は ADR-038 Phase c lint screen facet による mistral:7b の AI 所見。\n\
> agreement 75% (conditional GO) のため誤指摘あり。reviewer が独立判断する前提で参考情報として扱う。\n\n";

pub(super) fn write_report(
    output_path: &str,
    classifier_json: &str,
    stderr: &str,
) -> Result<(), String> {
    let path = Path::new(output_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("ディレクトリ作成失敗: {}", e))?;
    }
    let markdown = format_report(classifier_json, stderr);
    std::fs::write(path, markdown).map_err(|e| format!("write: {}", e))
}

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
    out.push_str(
        "classifier exe からの stderr 出力 (Phase A 順位 98 の num_ctx overflow 診断 log 等):\n\n",
    );
    out.push_str("```text\n");
    out.push_str(trimmed);
    out.push_str("\n```\n");
    out
}

/// markdown table cell 用に `|` と改行を escape する。
fn sanitize_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// `write_skip_report` を呼び出し、失敗時はステージログに記録する。
///
/// `run_lint_screen` のネスト深度を抑えるための分離 (match arm 内に if let を
/// 重ねないよう、エラー処理を 1 関数に閉じ込める)。
pub(super) fn write_skip_report_logged(output_path: &str) {
    if let Err(e) = write_skip_report(output_path) {
        log_stage(STAGE, &format!("skip: skip-report 書き出し失敗: {}", e));
    }
}

/// 全ハンクが対象外拡張子だった場合に書き出す skip-report。invoke は完全に skip する。
fn write_skip_report(output_path: &str) -> Result<(), String> {
    let path = Path::new(output_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("ディレクトリ作成失敗: {}", e))?;
    }
    let body = format!(
        "{}## Summary\n\n- screen_decision: `skipped`\n- 理由: docs-only diff のため lint_screen はスキップしました \
         (`.md` / `.markdown` 拡張子のみで Rust hallucinate FP を構造的に防止、Bundle k 順位 123 / ADR-038)\n\n\
         ## Findings\n\n(なし)\n",
        REPORT_PREAMBLE
    );
    std::fs::write(path, body).map_err(|e| format!("write: {}", e))
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
    fn write_skip_report_errors_when_parent_is_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let blocking_file = dir.path().join("blocking-as-dir-name");
        std::fs::write(&blocking_file, "existing regular file").unwrap();

        let bad_path = blocking_file.join("nested-report.md");
        let result = write_skip_report(bad_path.to_str().unwrap());
        assert!(
            result.is_err(),
            "regular file 配下への create_dir_all は err になるべき: {:?}",
            result
        );
        assert!(
            !bad_path.exists(),
            "err path で report が書き込まれないことを確認"
        );
    }

    #[test]
    fn write_skip_report_logged_does_not_panic_on_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let blocking_file = dir.path().join("blocking-regular-file");
        std::fs::write(&blocking_file, "existing regular file").unwrap();

        let bad_path = blocking_file.join("nested-report.md");
        write_skip_report_logged(bad_path.to_str().unwrap());

        assert!(
            blocking_file.is_file(),
            "blocking file が regular file のまま保持されていること (副作用なし)"
        );
        assert!(
            !bad_path.exists(),
            "err path で report が書き込まれないこと (silent fallback 再発防止 = Bundle l 順位 131)"
        );
    }

    #[test]
    fn write_skip_report_logged_succeeds_on_writable_path() {
        let dir = tempfile::tempdir().unwrap();
        let nested_path = dir.path().join("sub").join("report.md");
        write_skip_report_logged(nested_path.to_str().unwrap());
        assert!(
            nested_path.exists(),
            "writable path では report が生成され、log path に流れていないこと"
        );
        let body = std::fs::read_to_string(&nested_path).unwrap();
        assert!(body.contains("skipped"));
    }

    #[test]
    fn write_skip_report_writes_explanatory_body() {
        let path = std::env::temp_dir().join(format!(
            "test-lint-screen-skip-report-{}.md",
            std::process::id()
        ));
        let path_str = path.to_str().unwrap();
        write_skip_report(path_str).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("skipped"));
        assert!(body.contains("docs-only diff"));
        assert!(body.contains("Bundle k 順位 123"));
        let _ = std::fs::remove_file(&path);
    }
}
