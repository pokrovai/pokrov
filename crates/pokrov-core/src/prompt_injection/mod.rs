//! Prompt-injection detection for untrusted content (MCP tool output, future
//! resources/prompts/RAG). The scanner owns source gating, size limits,
//! threshold classification and fail-mode resolution; detector providers own
//! model-specific tokenization and fragment splitting behind a shared trait.

mod scanner;
#[cfg(test)]
mod scanner_tests;
mod static_detector;
mod types;

pub use scanner::PromptInjectionScanner;
pub use static_detector::StaticPromptInjectionDetector;
pub use types::{
    PromptInjectionAction, PromptInjectionClassification, PromptInjectionDecision,
    PromptInjectionDetection, PromptInjectionDetector, PromptInjectionDetectorDescriptor,
    PromptInjectionDetectorError, PromptInjectionFailMode, PromptInjectionMode,
    PromptInjectionOutcome, PromptInjectionPolicy, PromptInjectionSource,
    PromptInjectionSourcePolicy,
};
