//! PostToolUse リンターフック (設定駆動型)
//!
//! Write/Edit ツール使用後にファイルに対してリンター/フォーマッターを実行し、
//! 診断結果を additionalContext として Claude にフィードバックします。
//!
//! .claude/hooks-config.toml の [post_tool_linter] セクションから
//! 拡張子ごとのパイプラインを読み込みます。

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

// --- 入力 ---

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: Option<String>,
    path: Option<String>,
}

// --- 出力 ---

#[derive(Serialize)]
struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

// --- 設定 ---

#[derive(Deserialize, Default)]
struct Config {
    post_tool_linter: Option<PostToolLinterConfig>,
}

#[derive(Deserialize, Default)]
struct PostToolLinterConfig {
    pipelines: Option<Vec<PipelineConfig>>,
}

#[derive(Deserialize, Clone)]
struct PipelineConfig {
    extensions: Vec<String>,
    steps: Vec<StepConfig>,
}

#[derive(Deserialize, Clone)]
struct StepConfig {
    cmd: String,
    args: Vec<String>,
    fix: bool,
}

/// デフォルトパイプライン (設定ファイルが無い場合のフォールバック)
fn default_pipelines() -> Vec<PipelineConfig> {
    vec![
        PipelineConfig {
            extensions: vec!["ts".into(), "tsx".into(), "js".into(), "jsx".into()],
            steps: vec![
                StepConfig { cmd: "npx".into(), args: vec!["--no-install".into(), "biome".into(), "format".into(), "--write".into(), "{file}".into()], fix: true },
                StepConfig { cmd: "npx".into(), args: vec!["--no-install".into(), "oxlint".into(), "--fix".into(), "{file}".into()], fix: true },
                StepConfig { cmd: "npx".into(), args: vec!["--no-install".into(), "oxlint".into(), "{file}".into()], fix: false },
            ],
        },
        PipelineConfig {
            extensions: vec!["py".into()],
            steps: vec![
                StepConfig { cmd: "ruff".into(), args: vec!["check".into(), "--fix".into(), "{file}".into()], fix: true },
                StepConfig { cmd: "ruff".into(), args: vec!["format".into(), "{file}".into()], fix: true },
                StepConfig { cmd: "ruff".into(), args: vec!["check".into(), "{file}".into()], fix: false },
            ],
        },
    ]
}

/// 設定ファイルのパス解決
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む
fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("[post-tool-linter] Warning: Failed to parse {}: {}", path.display(), e);
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

/// ファイル拡張子に一致するパイプラインを検索
fn find_pipeline<'a>(file: &str, pipelines: &'a [PipelineConfig]) -> Option<&'a PipelineConfig> {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())?;

    pipelines.iter().find(|p| p.extensions.iter().any(|e| e.to_lowercase() == ext))
}

/// コマンドを実行し、(stdout, stderr) を返す
/// シェル (cmd /c) を経由しないため、ファイルパスのメタ文字によるインジェクションを防止する
fn run_command(program: &str, args: &[String]) -> (String, String) {
    match Command::new(program).args(args).output() {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (String::new(), format!("Failed to run {}: {}", program, e)),
    }
}

/// stdout と stderr を適切に結合する
fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.ends_with('\n') {
        format!("{}{}", stdout, stderr)
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

/// フィードバック JSON を stdout に出力
fn emit_feedback(message: &str) {
    let output = HookOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PostToolUse".to_string(),
            additional_context: message.to_string(),
        },
    };
    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}

/// args 内の {file} プレースホルダーをファイルパスに置換
fn resolve_args(args: &[String], file: &str) -> Vec<String> {
    args.iter().map(|a| a.replace("{file}", file)).collect()
}

/// パイプラインを実行
fn run_pipeline(file: &str, pipeline: &PipelineConfig) {
    let mut diagnostics = String::new();

    for step in &pipeline.steps {
        let resolved = resolve_args(&step.args, file);
        let (stdout, stderr) = run_command(&step.cmd, &resolved);

        if !step.fix {
            // 診断ステップ: 出力を収集
            let combined = combine_output(&stdout, &stderr);
            if !combined.trim().is_empty() {
                if !diagnostics.is_empty() {
                    diagnostics.push('\n');
                }
                diagnostics.push_str(&combined);
            }
        }
        // fix ステップ: 出力を捨てて続行
    }

    // 診断結果があればフィードバック (先頭20行に制限)
    let trimmed: String = diagnostics.lines().take(20).collect::<Vec<_>>().join("\n");
    if !trimmed.trim().is_empty() {
        emit_feedback(&trimmed);
    }
}

fn main() {
    let config = load_config();

    // stdin を消費（フックの仕様上必須）
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[post-tool-linter] Warning: Failed to read stdin: {}", e);
        return;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return,
    };

    let file = hook_input
        .tool_input
        .and_then(|t| t.file_path.filter(|s| !s.is_empty()).or(t.path))
        .unwrap_or_default();

    if file.is_empty() {
        return;
    }

    let pipelines = config
        .post_tool_linter
        .and_then(|c| c.pipelines)
        .unwrap_or_else(default_pipelines);

    if let Some(pipeline) = find_pipeline(&file, &pipelines) {
        run_pipeline(&file, pipeline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- パイプライン検索テスト ---

    #[test]
    fn finds_ts_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("src/app.ts", &pipelines).is_some());
    }

    #[test]
    fn finds_tsx_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("components/App.tsx", &pipelines).is_some());
    }

    #[test]
    fn finds_js_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("index.js", &pipelines).is_some());
    }

    #[test]
    fn finds_jsx_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("Component.jsx", &pipelines).is_some());
    }

    #[test]
    fn finds_py_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("main.py", &pipelines).is_some());
    }

    #[test]
    fn finds_py_windows_path() {
        let pipelines = default_pipelines();
        assert!(find_pipeline(r"e:\work\project\src\app.py", &pipelines).is_some());
    }

    #[test]
    fn finds_py_case_insensitive() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("file.PY", &pipelines).is_some());
        assert!(find_pipeline("file.Py", &pipelines).is_some());
    }

    #[test]
    fn rs_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("main.rs", &pipelines).is_none());
    }

    #[test]
    fn json_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("package.json", &pipelines).is_none());
    }

    #[test]
    fn no_extension_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("Makefile", &pipelines).is_none());
    }

    #[test]
    fn empty_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("", &pipelines).is_none());
    }

    #[test]
    fn windows_path_ts() {
        let pipelines = default_pipelines();
        assert!(find_pipeline(r"e:\work\project\src\app.ts", &pipelines).is_some());
    }

    #[test]
    fn case_insensitive_ts() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("file.TS", &pipelines).is_some());
        assert!(find_pipeline("file.Tsx", &pipelines).is_some());
    }

    // --- 出力結合 ---

    #[test]
    fn combine_empty_stdout() {
        assert_eq!(combine_output("", "error"), "error");
    }

    #[test]
    fn combine_empty_stderr() {
        assert_eq!(combine_output("output", ""), "output");
    }

    #[test]
    fn combine_both_with_trailing_newline() {
        assert_eq!(combine_output("output\n", "error"), "output\nerror");
    }

    #[test]
    fn combine_both_without_trailing_newline() {
        assert_eq!(combine_output("output", "error"), "output\nerror");
    }

    // --- フィードバック JSON ---

    #[test]
    fn feedback_json_has_correct_structure() {
        let output = HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PostToolUse".to_string(),
                additional_context: "test diagnostic".to_string(),
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains(r#""hookEventName":"PostToolUse""#));
        assert!(json.contains(r#""additionalContext":"test diagnostic""#));
    }

    // --- args 置換 ---

    #[test]
    fn resolve_args_replaces_file() {
        let args = vec!["check".to_string(), "{file}".to_string()];
        let resolved = resolve_args(&args, "src/app.ts");
        assert_eq!(resolved, vec!["check", "src/app.ts"]);
    }

    #[test]
    fn resolve_args_no_placeholder() {
        let args = vec!["--fix".to_string()];
        let resolved = resolve_args(&args, "src/app.ts");
        assert_eq!(resolved, vec!["--fix"]);
    }

    // --- カスタムパイプライン ---

    #[test]
    fn custom_pipeline_matches() {
        let pipelines = vec![PipelineConfig {
            extensions: vec!["go".into()],
            steps: vec![StepConfig {
                cmd: "gofmt".into(),
                args: vec!["-w".into(), "{file}".into()],
                fix: true,
            }],
        }];
        assert!(find_pipeline("main.go", &pipelines).is_some());
        assert!(find_pipeline("main.rs", &pipelines).is_none());
    }

    // --- デフォルトパイプライン ---

    #[test]
    fn default_pipelines_has_ts_and_py() {
        let pipelines = default_pipelines();
        assert_eq!(pipelines.len(), 2);
        assert!(pipelines[0].extensions.contains(&"ts".to_string()));
        assert!(pipelines[1].extensions.contains(&"py".to_string()));
    }

    #[test]
    fn ts_pipeline_has_3_steps() {
        let pipelines = default_pipelines();
        assert_eq!(pipelines[0].steps.len(), 3);
    }

    #[test]
    fn py_pipeline_has_3_steps() {
        let pipelines = default_pipelines();
        assert_eq!(pipelines[1].steps.len(), 3);
    }

    #[test]
    fn fix_steps_come_before_check() {
        let pipelines = default_pipelines();
        for p in &pipelines {
            // fix ステップが先、check (fix=false) が最後
            let last = p.steps.last().unwrap();
            assert!(!last.fix, "Last step should be a check (fix=false)");
        }
    }
}
