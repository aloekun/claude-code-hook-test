//! 共通 lint 違反出力型と additionalContext 用 JSON 構造体。
//!
//! `LintViolation` は CodeRabbit 記事のフォーマット (type / severity / location / message /
//! why / fix / example) に準拠した構造で、custom-lint と utf8-integrity の両方が共通利用する。
//! `HookOutput` は PostToolUse hook の stdout で Claude に渡す JSON 包装。

use serde::Serialize;

/// カスタムルール / utf8-integrity 違反の最大出力件数 (外部ツール診断の 20 行制限と同等)。
pub(crate) const MAX_CUSTOM_VIOLATIONS: usize = 20;

#[derive(Serialize)]
pub(crate) struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub(crate) hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
pub(crate) struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub(crate) hook_event_name: String,
    #[serde(rename = "additionalContext")]
    pub(crate) additional_context: String,
}

#[derive(Serialize)]
pub(crate) struct LintViolation {
    pub(crate) r#type: String,
    pub(crate) severity: String,
    pub(crate) location: ViolationLocation,
    pub(crate) message: String,
    pub(crate) why: String,
    pub(crate) fix: ViolationFix,
    pub(crate) example: ViolationExample,
}

#[derive(Serialize)]
pub(crate) struct ViolationLocation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) symbol: String,
}

#[derive(Serialize)]
pub(crate) struct ViolationFix {
    pub(crate) strategy: String,
    pub(crate) steps: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct ViolationExample {
    pub(crate) bad: String,
    pub(crate) good: String,
}

/// フィードバック JSON を stdout に出力
pub(crate) fn emit_feedback(message: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
