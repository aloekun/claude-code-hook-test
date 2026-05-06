//! CodeRabbit findings を Ollama で classify するライブラリ層
//!
//! main.rs は CLI 引数パースと stdin/stdout I/O のみを担当し、
//! 分類本体ロジックは本モジュールに集約する (テスト容易性のため)。

use lib_ollama_client::{generate_json, OllamaApi, OllamaError};
use lib_report_formatter::Finding;
use serde::{Deserialize, Serialize};

/// 分類済み finding (元 Finding を flatten で保持し、enrich field を追加)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClassifiedFinding {
    #[serde(flatten)]
    pub finding: Finding,
    pub action: String,
    pub action_confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_issue: Option<String>,
    /// LLM が呼び出せなかった / 結果が壊れた等のフォールバック理由
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

/// LLM raw output schema (prompts/classify.txt の出力契約と一致)
#[derive(Deserialize, Debug)]
struct LlmClassification {
    action: String,
    action_confidence: f32,
    #[serde(default)]
    normalized_issue: Option<String>,
}

/// プロンプトテンプレートに finding を埋め込む
///
/// 単方向スキャンで置換するため、placeholder の値に別の placeholder 文字列が
/// 含まれていても二重展開しない (例: issue の値が `{suggestion}` を含む場合)。
pub fn build_prompt(template: &str, finding: &Finding) -> String {
    let vars: &[(&str, &str)] = &[
        ("{severity}", finding.severity.as_str()),
        ("{file}", finding.file.as_str()),
        ("{line}", finding.line.as_str()),
        ("{issue}", finding.issue.as_str()),
        ("{suggestion}", finding.suggestion.as_str()),
    ];

    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while !rest.is_empty() {
        let mut matched = false;
        for &(placeholder, value) in vars {
            if rest.starts_with(placeholder) {
                out.push_str(value);
                rest = &rest[placeholder.len()..];
                matched = true;
                break;
            }
        }
        if !matched {
            let mut chars = rest.chars();
            out.push(chars.next().unwrap());
            rest = chars.as_str();
        }
    }

    out
}

const VALID_ACTIONS: &[&str] = &[
    "auto_fix",
    "human_review",
    "false_positive_likely",
    "informational",
];

/// LLM 出力を ClassifiedFinding に変換 + バリデーション
fn from_llm_output(finding: &Finding, llm: LlmClassification) -> ClassifiedFinding {
    let action = if VALID_ACTIONS.contains(&llm.action.as_str()) {
        llm.action
    } else {
        return fallback(
            finding,
            format!("invalid action from LLM: {}", llm.action),
        );
    };
    let confidence = llm.action_confidence.clamp(0.0, 1.0);
    let normalized = llm.normalized_issue
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    ClassifiedFinding {
        finding: finding.clone(),
        action,
        action_confidence: confidence,
        normalized_issue: normalized,
        fallback_reason: None,
    }
}

/// LLM 呼び出し失敗時のフォールバック (human_review に倒す、conf=0.0)
fn fallback(finding: &Finding, reason: impl Into<String>) -> ClassifiedFinding {
    ClassifiedFinding {
        finding: finding.clone(),
        action: "human_review".to_string(),
        action_confidence: 0.0,
        normalized_issue: None,
        fallback_reason: Some(reason.into()),
    }
}

/// 1 件分類 (公開 API)
///
/// Ollama 失敗時は `human_review` + `fallback_reason` を埋めて返す
/// (block しない: consumer は finding を失わず、後段で Claude が判断する)。
pub fn classify_one(
    client: &dyn OllamaApi,
    template: &str,
    finding: &Finding,
) -> ClassifiedFinding {
    let prompt = build_prompt(template, finding);
    match generate_json::<LlmClassification>(client, &prompt) {
        Ok(llm) => from_llm_output(finding, llm),
        Err(e) => fallback(finding, format!("ollama error: {}: {}", llm_err_kind(&e), e)),
    }
}

/// バッチ分類 (公開 API)
pub fn classify_batch(
    client: &dyn OllamaApi,
    template: &str,
    findings: &[Finding],
) -> Vec<ClassifiedFinding> {
    findings
        .iter()
        .map(|f| classify_one(client, template, f))
        .collect()
}

fn llm_err_kind(e: &OllamaError) -> &'static str {
    match e {
        OllamaError::Http(_) => "http",
        OllamaError::Api(_) => "api",
        OllamaError::Parse(_) => "parse",
        OllamaError::EmptyResponse => "empty",
        OllamaError::Io(_) => "io",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct StubOllama {
        responses: RefCell<Vec<Result<String, OllamaError>>>,
        calls: RefCell<Vec<String>>,
    }

    impl StubOllama {
        fn new(responses: Vec<Result<String, OllamaError>>) -> Self {
            Self {
                responses: RefCell::new(responses),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl OllamaApi for StubOllama {
        fn generate_raw_json(&self, prompt: &str) -> Result<String, OllamaError> {
            self.calls.borrow_mut().push(prompt.to_string());
            self.responses.borrow_mut().remove(0)
        }
    }

    fn sample_finding() -> Finding {
        Finding {
            severity: "Critical".into(),
            file: "src/main.rs".into(),
            line: "42".into(),
            issue: "state 書き込み前に daemon をスポーン".into(),
            suggestion: "順序を入れ替える".into(),
            source: "CodeRabbit".into(),
        }
    }

    #[test]
    fn build_prompt_substitutes_all_placeholders() {
        let template = "S={severity} F={file} L={line} I={issue} G={suggestion}";
        let finding = sample_finding();
        let result = build_prompt(template, &finding);
        assert_eq!(
            result,
            "S=Critical F=src/main.rs L=42 I=state 書き込み前に daemon をスポーン G=順序を入れ替える"
        );
    }

    #[test]
    fn build_prompt_does_not_expand_placeholders_in_values() {
        let template = "I={issue} G={suggestion}";
        let finding = Finding {
            severity: "Critical".into(),
            file: "f".into(),
            line: "1".into(),
            issue: "see {suggestion}".into(),
            suggestion: "foo".into(),
            source: "CR".into(),
        };
        let result = build_prompt(template, &finding);
        assert_eq!(result, "I=see {suggestion} G=foo");
    }

    #[test]
    fn classify_one_passes_through_valid_llm_output() {
        let stub = StubOllama::new(vec![Ok(
            r#"{"action":"human_review","action_confidence":0.9,"normalized_issue":"daemon spawn 順序"}"#
                .to_string(),
        )]);
        let result = classify_one(&stub, "T={severity}", &sample_finding());
        assert_eq!(result.action, "human_review");
        assert!((result.action_confidence - 0.9).abs() < f32::EPSILON);
        assert_eq!(result.normalized_issue.as_deref(), Some("daemon spawn 順序"));
        assert!(result.fallback_reason.is_none());
    }

    #[test]
    fn classify_one_falls_back_on_invalid_action() {
        let stub = StubOllama::new(vec![Ok(
            r#"{"action":"delete_file","action_confidence":0.99,"normalized_issue":null}"#
                .to_string(),
        )]);
        let result = classify_one(&stub, "T", &sample_finding());
        assert_eq!(result.action, "human_review");
        assert_eq!(result.action_confidence, 0.0);
        assert!(result
            .fallback_reason
            .as_deref()
            .unwrap()
            .contains("invalid action"));
    }

    #[test]
    fn classify_one_falls_back_on_ollama_error() {
        let stub = StubOllama::new(vec![Err(OllamaError::EmptyResponse)]);
        let result = classify_one(&stub, "T", &sample_finding());
        assert_eq!(result.action, "human_review");
        assert!(result
            .fallback_reason
            .as_deref()
            .unwrap()
            .contains("empty"));
    }

    #[test]
    fn classify_one_falls_back_on_parse_error() {
        let stub = StubOllama::new(vec![Ok("not json".to_string())]);
        let result = classify_one(&stub, "T", &sample_finding());
        assert_eq!(result.action, "human_review");
        assert!(result
            .fallback_reason
            .as_deref()
            .unwrap()
            .contains("parse"));
    }

    #[test]
    fn classify_one_clamps_confidence_to_unit_range() {
        let stub = StubOllama::new(vec![Ok(
            r#"{"action":"auto_fix","action_confidence":2.5}"#.to_string(),
        )]);
        let result = classify_one(&stub, "T", &sample_finding());
        assert_eq!(result.action_confidence, 1.0);
    }

    #[test]
    fn classify_one_treats_empty_normalized_issue_as_none() {
        let stub = StubOllama::new(vec![Ok(
            r#"{"action":"auto_fix","action_confidence":0.5,"normalized_issue":"   "}"#.to_string(),
        )]);
        let result = classify_one(&stub, "T", &sample_finding());
        assert!(result.normalized_issue.is_none());
    }

    #[test]
    fn classify_one_trims_whitespace_from_normalized_issue() {
        let stub = StubOllama::new(vec![Ok(
            r#"{"action":"auto_fix","action_confidence":0.5,"normalized_issue":"  daemon spawn  "}"#
                .to_string(),
        )]);
        let result = classify_one(&stub, "T", &sample_finding());
        assert_eq!(result.normalized_issue.as_deref(), Some("daemon spawn"));
    }

    #[test]
    fn classified_finding_serializes_with_flattened_finding_fields() {
        let stub = StubOllama::new(vec![Ok(
            r#"{"action":"auto_fix","action_confidence":0.9}"#.to_string(),
        )]);
        let result = classify_one(&stub, "T", &sample_finding());
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["severity"], "Critical");
        assert_eq!(json["file"], "src/main.rs");
        assert_eq!(json["action"], "auto_fix");
        assert!(json.get("fallback_reason").is_none());
    }

    #[test]
    fn classify_batch_processes_all_findings_in_order() {
        let stub = StubOllama::new(vec![
            Ok(r#"{"action":"auto_fix","action_confidence":0.8}"#.to_string()),
            Ok(r#"{"action":"informational","action_confidence":0.7}"#.to_string()),
        ]);
        let findings = vec![sample_finding(), sample_finding()];
        let results = classify_batch(&stub, "T", &findings);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].action, "auto_fix");
        assert_eq!(results[1].action, "informational");
    }

    #[test]
    fn classify_batch_partial_failure_does_not_block_others() {
        let stub = StubOllama::new(vec![
            Ok(r#"{"action":"auto_fix","action_confidence":0.8}"#.to_string()),
            Err(OllamaError::EmptyResponse),
            Ok(r#"{"action":"informational","action_confidence":0.7}"#.to_string()),
        ]);
        let findings = vec![sample_finding(), sample_finding(), sample_finding()];
        let results = classify_batch(&stub, "T", &findings);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].action, "auto_fix");
        assert_eq!(results[1].action, "human_review");
        assert!(results[1].fallback_reason.is_some());
        assert_eq!(results[2].action, "informational");
    }
}
