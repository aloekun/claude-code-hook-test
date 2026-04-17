use std::path::Path;
use std::process::Command;

use crate::config::DiffConfig;
use crate::log::log_stage;

#[derive(Debug, PartialEq)]
pub(crate) enum DiffResult {
    /// diff に内容があり、ファイルへの書き出しが完了した
    HasContent,
    /// diff 出力が空 (レビュー対象なし、push は続行可能)
    Empty,
    /// diff コマンドの実行またはファイル書き出しに失敗した
    Error,
}

/// diff 取得専用: 出力を切り詰めずに全行を取得する。
/// runner::run_cmd は MAX_LINES=40 で打ち切るため diff には使えない。
fn run_diff_cmd(cmd: &str) -> Result<String, String> {
    let output = Command::new("cmd")
        .args(["/c", cmd])
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", cmd, e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Err(stderr)
    }
}

pub(crate) fn run_diff(config: &DiffConfig) -> DiffResult {
    log_stage("diff", &format!("実行: {}", config.command));

    let output = match run_diff_cmd(&config.command) {
        Ok(output) => output,
        Err(err) => {
            log_stage("diff", "diff コマンド失敗");
            if !err.is_empty() {
                eprintln!("{}", err);
            }
            return DiffResult::Error;
        }
    };

    if output.is_empty() {
        log_stage(
            "diff",
            "diff 出力が空です。レビューをスキップして push に進みます。",
        );
        return DiffResult::Empty;
    }

    let path = Path::new(&config.output_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log_stage("diff", &format!("ディレクトリ作成失敗: {}", e));
            return DiffResult::Error;
        }
    }

    match std::fs::write(path, &output) {
        Ok(()) => {
            let line_count = output.lines().count();
            log_stage(
                "diff",
                &format!("書き出し完了: {} ({} 行)", config.output_path, line_count),
            );
            DiffResult::HasContent
        }
        Err(e) => {
            log_stage("diff", &format!("ファイル書き出し失敗: {}", e));
            DiffResult::Error
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_diff_cmd_captures_more_than_40_lines() {
        let result = run_diff_cmd("for /L %i in (1,1,100) do @echo line %i");
        assert!(result.is_ok(), "command should succeed");
        let output = result.unwrap();
        let line_count = output.lines().count();
        assert!(
            line_count > 40,
            "expected >40 lines captured, got {}; run_diff_cmd must not apply the 40-line cap",
            line_count
        );
    }

    #[test]
    fn run_diff_returns_empty_when_output_is_empty() {
        let out_path = std::env::temp_dir().join("test-run-diff-empty.txt");
        let _ = std::fs::remove_file(&out_path);

        let config = DiffConfig {
            // `type nul` produces zero bytes on Windows.
            command: "type nul".to_string(),
            output_path: out_path.to_string_lossy().into_owned(),
        };

        let result = run_diff(&config);

        assert_eq!(
            result,
            DiffResult::Empty,
            "run_diff must return Empty when the diff command produces empty output"
        );
        assert!(
            !out_path.exists(),
            "output file must not be created for an empty diff"
        );
    }
}
