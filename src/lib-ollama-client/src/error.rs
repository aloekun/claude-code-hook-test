//! Ollama client のエラー型

/// Ollama 呼び出し時に発生するエラー
///
/// `Http` / `Api` / `EmptyResponse` は consumer がフォールバックを判断するための
/// 異なる failure mode。`Parse` は LLM の structured output が壊れたケース。
#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Ollama API error: {0}")]
    Api(String),

    #[error("JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Empty response from Ollama")]
    EmptyResponse,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
