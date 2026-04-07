use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_models")]
    pub models: Vec<NerModelBinding>,
    /// When non-empty, all texts are processed with this language model
    /// and auto-detection is skipped entirely.
    #[serde(default)]
    pub default_language: String,
    #[serde(default = "default_fallback_language")]
    pub fallback_language: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
    #[serde(default = "default_max_seq_length")]
    pub max_seq_length: usize,
    #[serde(default = "default_skip_llm_tools_and_system")]
    pub skip_llm_tools_and_system: bool,
    /// List of regex patterns matched against each JSON pointer segment;
    /// matching paths are skipped by NER.
    /// Example: ["^__"] skips all fields starting with double underscore.
    #[serde(default)]
    pub skip_fields: Vec<String>,
    /// List of regex patterns matched against text content; matched
    /// substrings are replaced with spaces before NER inference so the
    /// rest of the text is still processed. Spans are remapped to the
    /// original text automatically.
    /// Example: ['"__typename"\\s*:\\s*"[^"]*"'] strips GraphQL type
    /// discriminator key-value pairs while keeping surrounding content.
    #[serde(default)]
    pub strip_values: Vec<String>,
    /// List of regex patterns; NER hits whose recognized text matches
    /// are discarded. Example: ["^_E_"] skips GraphQL entity type markers.
    #[serde(default)]
    pub exclude_entity_patterns: Vec<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, NerProfileConfig>,
}

fn default_true() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    80
}

fn default_confidence_threshold() -> f32 {
    0.7
}

fn default_max_seq_length() -> usize {
    512
}

fn default_skip_llm_tools_and_system() -> bool {
    true
}

fn default_fallback_language() -> String {
    "en".to_string()
}

fn default_models() -> Vec<NerModelBinding> {
    vec![
        NerModelBinding {
            language: "en".to_string(),
            // Relative to CWD; override in config for production deployments.
            model_path: "./models/bert-base-NER/model.onnx".to_string(),
            tokenizer_path: "./models/bert-base-NER/tokenizer.json".to_string(),
            priority: 100,
        },
        NerModelBinding {
            language: "ru".to_string(),
            model_path: "./models/ner-rubert-tiny-news/model.onnx".to_string(),
            tokenizer_path: "./models/ner-rubert-tiny-news/tokenizer.json".to_string(),
            priority: 100,
        },
    ]
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NerModelBinding {
    pub language: String,
    pub model_path: String,
    pub tokenizer_path: String,
    #[serde(default = "default_binding_priority")]
    pub priority: u16,
}

fn default_binding_priority() -> u16 {
    100
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NerProfileConfig {
    #[serde(default = "default_fail_mode")]
    pub fail_mode: NerFailMode,
    #[serde(default = "default_entity_types")]
    pub entity_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NerFailMode {
    FailOpen,
    FailClosed,
}

fn default_fail_mode() -> NerFailMode {
    NerFailMode::FailOpen
}

fn default_entity_types() -> Vec<String> {
    vec!["person".to_string(), "organization".to_string()]
}
