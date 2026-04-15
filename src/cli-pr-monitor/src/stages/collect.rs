use std::path::Path;

use crate::log::log_info;
use crate::stages::poll::PollResult;

const OUTPUT_PATH: &str = ".takt/review-comments.json";

/// PollResult を .takt/review-comments.json に書き出す
///
/// instruction (analyze-coderabbit.md) が期待するフィールド:
/// action, summary, ci, coderabbit, findings
pub(crate) fn collect_findings(result: &PollResult) -> bool {
    // instruction が期待するスキーマに合わせたラッパーを構築
    let wrapper = serde_json::json!({
        "action": result.action,
        "summary": result.summary,
        "ci": result.ci,
        "coderabbit": result.coderabbit,
        "findings": result.findings,
        "check_output": result.check_output,
    });

    let output_path = Path::new(OUTPUT_PATH);

    // .takt/ ディレクトリが存在しない場合は作成
    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log_info(&format!("{} ディレクトリ作成失敗: {}", parent.display(), e));
                return false;
            }
        }
    }

    let json = match serde_json::to_string_pretty(&wrapper) {
        Ok(j) => j,
        Err(e) => {
            log_info(&format!("review-comments JSON シリアライズ失敗: {}", e));
            return false;
        }
    };

    match std::fs::write(output_path, &json) {
        Ok(()) => {
            log_info(&format!(
                "書き出し完了: {} ({} bytes)",
                OUTPUT_PATH,
                json.len()
            ));
            true
        }
        Err(e) => {
            log_info(&format!("{} 書き込み失敗: {}", OUTPUT_PATH, e));
            false
        }
    }
}
