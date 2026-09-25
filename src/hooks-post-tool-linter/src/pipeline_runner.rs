//! 拡張子ごとの lint パイプライン実行 (Layer 2)。
//!
//! `[post_tool_linter].pipelines` または `default_pipelines()` の `PipelineConfig` を
//! file の拡張子に合わせて選択し、各 step を順次実行する。fix step は出力を捨て、
//! check step (fix=false) の stdout/stderr を集約して先頭 20 行を additionalContext として返す。

use crate::config::{Config, PipelineConfig, default_pipelines};
use crate::violation::emit_feedback;
use lib_subprocess::combine_output;
use std::path::Path;
use std::process::Command;

/// ファイル拡張子に一致するパイプラインを検索
pub(crate) fn find_pipeline<'a>(
    file: &str,
    pipelines: &'a [PipelineConfig],
) -> Option<&'a PipelineConfig> {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())?;

    pipelines
        .iter()
        .find(|p| p.extensions.iter().any(|e| e.to_lowercase() == ext))
}

/// コマンドを実行し、(stdout, stderr) を返す。
/// シェル (cmd /c) を経由しないため、ファイルパスのメタ文字によるインジェクションを防止する。
fn run_command(program: &str, args: &[String]) -> (String, String) {
    match Command::new(program).args(args).output() {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (String::new(), format!("Failed to run {}: {}", program, e)),
    }
}

/// args 内のプレースホルダーを置換する。
/// - `{file}`: 編集されたファイルのパス
/// - `{{CLAUDE_DIR}}`: 本 exe が置かれた `.claude/` の絶対パス。本 hook は cwd を正規化
///   しないため、リポジトリ内のスクリプトを cwd に依存せず指すのに使う (順位 287 の
///   exe-relative 解決、ADR-082)
fn resolve_args(args: &[String], file: &str) -> Vec<String> {
    let claude_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_string_lossy().into_owned()));
    resolve_args_with(args, file, claude_dir.as_deref())
}

/// [`resolve_args`] のテスト可能な芯。`{{CLAUDE_DIR}}` を先に置換するので、ファイル
/// パスに同じ文字列が含まれていても展開されない。`claude_dir` が `None` なら残す。
fn resolve_args_with(args: &[String], file: &str, claude_dir: Option<&str>) -> Vec<String> {
    args.iter()
        .map(|a| {
            let a = match claude_dir {
                Some(dir) => a.replace("{{CLAUDE_DIR}}", dir),
                None => a.clone(),
            };
            a.replace("{file}", file)
        })
        .collect()
}

/// パイプラインを実行
pub(crate) fn run_pipeline(file: &str, pipeline: &PipelineConfig) {
    let mut diagnostics = String::new();

    for step in &pipeline.steps {
        let resolved = resolve_args(&step.args, file);
        let (stdout, stderr) = run_command(&step.cmd, &resolved);

        if !step.fix {
            let combined = combine_output(&stdout, &stderr);
            if !combined.trim().is_empty() {
                if !diagnostics.is_empty() {
                    diagnostics.push('\n');
                }
                diagnostics.push_str(&combined);
            }
        }
    }

    let trimmed: String = diagnostics.lines().take(20).collect::<Vec<_>>().join("\n");
    if !trimmed.trim().is_empty() {
        emit_feedback(&trimmed);
    }
}

/// PostToolUse pipeline layer のエントリ。configured pipelines (or default) から
/// file 拡張子に対応する pipeline を選択して実行する。
pub(crate) fn run_pipeline_layer(file: &str, config: Config) {
    let pipelines = config
        .post_tool_linter
        .and_then(|c| c.pipelines)
        .unwrap_or_else(default_pipelines);
    if let Some(pipeline) = find_pipeline(file, &pipelines) {
        run_pipeline(file, pipeline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StepConfig;

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

    #[test]
    fn resolve_args_expands_claude_dir() {
        let args = vec!["{{CLAUDE_DIR}}/../scripts/x.mjs".to_string(), "{file}".to_string()];
        let resolved = resolve_args_with(&args, "src/main.rs", Some("/repo/.claude"));
        assert_eq!(resolved, vec!["/repo/.claude/../scripts/x.mjs", "src/main.rs"]);
    }

    /// ファイルパス由来の文字列は展開しない (placeholder は config 側の args だけに効く)。
    #[test]
    fn resolve_args_does_not_expand_claude_dir_inside_the_file_path() {
        let args = vec!["{file}".to_string()];
        let resolved = resolve_args_with(&args, "{{CLAUDE_DIR}}.rs", Some("/repo/.claude"));
        assert_eq!(resolved, vec!["{{CLAUDE_DIR}}.rs"]);
    }

    #[test]
    fn resolve_args_leaves_claude_dir_when_unresolvable() {
        let args = vec!["{{CLAUDE_DIR}}/x".to_string()];
        assert_eq!(resolve_args_with(&args, "f", None), vec!["{{CLAUDE_DIR}}/x"]);
    }

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
            let last = p.steps.last().unwrap();
            assert!(!last.fix, "Last step should be a check (fix=false)");
        }
    }
}
