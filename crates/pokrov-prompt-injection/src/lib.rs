//! Local ONNX prompt-injection detector provider.
//!
//! Implements the `pokrov-core::prompt_injection` detector contract for
//! sequence-classification models (e.g. HikmaAI mDeBERTa, Prompt Guard 2):
//! provider-owned token windowing with overlap, max-score aggregation across
//! windows, and metadata-only results. The `pokrov-pi-bench` binary ships in
//! this crate so benchmark runs exercise the same code path as production.

mod detector;
mod error;
mod label_index;

pub use detector::{ChunkingLimits, LocalOnnxPromptInjectionDetector};
pub use error::PromptInjectionProviderError;
