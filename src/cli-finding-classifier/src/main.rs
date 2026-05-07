//! cli-finding-classifier
//!
//! CodeRabbit findings (lib_report_formatter::Finding) を stdin から JSON で受け、
//! Ollama で classify した ClassifiedFinding を stdout に JSON で出力する CLI。
//!
//! 使い方:
//!   check-ci-coderabbit --list-findings --pr 42 \
//!     | jq '.findings | map({severity, file, line: (.line | tostring), issue: .summary, suggestion: "", source: "CodeRabbit"})' \
//!     | cli-finding-classifier --model mistral:7b
//!
//! 引数:
//!   --model <name>          Ollama モデル名 (default: mistral:7b)
//!   --endpoint <url>        Ollama endpoint (default: http://localhost:11434)
//!   --timeout-secs <sec>    リクエストタイムアウト (default: 30)
//!   --prompt-file <path>    プロンプトテンプレートのパス (default: 同梱の classify.txt)
//!
//! 終了コード:
//!   0 - 正常終了 (一部 finding が fallback でも 0)
//!   1 - 入力 JSON が壊れている / プロンプトファイルが読めない 等の致命エラー

use cli_finding_classifier::{classify_batch, screen_diff};
use lib_ollama_client::OllamaClient;
use lib_report_formatter::Finding;
use std::io::Read;
use std::time::Duration;

const DEFAULT_PROMPT: &str = include_str!("../prompts/classify.txt");
const LINT_SCREEN_PROMPT: &str = include_str!("../prompts/lint-screen.txt");

#[derive(Debug, PartialEq)]
enum Mode {
    Classify,
    LintScreen,
}

#[derive(Debug)]
struct CliArgs {
    mode: Mode,
    model: String,
    endpoint: String,
    timeout_secs: u64,
    prompt_file: Option<String>,
}

fn parse_mode(value: &str) -> Result<Mode, String> {
    match value {
        "classify" => Ok(Mode::Classify),
        "lint-screen" => Ok(Mode::LintScreen),
        other => Err(format!(
            "--mode requires 'classify' or 'lint-screen', got '{other}'"
        )),
    }
}

fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut mode = Mode::Classify;
    let mut model = "mistral:7b".to_string();
    let mut endpoint = "http://localhost:11434".to_string();
    let mut timeout_secs: u64 = 30;
    let mut prompt_file: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => mode = parse_mode(&next_value(args, &mut i, "--mode")?)?,
            "--model" => model = next_value(args, &mut i, "--model")?,
            "--endpoint" => endpoint = next_value(args, &mut i, "--endpoint")?,
            "--timeout-secs" => {
                let v = next_value(args, &mut i, "--timeout-secs")?;
                timeout_secs = v
                    .parse()
                    .map_err(|_| format!("--timeout-secs requires integer, got {v}"))?;
            }
            "--prompt-file" => prompt_file = Some(next_value(args, &mut i, "--prompt-file")?),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    Ok(CliArgs {
        mode,
        model,
        endpoint,
        timeout_secs,
        prompt_file,
    })
}

fn next_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_help() {
    eprintln!(
        "cli-finding-classifier — Ollama-backed CodeRabbit triage / lint screen

Usage:
  cli-finding-classifier [OPTIONS] < input > output

Options:
  --mode <classify|lint-screen>
                          classify (default): findings を triage
                          lint-screen        : diff を lint 一次フィルタに通す
  --model <name>          Ollama model name (default: mistral:7b)
  --endpoint <url>        Ollama endpoint  (default: http://localhost:11434)
  --timeout-secs <sec>    Per-call timeout (default: 30)
  --prompt-file <path>    Prompt template file (default: mode に応じた built-in)
  -h, --help              Show this help

Mode classify:
  Input  (stdin):  JSON array of Finding (lib-report-formatter schema)
  Output (stdout): JSON array of ClassifiedFinding

Mode lint-screen:
  Input  (stdin):  unified diff (plain text)
  Output (stdout): LintScreenResult JSON object"
    );
}

fn default_prompt(mode: &Mode) -> &'static str {
    match mode {
        Mode::Classify => DEFAULT_PROMPT,
        Mode::LintScreen => LINT_SCREEN_PROMPT,
    }
}

fn load_template(cli: &CliArgs) -> Result<String, String> {
    match &cli.prompt_file {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read prompt file {path}: {e}")),
        None => Ok(default_prompt(&cli.mode).to_string()),
    }
}

fn run_classify(client: &OllamaClient, template: &str, input: &str) -> Result<String, String> {
    let findings: Vec<Finding> =
        serde_json::from_str(input).map_err(|e| format!("invalid Finding JSON on stdin: {e}"))?;
    let classified = classify_batch(client, template, &findings);
    serde_json::to_string_pretty(&classified)
        .map_err(|e| format!("failed to serialize output: {e}"))
}

fn run_lint_screen(client: &OllamaClient, template: &str, input: &str) -> Result<String, String> {
    let result = screen_diff(client, template, input);
    serde_json::to_string_pretty(&result).map_err(|e| format!("failed to serialize output: {e}"))
}

fn run() -> Result<(), String> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let cli = parse_args(&raw_args)?;
    let template = load_template(&cli)?;

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("failed to read stdin: {e}"))?;

    let client = OllamaClient::new(&cli.endpoint, &cli.model)
        .with_timeout(Duration::from_secs(cli.timeout_secs));

    let out = match cli.mode {
        Mode::Classify => run_classify(&client, &template, &input)?,
        Mode::LintScreen => run_lint_screen(&client, &template, &input)?,
    };
    println!("{out}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[cli-finding-classifier] {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_args_when_none_provided() {
        let cli = parse_args(&[]).unwrap();
        assert_eq!(cli.mode, Mode::Classify);
        assert_eq!(cli.model, "mistral:7b");
        assert_eq!(cli.endpoint, "http://localhost:11434");
        assert_eq!(cli.timeout_secs, 30);
        assert!(cli.prompt_file.is_none());
    }

    #[test]
    fn parses_mode_lint_screen() {
        let args = vec!["--mode".into(), "lint-screen".into()];
        let cli = parse_args(&args).unwrap();
        assert_eq!(cli.mode, Mode::LintScreen);
    }

    #[test]
    fn parses_mode_classify_explicitly() {
        let args = vec!["--mode".into(), "classify".into()];
        let cli = parse_args(&args).unwrap();
        assert_eq!(cli.mode, Mode::Classify);
    }

    #[test]
    fn errors_on_invalid_mode_value() {
        let args = vec!["--mode".into(), "bogus".into()];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--mode requires"));
        assert!(err.contains("bogus"));
    }

    #[test]
    fn default_prompt_for_classify_mode_is_classify_template() {
        let cli = parse_args(&[]).unwrap();
        let template = default_prompt(&cli.mode);
        assert!(template.contains("action_confidence"));
    }

    #[test]
    fn default_prompt_for_lint_screen_mode_is_lint_screen_template() {
        let args = vec!["--mode".into(), "lint-screen".into()];
        let cli = parse_args(&args).unwrap();
        let template = default_prompt(&cli.mode);
        assert!(template.contains("lint_findings"));
        assert!(template.contains("screen_decision"));
        assert!(template.contains("{diff}"));
    }

    #[test]
    fn parses_all_flags() {
        let args = vec![
            "--model".into(),
            "llama2:13b".into(),
            "--endpoint".into(),
            "http://example.com".into(),
            "--timeout-secs".into(),
            "60".into(),
            "--prompt-file".into(),
            "custom.txt".into(),
        ];
        let cli = parse_args(&args).unwrap();
        assert_eq!(cli.model, "llama2:13b");
        assert_eq!(cli.endpoint, "http://example.com");
        assert_eq!(cli.timeout_secs, 60);
        assert_eq!(cli.prompt_file.as_deref(), Some("custom.txt"));
    }

    #[test]
    fn errors_on_missing_value() {
        let args = vec!["--model".into()];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--model requires a value"));
    }

    #[test]
    fn errors_on_unknown_flag() {
        let args = vec!["--bogus".into()];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown argument"));
    }

    #[test]
    fn errors_on_non_integer_timeout() {
        let args = vec!["--timeout-secs".into(), "abc".into()];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("integer"));
    }

    #[test]
    fn default_prompt_template_is_embedded() {
        assert!(DEFAULT_PROMPT.contains("auto_fix"));
        assert!(DEFAULT_PROMPT.contains("{severity}"));
        assert!(DEFAULT_PROMPT.contains("{file}"));
    }
}
