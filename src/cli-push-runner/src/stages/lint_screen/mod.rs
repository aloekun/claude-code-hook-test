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
//!
//! module 構成:
//! - `diff_filter`: LLM 入力前の diff 前処理 (対象外拡張子除外 + metadata strip)
//! - `classifier`: cli-finding-classifier.exe の subprocess 起動
//! - `report`: classifier 出力 (JSON) の markdown 整形 + 書き出し

mod classifier;
mod diff_filter;
mod report;

use std::time::Instant;

use crate::config::{
    LintScreenConfig, DEFAULT_LINT_SCREEN_ENDPOINT, DEFAULT_LINT_SCREEN_EXE_PATH,
    DEFAULT_LINT_SCREEN_MAX_DIFF_LINES, DEFAULT_LINT_SCREEN_MODEL, DEFAULT_LINT_SCREEN_OUTPUT_PATH,
    DEFAULT_LINT_SCREEN_TIMEOUT_SECS,
};
use crate::log::log_stage;

use classifier::invoke_classifier;
use diff_filter::{filter_excluded_hunks, strip_diff_metadata_lines, FilterResult};
use report::{write_report, write_skip_report_logged};

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

#[cfg(test)]
mod tests {
    use super::*;

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
