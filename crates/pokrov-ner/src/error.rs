use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum NerError {
    #[error("ONNX session creation failed: {0}")]
    SessionInit(String),

    #[error("model file not found: {path}")]
    ModelNotFound { path: PathBuf },

    #[error("tokenizer file not found: {path}")]
    TokenizerNotFound { path: PathBuf },

    #[error("inference timeout after {ms}ms")]
    Timeout { ms: u64 },

    #[error("ONNX inference failed: {0}")]
    InferenceFailed(String),

    #[error("tokenization failed: {0}")]
    TokenizationFailed(String),

    #[error("no NER models configured")]
    NoModelsConfigured,
}
