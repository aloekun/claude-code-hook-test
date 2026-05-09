//! Ollama HTTP API クライアント (blocking)
//!
//! ローカル Ollama daemon (`http://localhost:11434`) に対して
//! `/api/generate` を `format: "json"` 指定で呼び出し、structured JSON を
//! 取得するための薄いラッパー。
//!
//! 設計方針:
//! - blocking I/O のみ (CLI 用途、tokio 不要)
//! - プロンプトはこのライブラリでは生成しない (consumer 責務)
//! - 失敗時は typed error を返し、consumer 側でフォールバック判断
//!
//! 関連 ADR: ADR-038 (試験運用)

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;

mod error;
pub use error::OllamaError;

/// Ollama API の抽象化 (テスト時に stub 差し替え可能)
///
/// dyn-compatible にするため、trait は raw JSON 文字列を返す。
/// 型付きデコードは consumer が行う (or `OllamaClient::generate_json` ヘルパーを使う)。
pub trait OllamaApi {
    /// プロンプトを送り、`format: "json"` で得られた JSON 文字列を返す
    fn generate_raw_json(&self, prompt: &str) -> Result<String, OllamaError>;
}

/// `OllamaApi::generate_raw_json` を呼んで `T` にデコードする無料関数。
///
/// trait に generic を持たせると dyn-compatible でなくなるため、
/// 型付き API は trait の外で提供する。
pub fn generate_json<T: DeserializeOwned>(
    api: &dyn OllamaApi,
    prompt: &str,
) -> Result<T, OllamaError> {
    let raw = api.generate_raw_json(prompt)?;
    let parsed: T = serde_json::from_str(&raw)?;
    Ok(parsed)
}

/// Ollama 既定の `num_ctx` (2048) は本リポジトリの lint-screen prompt
/// (~4000-5000 tokens) に対して不足し、prompt が silently truncate される
/// (PR #135 dogfood で eval13/15 に対して `prompt_eval_count: 4096` の上限到達を実証)。
/// mistral:7b は理論上 32K まで対応するが、安全マージンと推論コストの兼合いで 8192 を default とする。
pub const DEFAULT_NUM_CTX: u32 = 8192;

/// Ollama client 設定
#[derive(Debug, Clone)]
pub struct OllamaClient {
    endpoint: String,
    model: String,
    timeout: Duration,
    temperature: f32,
    num_ctx: u32,
}

impl OllamaClient {
    /// 新しいクライアントを生成
    ///
    /// `endpoint` には `http://localhost:11434` のようなベース URL を指定する
    /// (末尾の `/api/generate` はライブラリ側で付与する)。
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            timeout: Duration::from_secs(30),
            temperature: 0.1,
            num_ctx: DEFAULT_NUM_CTX,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Ollama の `num_ctx` (= context window in tokens) を上書きする。
    ///
    /// 本ライブラリの default ([`DEFAULT_NUM_CTX`]) で大半の用途に十分。
    /// prompt をさらに長く扱う特殊用途のみ使う想定。
    ///
    /// # Panics
    ///
    /// `num_ctx == 0` を与えると panic する (Ollama API は 0 を invalid として
    /// 処理時に error を返すため、build 段階で fail-fast させる)。
    pub fn with_num_ctx(mut self, num_ctx: u32) -> Self {
        assert!(num_ctx > 0, "num_ctx must be greater than 0");
        self.num_ctx = num_ctx;
        self
    }
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    format: &'a str,
    stream: bool,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
    num_ctx: u32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
    #[serde(default)]
    error: Option<String>,
}

impl OllamaApi for OllamaClient {
    fn generate_raw_json(&self, prompt: &str) -> Result<String, OllamaError> {
        let url = format!("{}/api/generate", self.endpoint.trim_end_matches('/'));
        let body = GenerateRequest {
            model: &self.model,
            prompt,
            format: "json",
            stream: false,
            options: GenerateOptions {
                temperature: self.temperature,
                num_ctx: self.num_ctx,
            },
        };

        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();

        let response = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| OllamaError::Http(e.to_string()))?;

        let envelope: GenerateResponse = response.into_json()?;

        if let Some(err) = envelope.error {
            return Err(OllamaError::Api(err));
        }
        if envelope.response.is_empty() {
            return Err(OllamaError::EmptyResponse);
        }
        Ok(envelope.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq)]
    struct TestPayload {
        action: String,
        confidence: f32,
    }

    #[test]
    fn parses_valid_json_response_via_typed_helper() {
        let mut server = Server::new();
        let inner_json = r#"{"action":"auto_fix","confidence":0.9}"#;
        let envelope = format!(
            r#"{{"model":"mistral:7b","response":{},"done":true}}"#,
            serde_json::to_string(inner_json).unwrap()
        );
        let mock = server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let result: TestPayload = generate_json(&client, "test prompt").unwrap();

        assert_eq!(
            result,
            TestPayload {
                action: "auto_fix".to_string(),
                confidence: 0.9
            }
        );
        mock.assert();
    }

    #[test]
    fn raw_json_returns_inner_response_string() {
        let mut server = Server::new();
        let inner = r#"{"action":"informational","confidence":0.5}"#;
        let envelope = format!(
            r#"{{"response":{},"done":true}}"#,
            serde_json::to_string(inner).unwrap()
        );
        server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let raw = client.generate_raw_json("test").unwrap();
        assert_eq!(raw, inner);
    }

    #[test]
    fn returns_api_error_when_ollama_returns_error_field() {
        let mut server = Server::new();
        let envelope = r#"{"model":"mistral:7b","response":"","error":"model not found","done":true}"#;
        server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let result = client.generate_raw_json("test");
        assert!(matches!(result, Err(OllamaError::Api(_))));
    }

    #[test]
    fn returns_empty_response_error_when_response_is_blank() {
        let mut server = Server::new();
        let envelope = r#"{"model":"mistral:7b","response":"","done":true}"#;
        server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let result = client.generate_raw_json("test");
        assert!(matches!(result, Err(OllamaError::EmptyResponse)));
    }

    #[test]
    fn typed_helper_returns_parse_error_when_response_is_not_valid_json() {
        let mut server = Server::new();
        let envelope = r#"{"model":"mistral:7b","response":"not json at all","done":true}"#;
        server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let result: Result<TestPayload, _> = generate_json(&client, "test");
        assert!(matches!(result, Err(OllamaError::Parse(_))));
    }

    #[test]
    fn returns_http_error_when_server_returns_500() {
        let mut server = Server::new();
        server
            .mock("POST", "/api/generate")
            .with_status(500)
            .with_body("internal error")
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let result = client.generate_raw_json("test");
        assert!(matches!(result, Err(OllamaError::Http(_))));
    }

    #[test]
    fn endpoint_with_trailing_slash_is_normalized() {
        let mut server = Server::new();
        let inner = r#"{"action":"informational","confidence":0.5}"#;
        let envelope = format!(
            r#"{{"response":{},"done":true}}"#,
            serde_json::to_string(inner).unwrap()
        );
        server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_body(envelope)
            .create();

        let url_with_slash = format!("{}/", server.url());
        let client = OllamaClient::new(url_with_slash, "mistral:7b");
        let raw = client.generate_raw_json("test").unwrap();
        assert!(raw.contains("informational"));
    }

    #[test]
    fn temperature_and_timeout_are_configurable() {
        let client = OllamaClient::new("http://localhost:11434", "mistral:7b")
            .with_timeout(Duration::from_secs(60))
            .with_temperature(0.5);
        assert_eq!(client.timeout, Duration::from_secs(60));
        assert!((client.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn num_ctx_defaults_and_overrides_apply() {
        let default_client = OllamaClient::new("http://localhost:11434", "mistral:7b");
        assert_eq!(default_client.num_ctx, DEFAULT_NUM_CTX);

        let overridden = OllamaClient::new("http://localhost:11434", "mistral:7b")
            .with_num_ctx(16384);
        assert_eq!(overridden.num_ctx, 16384);
    }

    #[test]
    #[should_panic(expected = "num_ctx must be greater than 0")]
    fn with_num_ctx_panics_on_zero() {
        let _ = OllamaClient::new("http://localhost:11434", "mistral:7b").with_num_ctx(0);
    }

    #[test]
    fn num_ctx_is_serialized_into_request_body() {
        let mut server = Server::new();
        let inner_json = r#"{"action":"auto_fix","confidence":0.9}"#;
        let envelope = format!(
            r#"{{"model":"mistral:7b","response":{},"done":true}}"#,
            serde_json::to_string(inner_json).unwrap()
        );
        let mock = server
            .mock("POST", "/api/generate")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"options":{"num_ctx":8192}}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let _: TestPayload = generate_json(&client, "test prompt").unwrap();

        mock.assert();
    }
}
