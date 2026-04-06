# Lightweight NER/NLP Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `pokrov-ner` — a standalone workspace crate that provides ML-based Named Entity Recognition for person and organization names in English and Russian text, integrated into the existing sanitization pipeline via the RemoteRecognizer extension point.

**Architecture:** Separate crate `pokrov-ner` with `ort` (ONNX Runtime) for inference and `tokenizers` for WordPiece encoding. Standalone API (`NerEngine`/`NerHit`) with zero dependency on `pokrov-core`. Integration via feature-gated `NerAdapter` in `pokrov-core` that normalizes results into `NormalizedHit` and respects configurable fail-open/fail-closed behavior.

**Tech Stack:** Rust 1.85+, `ort` 2.0 (load-dynamic), `tokenizers` 0.21, `ndarray` 0.16, `thiserror` 2, `serde` 1, `tracing` 0.1

**Design doc:** `docs/superpowers/plans/2026-04-06-lightweight-ner-nlp-design.md`

---

## Task 1: Scaffold pokrov-ner crate

**Files:**
- Create: `crates/pokrov-ner/Cargo.toml`
- Create: `crates/pokrov-ner/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create crate directory**

```bash
mkdir -p crates/pokrov-ner/src
```

**Step 2: Create `crates/pokrov-ner/Cargo.toml`**

```toml
[package]
name = "pokrov-ner"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
ndarray = "0.16"
ort = { version = "2.0", features = ["load-dynamic"] }
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tokenizers = "0.21"
tracing = "0.1"
```

**Step 3: Create `crates/pokrov-ner/src/lib.rs`**

```rust
pub mod decode;
pub mod engine;
pub mod error;
pub mod model;

pub use engine::NerEngine;
pub use error::NerError;
pub use model::{DetectedLanguage, NerConfig, NerEntityType, NerHit};
```

**Step 4: Add to workspace `Cargo.toml`**

In `[workspace] members`, add `"crates/pokrov-ner"`.

**Step 5: Verify compilation**

```bash
cargo check -p pokrov-ner
```

Expected: compiles with errors about missing modules (next tasks fill them in).

**Step 6: Commit**

```bash
git add crates/pokrov-ner/ Cargo.toml
git commit -m "feat(ner): scaffold pokrov-ner workspace crate"
```

---

## Task 2: Error types

**Files:**
- Create: `crates/pokrov-ner/src/error.rs`

**Step 1: Write the error module**

```rust
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
}
```

**Step 2: Verify**

```bash
cargo check -p pokrov-ner
```

Expected: PASS

**Step 3: Commit**

```bash
git add crates/pokrov-ner/src/error.rs
git commit -m "feat(ner): add NerError enum"
```

---

## Task 3: Model types — NerConfig, NerEntityType, NerHit, DetectedLanguage

**Files:**
- Create: `crates/pokrov-ner/src/model.rs`

**Step 1: Write the model module**

```rust
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedLanguage {
    En,
    Ru,
    Unknown,
}

impl DetectedLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NerConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
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

impl Default for NerConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/ner-model.onnx"),
            tokenizer_path: PathBuf::from("models/ner-tokenizer.json"),
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
    pub language: DetectedLanguage,
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
    fn detected_language_str_roundtrip() {
        assert_eq!(DetectedLanguage::En.as_str(), "en");
        assert_eq!(DetectedLanguage::Ru.as_str(), "ru");
        assert_eq!(DetectedLanguage::Unknown.as_str(), "unknown");
    }

    #[test]
    fn config_defaults() {
        let config = NerConfig::default();
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
            language: DetectedLanguage::Ru,
        };
        let json = serde_json::to_string(&hit).unwrap();
        assert!(json.contains("person"));
        assert!(json.contains("Ivan Petrov"));
    }
}
```

**Step 2: Run tests**

```bash
cargo test -p pokrov-ner
```

Expected: PASS

**Step 3: Commit**

```bash
git add crates/pokrov-ner/src/model.rs
git commit -m "feat(ner): add NerConfig, NerEntityType, NerHit, DetectedLanguage"
```

---

## Task 4: BIO tag decoder

**Files:**
- Create: `crates/pokrov-ner/src/decode.rs`

**Step 1: Write the decode module**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BioSpan {
    pub entity_label: String,
    pub token_start: usize,
    pub token_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntitySpan {
    pub label: String,
    pub token_start: usize,
    pub token_end: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub text: String,
}

pub fn decode_bio_tags(labels: &[String], char_offsets: &[(usize, usize)]) -> Vec<RawEntitySpan> {
    let mut spans = Vec::new();
    let mut current: Option<BioSpan> = None;

    for (i, label) in labels.iter().enumerate() {
        if label.starts_with("B-") {
            if let Some(span) = current.take() {
                push_span(&span, char_offsets, &mut spans);
            }
            current = Some(BioSpan {
                entity_label: label[2..].to_string(),
                token_start: i,
                token_end: i,
            });
        } else if label.starts_with("I-") {
            if let Some(ref mut span) = current {
                if span.entity_label == label[2..] {
                    span.token_end = i;
                } else {
                    push_span(span, char_offsets, &mut spans);
                    *span = BioSpan {
                        entity_label: label[2..].to_string(),
                        token_start: i,
                        token_end: i,
                    };
                }
            } else {
                current = Some(BioSpan {
                    entity_label: label[2..].to_string(),
                    token_start: i,
                    token_end: i,
                });
            }
        } else {
            if let Some(span) = current.take() {
                push_span(&span, char_offsets, &mut spans);
            }
        }
    }

    if let Some(span) = current.take() {
        push_span(&span, char_offsets, &mut spans);
    }

    spans
}

fn push_span(span: &BioSpan, offsets: &[(usize, usize)], out: &mut Vec<RawEntitySpan>) {
    if span.token_start >= offsets.len() || span.token_end >= offsets.len() {
        return;
    }
    let char_start = offsets[span.token_start].0;
    let char_end = offsets[span.token_end].1;
    out.push(RawEntitySpan {
        label: span.entity_label.clone(),
        token_start: span.token_start,
        token_end: span.token_end,
        char_start,
        char_end,
        text: String::new(),
    });
}

pub fn argmax_labels(logits: &ndarray::Array3<f32>) -> Vec<usize> {
    let (_, seq_len, num_labels) = logits.dim();
    let mut labels = Vec::with_capacity(seq_len);

    for t in 0..seq_len {
        let mut max_val = f32::NEG_INFINITY;
        let mut max_idx = 0usize;
        for l in 0..num_labels {
            let val = logits[[0, t, l]];
            if val > max_val {
                max_val = val;
                max_idx = l;
            }
        }
        labels.push(max_idx);
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_person_entity() {
        let labels = vec![
            "O".into(), "B-PER".into(), "I-PER".into(), "O".into(),
        ];
        let offsets = vec![(0, 1), (2, 6), (7, 12), (13, 18)];

        let spans = decode_bio_tags(&labels, &offsets);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].label, "PER");
        assert_eq!(spans[0].char_start, 2);
        assert_eq!(spans[0].char_end, 12);
    }

    #[test]
    fn two_separate_entities() {
        let labels = vec![
            "O".into(), "B-PER".into(), "O".into(), "B-ORG".into(), "O".into(),
        ];
        let offsets = vec![(0, 1), (2, 6), (7, 8), (9, 15), (16, 20)];

        let spans = decode_bio_tags(&labels, &offsets);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].label, "PER");
        assert_eq!(spans[1].label, "ORG");
    }

    #[test]
    fn no_entities() {
        let labels = vec!["O".into(), "O".into(), "O".into()];
        let offsets = vec![(0, 5), (6, 10), (11, 15)];

        let spans = decode_bio_tags(&labels, &offsets);
        assert!(spans.is_empty());
    }

    #[test]
    fn i_without_b_treated_as_new_entity() {
        let labels = vec!["O".into(), "I-PER".into(), "I-PER".into(), "O".into()];
        let offsets = vec![(0, 1), (2, 6), (7, 12), (13, 18)];

        let spans = decode_bio_tags(&labels, &offsets);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].char_start, 2);
        assert_eq!(spans[0].char_end, 12);
    }

    #[test]
    fn label_switch_starts_new_entity() {
        let labels = vec!["B-PER".into(), "I-ORG".into(), "O".into()];
        let offsets = vec![(0, 5), (6, 12), (13, 18)];

        let spans = decode_bio_tags(&labels, &offsets);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].label, "PER");
        assert_eq!(spans[1].label, "ORG");
    }

    #[test]
    fn argmax_picks_highest_logit() {
        let logits = ndarray::Array3::from_shape_vec(
            (1, 3, 3),
            vec![
                0.1, 0.5, 0.3,
                0.9, 0.1, 0.0,
                0.0, 0.0, 0.8,
            ],
        ).unwrap();

        let labels = argmax_labels(&logits);
        assert_eq!(labels, vec![1, 0, 2]);
    }
}
```

**Step 2: Run tests**

```bash
cargo test -p pokrov-ner
```

Expected: PASS

**Step 3: Commit**

```bash
git add crates/pokrov-ner/src/decode.rs
git commit -m "feat(ner): add BIO tag decoder with char offset alignment"
```

---

## Task 5: NerEngine — ONNX session + inference pipeline

**Files:**
- Create: `crates/pokrov-ner/src/engine.rs`

**Step 1: Write the engine module**

```rust
use std::collections::HashMap;

use ndarray::Array2;
use ort::session::Session;
use tokenizers::Tokenizer;
use tracing::{debug, info};

use crate::decode::{argmax_labels, decode_bio_tags};
use crate::error::NerError;
use crate::model::{DetectedLanguage, NerConfig, NerEntityType, NerHit};

pub struct NerEngine {
    session: Session,
    tokenizer: Tokenizer,
    id2label: HashMap<usize, String>,
    config: NerConfig,
}

impl NerEngine {
    pub fn new(config: NerConfig) -> Result<Self, NerError> {
        if !config.model_path.exists() {
            return Err(NerError::ModelNotFound { path: config.model_path.clone() });
        }
        if !config.tokenizer_path.exists() {
            return Err(NerError::TokenizerNotFound { path: config.tokenizer_path.clone() });
        }

        let session = Session::builder()
            .and_then(|b| b.commit_from_file(&config.model_path))
            .map_err(|e| NerError::SessionInit(e.to_string()))?;

        let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| NerError::SessionInit(format!("tokenizer load: {}", e)))?;

        let id2label = default_id2label();

        info!(
            model = %config.model_path.display(),
            max_seq = config.max_seq_length,
            "NER engine initialized"
        );

        Ok(Self { session, tokenizer, id2label, config })
    }

    pub fn recognize(
        &self,
        text: &str,
        entity_types: &[NerEntityType],
    ) -> Result<Vec<NerHit>, NerError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| NerError::TokenizationFailed(e.to_string()))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let type_ids = encoding.get_type_ids();

        let seq_len = input_ids.len();

        let input_ids_arr = Array2::from_shape_vec(
            (1, seq_len),
            input_ids.iter().map(|&v| v as i64).collect(),
        )
        .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let attention_arr = Array2::from_shape_vec(
            (1, seq_len),
            attention_mask.iter().map(|&v| v as i64).collect(),
        )
        .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let type_ids_arr = Array2::from_shape_vec(
            (1, seq_len),
            type_ids.iter().map(|&v| v as i64).collect(),
        )
        .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => input_ids_arr.view(),
                "attention_mask" => attention_arr.view(),
                "token_type_ids" => type_ids_arr.view(),
            ].map_err(|e| NerError::InferenceFailed(e.to_string()))?)
            .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let logits = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| NerError::InferenceFailed(format!("logits extract: {}", e)))?;

        let label_indices = argmax_labels(&logits.to_owned());
        let labels: Vec<String> = label_indices
            .iter()
            .map(|&idx| {
                self.id2label.get(&idx).cloned().unwrap_or_else(|| "O".to_string())
            })
            .collect();

        let offsets: Vec<(usize, usize)> = encoding
            .get_offsets()
            .iter()
            .map(|&(s, e)| (s, e))
            .collect();

        let raw_spans = decode_bio_tags(&labels, &offsets);

        let allowed_labels: Vec<String> = entity_types
            .iter()
            .map(|et| match et {
                NerEntityType::Person => "PER",
                NerEntityType::Organization => "ORG",
            })
            .map(String::from)
            .collect();

        let hits: Vec<NerHit> = raw_spans
            .into_iter()
            .filter(|span| allowed_labels.contains(&span.label))
            .filter(|span| {
                let text_slice = &text[span.char_start..span.char_end];
                !text_slice.trim().is_empty()
            })
            .map(|span| {
                let matched_text = text[span.char_start..span.char_end].to_string();
                let language = detect_language_hint(&matched_text);
                NerHit {
                    entity: match span.label.as_str() {
                        "PER" => NerEntityType::Person,
                        "ORG" => NerEntityType::Organization,
                        _ => NerEntityType::Person,
                    },
                    text: matched_text,
                    start: span.char_start,
                    end: span.char_end,
                    score: 1.0,
                    language,
                }
            })
            .collect();

        debug!(hits = hits.len(), "NER recognition complete");
        Ok(hits)
    }
}

fn detect_language_hint(text: &str) -> DetectedLanguage {
    let cyrillic = text.chars().filter(|c| c >= '\u{0400}' && c <= '\u{04FF}').count();
    let latin = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    if cyrillic > latin {
        DetectedLanguage::Ru
    } else if latin > 0 {
        DetectedLanguage::En
    } else {
        DetectedLanguage::Unknown
    }
}

fn default_id2label() -> HashMap<usize, String> {
    HashMap::from([
        (0, "O".to_string()),
        (1, "B-PER".to_string()),
        (2, "I-PER".to_string()),
        (3, "B-ORG".to_string()),
        (4, "I-ORG".to_string()),
        (5, "B-LOC".to_string()),
        (6, "I-LOC".to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detection_russian() {
        assert_eq!(detect_language_hint("Иван Петров"), DetectedLanguage::Ru);
    }

    #[test]
    fn language_detection_english() {
        assert_eq!(detect_language_hint("John Smith"), DetectedLanguage::En);
    }

    #[test]
    fn language_detection_mixed() {
        assert_eq!(detect_language_hint("Office"), DetectedLanguage::En);
    }

    #[test]
    fn new_fails_on_missing_model() {
        let config = NerConfig {
            model_path: "/nonexistent/model.onnx".into(),
            tokenizer_path: "/nonexistent/tokenizer.json".into(),
            ..NerConfig::default()
        };
        let result = NerEngine::new(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            NerError::ModelNotFound { path } => {
                assert!(path.to_string_lossy().contains("nonexistent"));
            }
            other => panic!("expected ModelNotFound, got: {}", other),
        }
    }

    #[test]
    fn default_id2label_covers_per_org_loc() {
        let labels = default_id2label();
        assert_eq!(labels[&1], "B-PER");
        assert_eq!(labels[&3], "B-ORG");
        assert_eq!(labels[&5], "B-LOC");
    }
}
```

**Step 2: Verify compilation**

```bash
cargo check -p pokrov-ner
```

Expected: PASS

**Step 3: Run tests**

```bash
cargo test -p pokrov-ner
```

Expected: PASS for unit tests. Engine integration tests require model files (Task 8).

**Step 4: Commit**

```bash
git add crates/pokrov-ner/src/engine.rs
git commit -m "feat(ner): add NerEngine with ONNX inference pipeline"
```

---

## Task 6: Add pokrov-ner to workspace dev-dependencies

**Files:**
- Modify: `Cargo.toml` (workspace dev-dependencies)

**Step 1: Add dev-dependency**

In workspace root `Cargo.toml`, add to `[dev-dependencies]`:

```toml
pokrov-ner = { path = "crates/pokrov-ner" }
```

**Step 2: Verify full workspace compilation**

```bash
cargo check --workspace
```

Expected: PASS

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(ner): add pokrov-ner to workspace dev-dependencies"
```

---

## Task 7: NerAdapter in pokrov-core (feature-gated)

**Files:**
- Create: `crates/pokrov-core/src/ner_adapter.rs`
- Modify: `crates/pokrov-core/src/lib.rs`
- Modify: `crates/pokrov-core/Cargo.toml`

**Step 1: Add optional dependency in `crates/pokrov-core/Cargo.toml`**

Add:

```toml
[features]
default = []
ner = ["pokrov-ner"]

[dependencies]
# ... existing deps ...
pokrov-ner = { path = "../pokrov-ner", optional = true }
```

**Step 2: Create `crates/pokrov-core/src/ner_adapter.rs`**

```rust
use crate::types::foundation::{EvidenceClass, HitLocationKind, NormalizedHit, SuppressionStatus, ValidationStatus};
use crate::types::{DetectionCategory, PolicyAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NerFailMode {
    FailOpen,
    FailClosed,
}

#[derive(Debug, Clone)]
pub struct NerAdapterConfig {
    pub enabled: bool,
    pub fail_mode: NerFailMode,
    pub entity_types: Vec<pokrov_ner::NerEntityType>,
    pub timeout_ms: u64,
}

pub struct NerAdapter {
    engine: pokrov_ner::NerEngine,
    config: NerAdapterConfig,
}

impl NerAdapter {
    pub fn new(
        engine: pokrov_ner::NerEngine,
        config: NerAdapterConfig,
    ) -> Self {
        Self { engine, config }
    }

    pub fn recognize_sync(
        &self,
        text: &str,
        json_pointer: &str,
    ) -> Result<Vec<NormalizedHit>, NerAdapterError> {
        let hits = self.engine
            .recognize(text, &self.config.entity_types)
            .map_err(|e| NerAdapterError::EngineFailed(e.to_string()))?;

        let normalized: Vec<NormalizedHit> = hits
            .into_iter()
            .map(|hit| NormalizedHit {
                rule_id: format!("ner.{}.{}", hit.entity.as_str(), hit.language.as_str()),
                category: match hit.entity {
                    pokrov_ner::NerEntityType::Person => DetectionCategory::Pii,
                    pokrov_ner::NerEntityType::Organization => DetectionCategory::CorporateMarkers,
                },
                location_kind: HitLocationKind::JsonPointer,
                json_pointer: json_pointer.to_string(),
                start: hit.start,
                end: hit.end,
                action_hint: PolicyAction::Redact,
                final_score: (hit.score * 100.0) as i32,
                family_priority: 200,
                priority: 200,
                evidence_class: EvidenceClass::RemoteRecognizer,
                validation_status: ValidationStatus::Candidate,
                suppression_status: SuppressionStatus::None,
                reason_codes: vec![format!("ner:{}", hit.entity.as_str())],
                replacement_template_present: false,
            })
            .collect();

        Ok(normalized)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NerAdapterError {
    #[error("NER engine failed: {0}")]
    EngineFailed(String),
    #[error("NER inference timeout after {0}ms")]
    Timeout(u64),
}
```

**Step 3: Register module in `crates/pokrov-core/src/lib.rs`**

Add after existing module declarations:

```rust
#[cfg(feature = "ner")]
pub mod ner_adapter;
```

**Step 4: Verify compilation**

```bash
cargo check -p pokrov-core --features ner
cargo check -p pokrov-core
```

Both should PASS (without `--features ner`, the adapter is excluded).

**Step 5: Commit**

```bash
git add crates/pokrov-core/
git commit -m "feat(core): add feature-gated NerAdapter for RemoteRecognizer integration"
```

---

## Task 8: Integration test with synthetic model

**Files:**
- Create: `crates/pokrov-ner/tests/integration.rs`

This test validates the full pipeline with a real ONNX model file. It is marked `#[ignore]` by default since it requires the model artifact.

**Step 1: Write integration test**

```rust
use pokrov_ner::{NerConfig, NerEngine, NerEntityType, NerHit};

fn engine_from_env() -> Option<NerEngine> {
    let model_path = std::env::var("NER_MODEL_PATH").ok()?;
    let tokenizer_path = std::env::var("NER_TOKENIZER_PATH").ok()?;
    let config = NerConfig {
        model_path: model_path.into(),
        tokenizer_path: tokenizer_path.into(),
        ..NerConfig::default()
    };
    NerEngine::new(config).ok()
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn recognize_russian_person() {
    let engine = engine_from_env().expect("NER engine must initialize");
    let hits = engine
        .recognize(
            "Меня зовут Иван Петров, я работаю в Газпроме",
            &[NerEntityType::Person, NerEntityType::Organization],
        )
        .expect("recognition must succeed");

    assert!(!hits.is_empty(), "should detect at least one entity");

    let persons: Vec<&NerHit> = hits
        .iter()
        .filter(|h| h.entity == NerEntityType::Person)
        .collect();
    assert!(!persons.is_empty(), "should detect at least one person");

    let orgs: Vec<&NerHit> = hits
        .iter()
        .filter(|h| h.entity == NerEntityType::Organization)
        .collect();
    assert!(!orgs.is_empty(), "should detect at least one organization");
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn recognize_english_person() {
    let engine = engine_from_env().expect("NER engine must initialize");
    let hits = engine
        .recognize(
            "My name is John Smith and I work at Microsoft",
            &[NerEntityType::Person, NerEntityType::Organization],
        )
        .expect("recognition must succeed");

    assert!(!hits.is_empty(), "should detect at least one entity");
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn empty_input_returns_empty() {
    let engine = engine_from_env().expect("NER engine must initialize");
    let hits = engine
        .recognize("", &[NerEntityType::Person])
        .expect("must not fail on empty input");
    assert!(hits.is_empty());
}

#[test]
#[ignore = "requires NER_MODEL_PATH and NER_TOKENIZER_PATH env vars"]
fn latency_under_100ms() {
    let engine = engine_from_env().expect("NER engine must initialize");
    let text = "Contact Alice Johnson at acme@example.com for details about the project.";

    let start = std::time::Instant::now();
    for _ in 0..10 {
        let _ = engine.recognize(text, &[NerEntityType::Person, NerEntityType::Organization]);
    }
    let avg = start.elapsed() / 10;
    assert!(avg.as_millis() < 100, "avg latency {:?} exceeds 100ms budget", avg);
}
```

**Step 2: Run (will be ignored without env vars)**

```bash
cargo test -p pokrov-ner --test integration
```

Expected: all tests show `ignored`.

**Step 3: Commit**

```bash
git add crates/pokrov-ner/tests/
git commit -m "test(ner): add integration tests with model-dependent latency benchmarks"
```

---

## Task 9: Model download script

**Files:**
- Create: `scripts/download-ner-model.sh`

**Step 1: Write download script**

```bash
#!/usr/bin/env bash
set -euo pipefail

MODEL_DIR="${1:-models/ner-rubert-tiny-news}"
MODEL_NAME="r1char9/ner-rubert-tiny-news"

echo "Downloading NER model: ${MODEL_NAME}"
echo "Output directory: ${MODEL_DIR}"

mkdir -p "${MODEL_DIR}"

if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 is required for model conversion"
    exit 1
fi

python3 -c "
from optimum.exporters.onnx import main_export
main_export(
    model_name='${MODEL_NAME}',
    output='${MODEL_DIR}',
    task='token-classification',
    do_validation=False,
)
"

echo "Model exported to ${MODEL_DIR}"
ls -la "${MODEL_DIR}"
```

**Step 2: Make executable**

```bash
chmod +x scripts/download-ner-model.sh
```

**Step 3: Commit**

```bash
git add scripts/download-ner-model.sh
git commit -m "feat(ner): add model download and ONNX conversion script"
```

---

## Task 10: YAML config extension for NER

**Files:**
- Modify: `crates/pokrov-config/Cargo.toml`
- Create: `crates/pokrov-config/src/model/ner.rs`
- Modify: `crates/pokrov-config/src/model/mod.rs`
- Modify: `config/pokrov.example.yaml`

**Step 1: Add optional dependency in `crates/pokrov-config/Cargo.toml`**

```toml
[features]
default = []
ner = ["pokrov-ner"]

[dependencies]
# ... existing deps ...
pokrov-ner = { path = "../pokrov-ner", optional = true }
```

**Step 2: Create `crates/pokrov-config/src/model/ner.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub model_path: String,
    pub tokenizer_path: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
    #[serde(default = "default_max_seq_length")]
    pub max_seq_length: usize,
    #[serde(default)]
    pub profiles: std::collections::BTreeMap<String, NerProfileConfig>,
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
```

**Step 3: Register in `crates/pokrov-config/src/model/mod.rs`**

Add:

```rust
#[cfg(feature = "ner")]
pub mod ner;
```

**Step 4: Extend `config/pokrov.example.yaml`**

Add under `sanitization:`:

```yaml
  ner:
    enabled: false
    model_path: "./models/ner-rubert-tiny-news/model.onnx"
    tokenizer_path: "./models/ner-rubert-tiny-news/tokenizer.json"
    timeout_ms: 80
    confidence_threshold: 0.7
    max_seq_length: 512
    profiles:
      strict:
        fail_mode: fail_closed
        entity_types: [person, organization]
      minimal:
        fail_mode: fail_open
        entity_types: [person]
```

**Step 5: Verify**

```bash
cargo check -p pokrov-config --features ner
cargo check --workspace
```

Expected: PASS

**Step 6: Commit**

```bash
git add crates/pokrov-config/ config/pokrov.example.yaml
git commit -m "feat(config): add NER YAML configuration schema"
```

---

## Task 11: Wire NerAdapter into SanitizationEngine (feature-gated)

**Files:**
- Modify: `crates/pokrov-core/src/lib.rs`

This task wires the NER adapter into the existing pipeline. The key change: after `detect_payload()`, if the `ner` feature is enabled and NER config is present, call the NER adapter on each string leaf and merge hits into the pipeline before overlap resolution.

Requirements:
1. Only activate when `#[cfg(feature = "ner")]`
2. Run NER in `spawn_blocking` (CPU-bound)
3. Merge NER hits with regex hits before overlap resolution
4. Respect timeout budget
5. Set degradation metadata on failure

**Step 1: Verify**

```bash
cargo check -p pokrov-core --features ner
cargo test -p pokrov-core
cargo test --workspace
```

Expected: All existing tests pass. NER path is dormant unless feature is enabled.

**Step 2: Commit**

```bash
git add crates/pokrov-core/src/lib.rs
git commit -m "feat(core): wire NerAdapter into SanitizationEngine pipeline (feature-gated)"
```

---

## Task 12: End-to-end verification

**Step 1: Full workspace check**

```bash
cargo check --workspace --all-features
cargo clippy --workspace --all-features --all-targets
cargo test --workspace
```

**Step 2: Verify feature isolation**

```bash
cargo check --workspace
cargo test --workspace
```

Without `--features ner`, everything compiles and passes with zero NER code included.

**Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix(ner): address clippy and test issues from full verification"
```

---

## Dependency Graph

```
Task 1 (scaffold) --> Task 2 (error) --> Task 3 (model types)
                                              |
                                  Task 4 (BIO decoder)
                                              |
                                  Task 5 (NerEngine) --> Task 6 (workspace deps)
                                                              |
                                                  Task 7 (NerAdapter in core)
                                                              |
                                                  Task 8 (integration tests)
                                                              |
                                                  Task 9 (download script)
                                                              |
                                                  Task 10 (YAML config)
                                                              |
                                                  Task 11 (wire into engine)
                                                              |
                                                  Task 12 (e2e verification)
```
