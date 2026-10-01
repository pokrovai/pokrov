use std::collections::BTreeMap;

use pokrov_core::prompt_injection::{
    PromptInjectionAction, PromptInjectionFailMode, PromptInjectionMode, PromptInjectionPolicy,
    PromptInjectionSource, PromptInjectionSourcePolicy,
};
use serde::{Deserialize, Serialize};

/// Prompt-injection detection stage configuration.
///
/// Disabled by default. When enabled, untrusted content from the configured
/// sources is classified before it reaches the agent/LLM; see
/// `specs/Prompt Injection.md` for the threat model.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptInjectionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: PromptInjectionProviderConfig,
    /// Default score threshold: `score >= threshold` classifies as injection.
    /// Per-source overrides live under `sources.<name>.threshold`.
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    /// What enforcement applies to injected content: `allow` records
    /// detections without blocking; `block` withholds content when enforcing.
    #[serde(default)]
    pub action: PromptInjectionAction,
    /// `dry_run` computes and audits decisions without applying them.
    #[serde(default)]
    pub mode: PromptInjectionMode,
    /// `fail_closed` withholds content when detection cannot complete.
    #[serde(default)]
    pub fail_mode: PromptInjectionFailMode,
    /// Wall-time budget for one detector call including all fragment
    /// inferences. Must exceed `chunking.max_chunks` × per-chunk model cost
    /// (~0.15 s measured on HikmaAI int8, debug build) or long content
    /// degrades as `detector_timeout` instead of being scanned.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub chunking: PromptInjectionChunkingConfig,
    #[serde(default)]
    pub sources: PromptInjectionSourcesConfig,
}

impl Default for PromptInjectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: PromptInjectionProviderConfig::default(),
            threshold: default_threshold(),
            action: PromptInjectionAction::default(),
            mode: PromptInjectionMode::default(),
            fail_mode: PromptInjectionFailMode::default(),
            timeout_ms: default_timeout_ms(),
            chunking: PromptInjectionChunkingConfig::default(),
            sources: PromptInjectionSourcesConfig::default(),
        }
    }
}

impl PromptInjectionConfig {
    /// Maps YAML-facing config into the core scanner policy. Sources absent
    /// from the map are treated as disabled by the scanner.
    pub fn to_policy(&self) -> PromptInjectionPolicy {
        let mut sources = BTreeMap::new();
        for (source, setting) in [
            (PromptInjectionSource::McpToolOutput, &self.sources.mcp_tool_output),
            (PromptInjectionSource::McpToolDescription, &self.sources.mcp_tool_description),
            (PromptInjectionSource::McpResource, &self.sources.mcp_resource),
            (PromptInjectionSource::McpPrompt, &self.sources.mcp_prompt),
            (PromptInjectionSource::Rag, &self.sources.rag),
        ] {
            let (enabled, threshold) = setting.resolve();
            sources.insert(source, PromptInjectionSourcePolicy { enabled, threshold });
        }

        PromptInjectionPolicy {
            action: self.action,
            mode: self.mode,
            fail_mode: self.fail_mode,
            threshold: self.threshold,
            timeout_ms: self.timeout_ms,
            max_content_bytes: self.chunking.max_content_bytes,
            sources,
        }
    }
}

/// Detector backend. `onnx` requires the `prompt-injection` runtime feature;
/// `static` is a deterministic test provider, not a security control.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptInjectionProviderConfig {
    /// Local sequence-classification model via ONNX Runtime.
    Onnx {
        /// Model identifier used for audit/metrics (`model_id`).
        model: String,
        model_path: String,
        tokenizer_path: String,
        /// Positive-class logit index override; resolved from `config.json`
        /// `id2label` next to the model when unset.
        #[serde(default)]
        injection_label_index: Option<u32>,
    },
    /// Substring-matching stub for exercising the scan/policy/audit path
    /// without a model file. Never enable as a real protection boundary.
    Static {
        #[serde(default)]
        static_match: Vec<String>,
        #[serde(default = "default_static_score")]
        static_score: f32,
    },
    /// Placeholder for configs that keep the section but run no detector.
    /// Invalid while `enabled` is true.
    #[default]
    None,
}

/// Provider-internal chunking limits plus the scanner-side content cap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptInjectionChunkingConfig {
    /// Model context window in tokens, including special tokens.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Token overlap between adjacent windows.
    #[serde(default = "default_overlap_tokens")]
    pub overlap_tokens: usize,
    /// Maximum number of windows evaluated per text. Content that does not
    /// fit within the window budget degrades the evaluation
    /// (`content_truncated`) instead of silently scoring only the prefix.
    #[serde(default = "default_max_chunks")]
    pub max_chunks: usize,
    /// Scanner-enforced cap on submitted content size in bytes.
    #[serde(default = "default_max_content_bytes")]
    pub max_content_bytes: usize,
}

impl Default for PromptInjectionChunkingConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            overlap_tokens: default_overlap_tokens(),
            max_chunks: default_max_chunks(),
            max_content_bytes: default_max_content_bytes(),
        }
    }
}

/// Per-source enablement. Untagged so both `mcp_tool_output: true` and the
/// detailed `{enabled, threshold}` form parse.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptInjectionSourcesConfig {
    /// Enabled by default: `mcp_tool_output` is the required v1 source.
    #[serde(default = "default_mcp_tool_output_setting")]
    pub mcp_tool_output: PromptInjectionSourceSetting,
    #[serde(default)]
    pub mcp_tool_description: PromptInjectionSourceSetting,
    #[serde(default)]
    pub mcp_resource: PromptInjectionSourceSetting,
    #[serde(default)]
    pub mcp_prompt: PromptInjectionSourceSetting,
    #[serde(default)]
    pub rag: PromptInjectionSourceSetting,
}

impl Default for PromptInjectionSourcesConfig {
    fn default() -> Self {
        Self {
            mcp_tool_output: default_mcp_tool_output_setting(),
            mcp_tool_description: PromptInjectionSourceSetting::default(),
            mcp_resource: PromptInjectionSourceSetting::default(),
            mcp_prompt: PromptInjectionSourceSetting::default(),
            rag: PromptInjectionSourceSetting::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PromptInjectionSourceSetting {
    /// `source: true|false` shorthand; `true` inherits the global threshold.
    Flag(bool),
    /// Detailed form with an optional per-source threshold override.
    Detailed { enabled: bool, #[serde(default)] threshold: Option<f32> },
}

impl Default for PromptInjectionSourceSetting {
    fn default() -> Self {
        Self::Flag(false)
    }
}

impl PromptInjectionSourceSetting {
    /// Resolves to `(enabled, threshold_override)`.
    pub const fn resolve(&self) -> (bool, Option<f32>) {
        match self {
            Self::Flag(enabled) => (*enabled, None),
            Self::Detailed { enabled, threshold } => (*enabled, *threshold),
        }
    }
}

fn default_threshold() -> f32 {
    0.9
}

fn default_timeout_ms() -> u64 {
    10_000
}

fn default_max_tokens() -> usize {
    512
}

fn default_overlap_tokens() -> usize {
    64
}

fn default_max_chunks() -> usize {
    32
}

fn default_max_content_bytes() -> usize {
    262_144
}

fn default_static_score() -> f32 {
    0.99
}

fn default_mcp_tool_output_setting() -> PromptInjectionSourceSetting {
    PromptInjectionSourceSetting::Flag(true)
}
