use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Untrusted-content origin classes accepted by the prompt-injection stage.
/// Variants without a wired call site (tool descriptions, resources, prompts,
/// RAG) are declared so that the detector contract and policy model stay
/// stable when those processing paths are introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptInjectionSource {
    McpToolOutput,
    McpToolDescription,
    McpResource,
    McpPrompt,
    Rag,
    External,
}

impl PromptInjectionSource {
    /// Stable metadata string for audit fields and metrics labels.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::McpToolOutput => "mcp_tool_output",
            Self::McpToolDescription => "mcp_tool_description",
            Self::McpResource => "mcp_resource",
            Self::McpPrompt => "mcp_prompt",
            Self::Rag => "rag",
            Self::External => "external",
        }
    }
}

/// Binary classification produced from a detector score and the resolved
/// threshold. A `suspicious` band may be introduced later without changing
/// policy resolution, which keys on the numeric score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptInjectionClassification {
    Benign,
    Injection,
}

impl PromptInjectionClassification {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Benign => "benign",
            Self::Injection => "injection",
        }
    }
}

/// Provider-level classification result for one submitted text.
/// `chunks_processed` reports how many model-context fragments the provider
/// evaluated internally; splitting itself is provider-owned because window
/// boundaries depend on the model's tokenizer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PromptInjectionDetection {
    pub score: f32,
    pub chunks_processed: u32,
}

/// Static identity of a detector instance; carried into audit and metrics.
/// `provider` is a bounded label value (`onnx`, `static`, future `external_http`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInjectionDetectorDescriptor {
    pub detector_id: String,
    pub provider: String,
    pub model_id: String,
}

/// Detector failures expressed without inspected content. Implementations
/// must never embed analyzed text in error messages.
#[derive(Debug, thiserror::Error)]
pub enum PromptInjectionDetectorError {
    #[error("detector unavailable: {0}")]
    Unavailable(String),
    #[error("detector inference timed out after {0}ms")]
    Timeout(u64),
    #[error("detector inference failed: {0}")]
    InferenceFailed(String),
    /// The content tail did not fit into the configured chunk budget. Reported
    /// as an error rather than a partial score so an unevaluated fragment can
    /// never masquerade as a benign verdict.
    #[error("content exceeds detector chunk coverage")]
    ContentTruncated,
}

/// Pluggable prompt-injection classifier contract.
///
/// Implementations own model-specific tokenization and fragment splitting for
/// their context window; the orchestrating scanner enforces content size
/// limits before invoking `detect`. Callers in async contexts must isolate
/// this blocking call (e.g. `tokio::task::spawn_blocking`).
pub trait PromptInjectionDetector: Send + Sync {
    /// Static identity for audit and metrics labels.
    fn descriptor(&self) -> &PromptInjectionDetectorDescriptor;

    /// Classifies `text` and returns the maximum fragment score together with
    /// the number of fragments evaluated. Thresholds are intentionally absent:
    /// score-to-classification mapping is a policy concern.
    fn detect(
        &self,
        text: &str,
    ) -> Result<PromptInjectionDetection, PromptInjectionDetectorError>;
}

/// What the policy applies to content classified as injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptInjectionAction {
    /// Record the detection in audit/metrics but never block.
    Allow,
    /// Block the content from reaching the model/agent when enforcing.
    #[default]
    Block,
}

/// Whether computed decisions are applied or only observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptInjectionMode {
    /// Decisions are applied; `block` withholds content from the model.
    #[default]
    Enforce,
    /// Decisions are computed and audited but never applied; used for
    /// threshold calibration during initial rollout.
    DryRun,
}

/// Behaviour when the detector cannot produce a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptInjectionFailMode {
    /// Pass content through marked as degraded.
    #[default]
    FailOpen,
    /// Withhold content when detection cannot complete.
    FailClosed,
}

impl PromptInjectionFailMode {
    /// Stable metadata string for metrics labels and audit fields.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FailOpen => "fail_open",
            Self::FailClosed => "fail_closed",
        }
    }
}

/// Per-source enablement and optional threshold override.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PromptInjectionSourcePolicy {
    pub enabled: bool,
    pub threshold: Option<f32>,
}

/// Runtime policy for the prompt-injection stage.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptInjectionPolicy {
    pub action: PromptInjectionAction,
    pub mode: PromptInjectionMode,
    pub fail_mode: PromptInjectionFailMode,
    /// Default score threshold applied when a source has no override.
    pub threshold: f32,
    /// Total wall-time budget for one detector call, including all fragments.
    pub timeout_ms: u64,
    /// Hard cap on submitted content size enforced before inference.
    pub max_content_bytes: usize,
    pub sources: BTreeMap<PromptInjectionSource, PromptInjectionSourcePolicy>,
}

/// Effective decision after threshold/action/mode/fail-mode resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptInjectionDecision {
    Allow,
    Block,
}

impl PromptInjectionDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
        }
    }
}

/// Metadata-only outcome of one source evaluation. Contains detector identity,
/// bucketable score and counters — never the inspected content or fragments.
#[derive(Debug, Clone)]
pub struct PromptInjectionOutcome {
    pub source: PromptInjectionSource,
    pub detector_id: String,
    pub provider: String,
    pub model_id: String,
    /// `None` when detection did not run to completion (degraded outcome).
    pub classification: Option<PromptInjectionClassification>,
    pub score: Option<f32>,
    pub threshold: f32,
    pub decision: PromptInjectionDecision,
    /// True when the same outcome would have blocked under `enforce` mode:
    /// a detected injection under `action: block`, or a degraded outcome
    /// under `fail_closed`. `action: allow` never sets this flag.
    pub would_block: bool,
    /// Configured failure behaviour; carried for metrics labels so consumers
    /// do not need to re-resolve policy.
    pub fail_mode: PromptInjectionFailMode,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub chunks_processed: u32,
    pub duration_ms: u64,
}
