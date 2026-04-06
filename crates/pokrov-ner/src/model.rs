use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NerEntityType {
    Person,
    Organization,
}

impl NerEntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NerModelBinding {
    pub language: String,
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    #[serde(default = "default_binding_priority")]
    pub priority: u16,
}

fn default_binding_priority() -> u16 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NerConfig {
    #[serde(default = "default_models")]
    pub models: Vec<NerModelBinding>,
    #[serde(default = "default_fallback_language")]
    pub fallback_language: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_seq_length")]
    pub max_seq_length: usize,
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
}

fn default_timeout_ms() -> u64 {
    80
}

fn default_max_seq_length() -> usize {
    512
}

fn default_confidence_threshold() -> f32 {
    0.7
}

fn default_fallback_language() -> String {
    "en".to_string()
}

fn default_models() -> Vec<NerModelBinding> {
    vec![
        NerModelBinding {
            language: "en".to_string(),
            model_path: PathBuf::from("models/bert-base-NER/model.onnx"),
            tokenizer_path: PathBuf::from("models/bert-base-NER/tokenizer.json"),
            priority: 100,
        },
        NerModelBinding {
            language: "ru".to_string(),
            model_path: PathBuf::from("models/ner-rubert-tiny-news/model.onnx"),
            tokenizer_path: PathBuf::from("models/ner-rubert-tiny-news/tokenizer.json"),
            priority: 100,
        },
    ]
}

impl Default for NerConfig {
    fn default() -> Self {
        Self {
            models: default_models(),
            fallback_language: default_fallback_language(),
            timeout_ms: default_timeout_ms(),
            max_seq_length: default_max_seq_length(),
            confidence_threshold: default_confidence_threshold(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NerHit {
    pub entity: NerEntityType,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub score: f32,
    pub language: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_type_str_roundtrip() {
        assert_eq!(NerEntityType::Person.as_str(), "person");
        assert_eq!(NerEntityType::Organization.as_str(), "organization");
    }

    #[test]
    fn config_defaults() {
        let config = NerConfig::default();
        assert_eq!(config.models.len(), 2);
        assert_eq!(config.fallback_language, "en");
        assert_eq!(config.timeout_ms, 80);
        assert_eq!(config.max_seq_length, 512);
        assert!((config.confidence_threshold - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn ner_hit_serializes_without_raw_payload_leak() {
        let hit = NerHit {
            entity: NerEntityType::Person,
            text: "Ivan Petrov".to_string(),
            start: 0,
            end: 11,
            score: 0.95,
            language: "ru".to_string(),
        };
        let json = serde_json::to_string(&hit).unwrap();
        assert!(json.contains("person"));
        assert!(json.contains("Ivan Petrov"));
    }
}
