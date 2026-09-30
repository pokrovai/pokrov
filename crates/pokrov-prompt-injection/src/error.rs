use std::path::PathBuf;

/// Detector construction and inference failures. Messages carry paths and
/// shape metadata only — never analyzed content.
#[derive(Debug, thiserror::Error)]
pub enum PromptInjectionProviderError {
    #[error("model file not found: {path}")]
    ModelNotFound { path: PathBuf },

    #[error("tokenizer file not found: {path}")]
    TokenizerNotFound { path: PathBuf },

    #[error("ONNX session creation failed: {0}")]
    SessionInit(String),

    #[error("tokenization failed: {0}")]
    TokenizationFailed(String),

    #[error("invalid detector configuration: {0}")]
    InvalidConfig(String),

    #[error("injection label index resolution failed: {0}")]
    LabelResolution(String),

    #[error("ONNX inference failed: {0}")]
    InferenceFailed(String),
}
