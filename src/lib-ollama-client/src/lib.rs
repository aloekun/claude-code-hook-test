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

    /// プロンプトを送り、JSON 文字列と Ollama 側 metadata を返す。
    ///
    /// metadata は `prompt_eval_count` / `eval_count` / `num_ctx` を含み、
    /// JSON parse error 発生時の context overflow 診断に使う ([`generate_json`] 内で消費)。
    /// stub 実装は default の空 metadata で十分 (= diagnostic 不要)。
    fn generate_with_metadata(
        &self,
        prompt: &str,
    ) -> Result<(String, OllamaMetadata), OllamaError> {
        let raw = self.generate_raw_json(prompt)?;
        Ok((raw, OllamaMetadata::default()))
    }
}

/// Ollama generate API の metadata (context overflow 診断用)
///
/// `OllamaApi::generate_with_metadata` が返す。`generate_json` が parse error 検知時に
/// stderr へ warn log emit する材料 (PR #136 で「mistral:7b の JSON schema breakdown」を
/// `num_ctx` overflow と誤診せず別アプローチに pivot しかけた事故の構造的予防、
/// 詳細は ADR-038 § Known failure modes 参照)。
#[derive(Debug, Clone, Default)]
pub struct OllamaMetadata {
    /// prompt の token 数 (Ollama API の `prompt_eval_count`)
    pub prompt_eval_count: Option<u32>,
    /// response の token 数 (Ollama API の `eval_count`)
    pub eval_count: Option<u32>,
    /// 呼出時の `num_ctx` setting (overflow 判定の base line)
    pub num_ctx: Option<u32>,
}

/// `OllamaApi::generate_raw_json` を呼んで `T` にデコードする無料関数。
///
/// trait に generic を持たせると dyn-compatible でなくなるため、
/// 型付き API は trait の外で提供する。
///
/// `T` のデシリアライズに失敗した場合は stderr に warn log を出力する
/// ([`OllamaMetadata`] 参照、context overflow 起因の truncation を decisive に診断する目的)。
pub fn generate_json<T: DeserializeOwned>(
    api: &dyn OllamaApi,
    prompt: &str,
) -> Result<T, OllamaError> {
    let (raw, metadata) = api.generate_with_metadata(prompt)?;
    serde_json::from_str::<T>(&raw).map_err(|err| {
        emit_overflow_diagnostic(&err, &raw, &metadata);
        OllamaError::from(err)
    })
}

/// `prompt_eval_count` が `num_ctx` の 90% 以上に達している場合に hint 文字列を返す純粋関数。
///
/// テスタブルに分離されており、`emit_overflow_diagnostic` はこれを呼び出すだけにする。
fn overflow_hint(metadata: &OllamaMetadata) -> Option<String> {
    let (pec, ctx) = (metadata.prompt_eval_count?, metadata.num_ctx?);
    let ratio_pct = (u64::from(pec) * 100) / u64::from(ctx.max(1));
    if ratio_pct >= 90 {
        Some(format!(
            "prompt_eval_count が num_ctx の {}% に達しています。\
             num_ctx を増やすことで解決可能 (`with_num_ctx` で override)",
            ratio_pct
        ))
    } else {
        None
    }
}

/// JSON parse error 検知時の context overflow 診断 log を stderr に emit する。
///
/// metadata が `prompt_eval_count` を持ち、かつ `num_ctx` cap の 90% 以上に達している場合は
/// "context overflow 起因の可能性" を明示する hint も含める。
fn emit_overflow_diagnostic(parse_error: &serde_json::Error, raw: &str, metadata: &OllamaMetadata) {
    let prompt_eval = metadata
        .prompt_eval_count
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".into());
    let eval = metadata
        .eval_count
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".into());
    let num_ctx_disp = metadata
        .num_ctx
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".into());

    eprintln!("[lib-ollama-client] WARN: Ollama JSON output may be truncated.");
    eprintln!("  parse_error: {}", parse_error);
    eprintln!(
        "  prompt_eval_count: {} (vs num_ctx: {})",
        prompt_eval, num_ctx_disp
    );
    eprintln!(
        "  eval_count: {}, response_length: {} chars",
        eval,
        raw.len()
    );

    if let Some(hint) = overflow_hint(metadata) {
        eprintln!("  hint: {}", hint);
    }
}

/// Ollama 既定の `num_ctx` (2048) は本リポジトリの lint-screen prompt + 実 PR diff に対して不足し、
/// prompt が silently truncate される (Ollama は overflow 時に prompt_eval_count を num_ctx に clamp して報告)。
///
/// dogfood の進化 (2048 → 8192 → 16384 → 32768) と各段階の latency / VRAM / overflow 観測値は
/// ADR-040 (docs/adr/adr-040-local-llm-context-size.md) に migrate 済。
/// 32768 は mistral:7b の theoretical max。overflow が再発する場合は diff truncation を
/// classifier 側で実装する次の Phase へ進む。
pub const DEFAULT_NUM_CTX: u32 = 32768;

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
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

impl OllamaClient {
    fn request_envelope(&self, prompt: &str) -> Result<GenerateResponse, OllamaError> {
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
        Ok(envelope)
    }
}

impl OllamaApi for OllamaClient {
    fn generate_raw_json(&self, prompt: &str) -> Result<String, OllamaError> {
        Ok(self.request_envelope(prompt)?.response)
    }

    fn generate_with_metadata(
        &self,
        prompt: &str,
    ) -> Result<(String, OllamaMetadata), OllamaError> {
        let envelope = self.request_envelope(prompt)?;
        let metadata = OllamaMetadata {
            prompt_eval_count: envelope.prompt_eval_count,
            eval_count: envelope.eval_count,
            num_ctx: Some(self.num_ctx),
        };
        Ok((envelope.response, metadata))
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
        let envelope =
            r#"{"model":"mistral:7b","response":"","error":"model not found","done":true}"#;
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

        let overridden =
            OllamaClient::new("http://localhost:11434", "mistral:7b").with_num_ctx(16384);
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
                r#"{"options":{"num_ctx":32768}}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let _: TestPayload = generate_json(&client, "test prompt").unwrap();

        mock.assert();
    }

    #[test]
    fn metadata_carries_prompt_eval_count_when_provided() {
        let mut server = Server::new();
        let inner_json = r#"{"action":"auto_fix","confidence":0.9}"#;
        let envelope = format!(
            r#"{{"model":"mistral:7b","response":{},"done":true,"prompt_eval_count":1234,"eval_count":56}}"#,
            serde_json::to_string(inner_json).unwrap()
        );
        let mock = server
            .mock("POST", "/api/generate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(envelope)
            .create();

        let client = OllamaClient::new(server.url(), "mistral:7b");
        let (_raw, metadata) = client.generate_with_metadata("test prompt").unwrap();

        assert_eq!(metadata.prompt_eval_count, Some(1234));
        assert_eq!(metadata.eval_count, Some(56));
        assert_eq!(metadata.num_ctx, Some(DEFAULT_NUM_CTX));
        mock.assert();
    }

    #[test]
    fn metadata_handles_missing_eval_counts_gracefully() {
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

        let client = OllamaClient::new(server.url(), "mistral:7b").with_num_ctx(4096);
        let (_raw, metadata) = client.generate_with_metadata("test prompt").unwrap();

        assert_eq!(metadata.prompt_eval_count, None);
        assert_eq!(metadata.eval_count, None);
        assert_eq!(metadata.num_ctx, Some(4096));
        mock.assert();
    }

    #[test]
    fn stub_trait_default_returns_empty_metadata() {
        struct StubOllama {
            response: String,
        }
        impl OllamaApi for StubOllama {
            fn generate_raw_json(&self, _prompt: &str) -> Result<String, OllamaError> {
                Ok(self.response.clone())
            }
        }

        let stub = StubOllama {
            response: r#"{"action":"informational","confidence":0.1}"#.to_string(),
        };
        let (raw, metadata) = stub.generate_with_metadata("prompt").unwrap();

        assert_eq!(raw, r#"{"action":"informational","confidence":0.1}"#);
        assert_eq!(metadata.prompt_eval_count, None);
        assert_eq!(metadata.eval_count, None);
        assert_eq!(metadata.num_ctx, None);
    }

    #[test]
    fn overflow_hint_present_when_prompt_eval_count_near_cap() {
        let metadata = OllamaMetadata {
            prompt_eval_count: Some(7400),
            eval_count: Some(50),
            num_ctx: Some(8192),
        };
        let hint = overflow_hint(&metadata);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("90%"));
    }

    #[test]
    fn overflow_hint_absent_when_metadata_absent() {
        assert!(overflow_hint(&OllamaMetadata::default()).is_none());
    }

    #[test]
    fn overflow_hint_absent_below_threshold() {
        let metadata = OllamaMetadata {
            prompt_eval_count: Some(7000),
            eval_count: Some(50),
            num_ctx: Some(8192),
        };
        assert!(overflow_hint(&metadata).is_none());
    }
}
