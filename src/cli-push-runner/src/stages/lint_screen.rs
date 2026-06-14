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
use lib_subprocess::wait_with_timeout_basic;

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

    let raw_diff = match read_diff(diff_path, config) {
        Ok(d) => d,
        Err(reason) => {
            log_stage(STAGE, &format!("skip: {}", reason));
            return;
        }
    };

    let output_path = config
        .output_path
        .as_deref()
        .unwrap_or(DEFAULT_LINT_SCREEN_OUTPUT_PATH);

    let diff = match filter_excluded_hunks(&raw_diff) {
        FilterResult::Kept(filtered) => filtered,
        FilterResult::AllExcluded => {
            log_stage(
                STAGE,
                "skip: docs-only diff (`.md`/`.markdown` のみ)、Bundle k 順位 123",
            );
            write_skip_report_logged(output_path);
            return;
        }
    };

    let diff = strip_diff_metadata_lines(&diff);

    invoke_and_write_report(config, output_path, &diff, started);
}

/// classifier 呼び出し + report 書き出しを 1 ステップにまとめた helper。
///
/// `run_lint_screen` を 50 行ガイドラインに収めるための機能分離。
fn invoke_and_write_report(
    config: &LintScreenConfig,
    output_path: &str,
    diff: &str,
    started: Instant,
) {
    let params = resolve_invoke_params(config);
    let output = match invoke_classifier(&params, diff) {
        Ok(o) => o,
        Err(reason) => {
            log_stage(STAGE, &format!("skip: classifier {}", reason));
            return;
        }
    };

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

    let stdout_handle = lib_subprocess::drain_pipe_capped(
        child.stdout.take().expect("stdout piped"),
        crate::runner::MAX_LINES,
    );
    let stderr_handle = lib_subprocess::drain_pipe_capped(
        child.stderr.take().expect("stderr piped"),
        crate::runner::MAX_LINES,
    );

    let exit = wait_with_timeout_basic(STAGE, &mut child, params.timeout_secs + 5)
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

/// lint_screen の対象外とする拡張子 (lowercase で比較)。
///
/// 由来 (Bundle k 順位 123): mistral:7b が docs-only diff や `.md` ファイルに対して
/// Rust の `unused-import` を hallucinate する FP が PR #148/#150/#151/#152/#153 で
/// 5 PR 連続観測された。diff 段階で `.md` / `.markdown` ハンクを drop することで
/// この failure mode を構造的に解消する (ADR-038 §Known failure mode 参照)。
const EXCLUDED_EXTENSIONS: &[&str] = &["md", "markdown"];

/// `filter_excluded_hunks` の戻り値。Markdown 100% の diff は invoke を完全に
/// skip して別 path (skip-report 書き出し + 短絡 return) に流す必要があるため、
/// 通常 case (`Kept`) と区別する enum を返す。
enum FilterResult {
    Kept(String),
    AllExcluded,
}

/// 入力 diff から `EXCLUDED_EXTENSIONS` 拡張子のハンクを除外する。
///
/// 戻り値:
/// - `FilterResult::Kept(text)`: 1 件以上の対象外ハンクが残った場合、その diff text
/// - `FilterResult::AllExcluded`: 全ハンクが対象外拡張子だった (= docs-only diff) 場合
///
/// 実装方針: `diff --git ` 行を file-diff の境界として 1 ハンク = 1 chunk に分割、
/// 各 chunk の `+++ b/<path>` (なければ `--- a/<path>`) から拡張子を取り出して判定。
/// 拡張子は ASCII lowercase 比較 (= 大文字 `.MD` / `.Markdown` も除外対象に含む)。
fn filter_excluded_hunks(raw_diff: &str) -> FilterResult {
    let chunks = split_into_file_diffs(raw_diff);
    if chunks.is_empty() {
        return FilterResult::Kept(raw_diff.to_string());
    }
    let kept: Vec<&str> = chunks
        .iter()
        .filter(|chunk| !chunk_has_excluded_extension(chunk))
        .copied()
        .collect();
    if kept.is_empty() {
        return FilterResult::AllExcluded;
    }
    FilterResult::Kept(kept.join(""))
}

/// diff text を `diff --git ` 行を境界に file-diff chunks に分割する。
///
/// 行頭の `diff --git ` のみを境界とみなす。chunk 末尾は次の境界直前 (改行込み)。
/// 入力が `diff --git ` で始まらない場合 (= unified diff fragment ではない可能性)、
/// 空 vec を返して caller が原文 fallthrough する。
fn split_into_file_diffs(raw_diff: &str) -> Vec<&str> {
    if !raw_diff.starts_with("diff --git ") {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    for (idx, _) in raw_diff.match_indices("\ndiff --git ") {
        let end = idx + 1;
        chunks.push(&raw_diff[chunk_start..end]);
        chunk_start = end;
    }
    chunks.push(&raw_diff[chunk_start..]);
    chunks
}

/// chunk 内の `+++ b/<path>` (new path) を優先して拡張子を抽出する。
/// new path が無い場合 (= delete 操作で `+++ /dev/null` のケース) のみ
/// `--- a/<path>` (old path) にフォールバック。`EXCLUDED_EXTENSIONS` に
/// 該当すれば true を返す。
///
/// 新パス優先の根拠 (CR #155 Major 指摘): unified diff の慣例では `--- a/<path>`
/// が `+++ b/<path>` より先に出現するため、単純な `find_map` で両者を OR にすると
/// 旧パスが優先されてしまう。これだと `*.rs → *.md` の rename で **新パス側が `.md`
/// にも関わらず旧 `.rs` 拡張子で判定**され、Markdown 除外が機能しない bug が生じる。
/// new path を chunk 全体から先に探し、無い場合のみ old path に落とす。
fn chunk_has_excluded_extension(chunk: &str) -> bool {
    let new_path = chunk.lines().find_map(|line| line.strip_prefix("+++ b/"));
    let old_path = chunk.lines().find_map(|line| line.strip_prefix("--- a/"));
    let path = new_path.or(old_path).unwrap_or("");
    if path.is_empty() {
        return false;
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    EXCLUDED_EXTENSIONS.contains(&ext.as_str())
}

/// git diff の metadata 行を strip して LLM 入力の signal/noise 比を改善する。
///
/// 由来 (Bundle l 順位 132): mistral:7b が `similarity index 100%` の `100%` を
/// magic-number として false positive 検出する事象が PR #155 (Bundle k-1) /
/// PR #156 (Phase E) で観測された。git diff metadata 行は file rename / move を含む
/// PR で必ず出現するため、LLM 入力前に決定論的に除去することで構造的 FP を解消する。
///
/// 除去対象 (lossless: 各行を空行に置き換えずに完全削除):
/// - `similarity index NN%` — rename / copy 時の similarity ratio (magic-number FP の主因)
/// - `dissimilarity index NN%` — 同上 (git 1.6.5+ 形式)
/// - `index <hex>..<hex>[ <mode>]` — blob hash + file mode (hex の連続が magic 化されやすい)
/// - `new file mode NNNNNN` / `deleted file mode NNNNNN` / `old mode NNNNNN` /
///   `new mode NNNNNN` — Unix mode の 6 桁数値も magic 化されやすい
/// - `rename from <path>` / `rename to <path>` — rename target は filter_excluded_hunks
///   が `+++ b/<path>` で既に判定済 (情報量ゼロ)
/// - `copy from <path>` / `copy to <path>` — 同上
///
/// 保持: `diff --git ` (ハンク境界、file 識別) / `--- a/` / `+++ b/` (path 識別) /
/// `@@ ... @@` (hunk header、line range 情報は LLM が file 位置を理解するのに必要) /
/// `+` / `-` / ` ` (content 行)。
fn strip_diff_metadata_lines(diff: &str) -> String {
    diff.lines()
        .filter(|line| !is_diff_metadata_line(line))
        .map(|line| {
            let mut s = String::with_capacity(line.len() + 1);
            s.push_str(line);
            s.push('\n');
            s
        })
        .collect()
}

/// `strip_diff_metadata_lines` の per-line 判定。除去対象なら true。
fn is_diff_metadata_line(line: &str) -> bool {
    line.starts_with("similarity index ")
        || line.starts_with("dissimilarity index ")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("old mode ")
        || line.starts_with("new mode ")
        || line.starts_with("rename from ")
        || line.starts_with("rename to ")
        || line.starts_with("copy from ")
        || line.starts_with("copy to ")
}

/// `write_skip_report` を呼び出し、失敗時はステージログに記録する。
///
/// `run_lint_screen` のネスト深度を抑えるための分離 (match arm 内に if let を
/// 重ねないよう、エラー処理を 1 関数に閉じ込める)。
fn write_skip_report_logged(output_path: &str) {
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

    fn rust_chunk(path: &str) -> String {
        format!(
            "diff --git a/{path} b/{path}\n\
             index abc..def 100644\n\
             --- a/{path}\n\
             +++ b/{path}\n\
             @@ -1,1 +1,1 @@\n\
             -old\n\
             +new\n",
            path = path
        )
    }

    fn md_chunk(path: &str) -> String {
        format!(
            "diff --git a/{path} b/{path}\n\
             index abc..def 100644\n\
             --- a/{path}\n\
             +++ b/{path}\n\
             @@ -1,1 +1,1 @@\n\
             -# heading\n\
             +# heading updated\n",
            path = path
        )
    }

    fn assert_kept(result: FilterResult) -> String {
        match result {
            FilterResult::Kept(text) => text,
            FilterResult::AllExcluded => panic!("expected Kept, got AllExcluded"),
        }
    }

    #[test]
    fn filter_excluded_hunks_keeps_rust_only_diff_unchanged() {
        let diff = rust_chunk("src/lib.rs");
        let result = assert_kept(filter_excluded_hunks(&diff));
        assert_eq!(result, diff);
    }

    #[test]
    fn filter_excluded_hunks_drops_md_hunk_from_mixed_diff() {
        let rust = rust_chunk("src/main.rs");
        let md = md_chunk("docs/README.md");
        let combined = format!("{}{}", rust, md);
        let kept = assert_kept(filter_excluded_hunks(&combined));
        assert!(kept.contains("src/main.rs"));
        assert!(!kept.contains("docs/README.md"));
    }

    #[test]
    fn filter_excluded_hunks_signals_all_excluded_for_pure_markdown_diff() {
        let diff = format!(
            "{}{}",
            md_chunk("docs/a.md"),
            md_chunk("docs/b.markdown")
        );
        match filter_excluded_hunks(&diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => panic!("expected AllExcluded for pure .md/.markdown diff"),
        }
    }

    #[test]
    fn filter_excluded_hunks_treats_markdown_extension_case_insensitively() {
        let diff = format!("{}{}", md_chunk("README.MD"), md_chunk("notes.Markdown"));
        match filter_excluded_hunks(&diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => panic!("uppercase .MD / mixed-case .Markdown must be excluded"),
        }
    }

    #[test]
    fn filter_excluded_hunks_keeps_path_with_md_in_middle_not_extension() {
        let diff = rust_chunk("src/something.mdxyz.rs");
        let kept = assert_kept(filter_excluded_hunks(&diff));
        assert_eq!(kept, diff);
    }

    #[test]
    fn filter_excluded_hunks_handles_non_diff_input_as_passthrough() {
        let raw = "not a unified diff\njust raw text";
        let kept = assert_kept(filter_excluded_hunks(raw));
        assert_eq!(kept, raw);
    }

    #[test]
    fn filter_excluded_hunks_keeps_dev_null_create_path() {
        let diff = "diff --git a/src/new.rs b/src/new.rs\n\
                    new file mode 100644\n\
                    index 0000000..1234567\n\
                    --- /dev/null\n\
                    +++ b/src/new.rs\n\
                    @@ -0,0 +1,1 @@\n\
                    +pub fn x() {}\n";
        let kept = assert_kept(filter_excluded_hunks(diff));
        assert!(kept.contains("src/new.rs"));
    }

    #[test]
    fn filter_excluded_hunks_prefers_b_path_on_rename_to_markdown() {
        let diff = "diff --git a/src/a.rs b/docs/a.md\n\
                    similarity index 100%\n\
                    rename from src/a.rs\n\
                    rename to docs/a.md\n\
                    --- a/src/a.rs\n\
                    +++ b/docs/a.md\n\
                    @@ -1,1 +1,1 @@\n\
                    -old\n\
                    +new\n";
        match filter_excluded_hunks(diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => panic!(
                "rename .rs -> .md must be excluded based on new path (CR #155 Major)"
            ),
        }
    }

    #[test]
    fn filter_excluded_hunks_keeps_rename_from_md_to_rust() {
        let diff = "diff --git a/docs/old.md b/src/new.rs\n\
                    similarity index 100%\n\
                    rename from docs/old.md\n\
                    rename to src/new.rs\n\
                    --- a/docs/old.md\n\
                    +++ b/src/new.rs\n\
                    @@ -1,1 +1,1 @@\n\
                    -old\n\
                    +new\n";
        let kept = assert_kept(filter_excluded_hunks(diff));
        assert!(
            kept.contains("src/new.rs"),
            "rename .md -> .rs must be kept based on new path (symmetric to rename-to-md test)"
        );
    }

    #[test]
    fn filter_excluded_hunks_excludes_dev_null_delete_of_md() {
        let diff = "diff --git a/docs/old.md b/docs/old.md\n\
                    deleted file mode 100644\n\
                    index 1234567..0000000\n\
                    --- a/docs/old.md\n\
                    +++ /dev/null\n\
                    @@ -1,1 +0,0 @@\n\
                    -# removed\n";
        match filter_excluded_hunks(diff) {
            FilterResult::AllExcluded => {}
            FilterResult::Kept(_) => panic!(
                "delete of .md file should be excluded (--- a/ path is .md, +++ is /dev/null)"
            ),
        }
    }

    #[test]
    fn filter_excluded_hunks_preserves_hunk_boundaries_for_three_file_mixed() {
        let diff = format!(
            "{}{}{}",
            rust_chunk("src/a.rs"),
            md_chunk("docs/b.md"),
            rust_chunk("src/c.rs"),
        );
        let kept = assert_kept(filter_excluded_hunks(&diff));
        assert!(kept.contains("src/a.rs"));
        assert!(!kept.contains("docs/b.md"));
        assert!(kept.contains("src/c.rs"));
        let lines: Vec<&str> = kept.lines().filter(|l| l.starts_with("diff --git ")).collect();
        assert_eq!(lines.len(), 2, "exactly 2 diff --git boundaries must remain");
    }

    #[test]
    fn strip_diff_metadata_drops_similarity_index_line() {
        let diff = "diff --git a/src/a.rs b/src/b.rs\n\
                    similarity index 100%\n\
                    rename from src/a.rs\n\
                    rename to src/b.rs\n\
                    --- a/src/a.rs\n\
                    +++ b/src/b.rs\n\
                    @@ -1,1 +1,1 @@\n\
                    -old\n\
                    +new\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(
            !stripped.contains("similarity index"),
            "similarity index line must be stripped, got: {}",
            stripped
        );
        assert!(!stripped.contains("100%"));
        assert!(!stripped.contains("rename from"));
        assert!(!stripped.contains("rename to"));
    }

    #[test]
    fn strip_diff_metadata_preserves_hunk_boundaries_and_content() {
        let diff = "diff --git a/src/x.rs b/src/x.rs\nindex abc1234..def5678 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,2 +1,2 @@\n-fn old() {}\n+fn new() {}\n // unchanged context line\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(stripped.contains("diff --git "));
        assert!(stripped.contains("--- a/src/x.rs"));
        assert!(stripped.contains("+++ b/src/x.rs"));
        assert!(stripped.contains("@@ -1,2 +1,2 @@"));
        assert!(stripped.contains("-fn old() {}"));
        assert!(stripped.contains("+fn new() {}"));
        assert!(stripped.contains(" // unchanged context line"));
        assert!(!stripped.contains("index abc1234"));
    }

    #[test]
    fn strip_diff_metadata_drops_file_mode_lines() {
        let diff = "diff --git a/script.sh b/script.sh\n\
                    old mode 100644\n\
                    new mode 100755\n\
                    --- a/script.sh\n\
                    +++ b/script.sh\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("old mode"));
        assert!(!stripped.contains("new mode"));
        assert!(!stripped.contains("100755"));
        assert!(stripped.contains("--- a/script.sh"));
    }

    #[test]
    fn strip_diff_metadata_drops_new_and_deleted_file_mode() {
        let diff = "diff --git a/created.rs b/created.rs\n\
                    new file mode 100644\n\
                    index 0000000..1234567\n\
                    --- /dev/null\n\
                    +++ b/created.rs\n\
                    diff --git a/removed.rs b/removed.rs\n\
                    deleted file mode 100644\n\
                    index 7654321..0000000\n\
                    --- a/removed.rs\n\
                    +++ /dev/null\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("new file mode"));
        assert!(!stripped.contains("deleted file mode"));
        assert!(!stripped.contains("100644"));
        assert!(stripped.contains("--- /dev/null"));
        assert!(stripped.contains("+++ /dev/null"));
    }

    #[test]
    fn strip_diff_metadata_drops_copy_lines() {
        let diff = "diff --git a/orig.rs b/copy.rs\n\
                    similarity index 95%\n\
                    copy from orig.rs\n\
                    copy to copy.rs\n\
                    --- a/orig.rs\n\
                    +++ b/copy.rs\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("copy from"));
        assert!(!stripped.contains("copy to"));
        assert!(!stripped.contains("similarity index"));
        assert!(stripped.contains("+++ b/copy.rs"));
    }

    #[test]
    fn strip_diff_metadata_drops_dissimilarity_index() {
        let diff = "dissimilarity index 30%\n+changed\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(!stripped.contains("dissimilarity index"));
        assert!(stripped.contains("+changed"));
    }

    #[test]
    fn strip_diff_metadata_keeps_content_lines_with_metadata_keywords_as_substring() {
        let diff = "+let index = 0;\n\
                    -println!(\"similarity index ratio\");\n\
                    + // index of array\n";
        let stripped = strip_diff_metadata_lines(diff);
        assert!(stripped.contains("+let index = 0;"));
        assert!(stripped.contains("-println!(\"similarity index ratio\");"));
        assert!(stripped.contains("+ // index of array"));
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
