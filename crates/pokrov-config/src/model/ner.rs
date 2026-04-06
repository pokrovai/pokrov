use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_models")]
    pub models: Vec<NerModelBinding>,
    #[serde(default = "default_fallback_language")]
    pub fallback_language: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
    #[serde(default = "default_max_seq_length")]
    pub max_seq_length: usize,
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
