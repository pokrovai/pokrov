# Lightweight NER/NLP Module Design: pokrov-ner

**Date:** 2026-04-06
**Status:** Approved
**Scope:** Post-v1 (v1.1 backlog item)
**Entities:** Person names (EN+RU), Organization names (EN+RU)

## Context

Pokrov's detection engine is regex-first: 22 builtin rules, deterministic recognizers, context scoring, field-gating, and Luhn validation. This covers structured patterns (emails, phones, credit cards, tokens, addresses) well but cannot detect arbitrary person or organization names in free-form text.

Four entities are explicitly deferred in the entity pack:
- `ml_person_name_parity` — broad person-name detection requires ML NER
- `ml_location_and_organization_ner` — location and organization families require heavy NLP
- `medical_and_phi_ontology` — medical PHI outside native scope
- `global_national_identifier_long_tail` — long-tail national IDs

This design covers `ml_person_name_parity` (PER) and `ml_location_and_organization_ner` (ORG only, not LOC) for EN+RU.

## Decision Summary

| Decision | Choice | Rationale |
|---|---|---|
| **Location** | Separate workspace crate `pokrov-ner` | Clean boundary, ML deps isolated from core |
| **Inference runtime** | `ort` (ONNX Runtime) via `load-dynamic` | Most mature Rust ML inference, proven in production |
| **Model** | `r1char9/ner-rubert-tiny-news` (~45 MB ONNX) | RU-first with partial EN, PER+ORG+LOC, F1=0.96 on RU |
| **Entity types** | Person, Organization | Covers the two highest-priority deferred entities |
| **Languages** | EN + RU simultaneously | Matches phase-one entity pack scope |
| **Latency budget** | Parallel, <100ms P95 | Does not consume the 50ms main pipeline budget |
| **Fail behavior** | Configurable per profile: fail-open or fail-closed | Strict profile blocks on NER failure, minimal profile continues |
| **Integration** | RemoteRecognizer extension point (spec 09) | Existing frozen pipeline contract, NormalizedHit model |

## Architecture

### Crate Structure

```
crates/pokrov-ner/
├── Cargo.toml
└── src/
    ├── lib.rs        # Public API: NerEngine, NerConfig, NerHit
    ├── engine.rs     # ONNX session management, inference orchestration
    ├── tokenizer.rs  # HuggingFace tokenizers integration (WordPiece)
    ├── decode.rs     # BIO/IOBES tag decoding → character-aligned spans
    ├── model.rs      # Model metadata, entity label mapping, id2label
    └── error.rs      # NerError enum
```

**Dependency direction:** `pokrov-ner` has zero dependency on `pokrov-core`. Integration flows one way: `pokrov-core` optionally depends on `pokrov-ner` via feature flag.

### Dependencies

```toml
[dependencies]
ort = { version = "2.0", features = ["load-dynamic"] }
tokenizers = "0.21"
ndarray = "0.16"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tracing = "0.1"
```

Binary size impact: ~0 MB added to compiled binary (ONNX Runtime loaded dynamically at startup). Model files distributed separately (~45 MB).

### Public API

```rust
pub struct NerEngine { /* ONNX session, tokenizer, label map */ }

pub struct NerConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub timeout_ms: u64,           // default: 80
    pub max_seq_length: usize,     // default: 512
    pub confidence_threshold: f32, // default: 0.7
}

pub enum NerEntityType { Person, Organization }
pub enum DetectedLanguage { En, Ru, Unknown }

pub struct NerHit {
    pub entity: NerEntityType,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub score: f32,
    pub language: DetectedLanguage,
}

impl NerEngine {
    pub fn new(config: NerConfig) -> Result<Self, NerError>;
    pub fn recognize(&self, text: &str, entity_types: &[NerEntityType]) -> Result<Vec<NerHit>, NerError>;
}
```

## Inference Pipeline

```
Input text
    │
    ▼
[1. Tokenize]           HuggingFace tokenizers (WordPiece, max_seq_length=512)
    │                   → input_ids, attention_mask, token_type_ids
    ▼
[2. ONNX Inference]     ort::Session::run()
    │                   → logits [batch, seq_len, num_labels]
    ▼
[3. Softmax + Argmax]   Per-token label prediction
    │                   → [O, B-PER, I-PER, B-ORG, I-ORG, O, ...]
    ▼
[4. BIO Decode]         Group B-/I- tokens into entity spans
    │                   → [(entity_type, start, end, avg_score)]
    ▼
[5. Score Filter]       Filter by confidence_threshold
    │                   → Only hits above threshold
    ▼
[6. Char Alignment]     Map token spans → original text offsets
    │                   → NerHit { text, start, end, score }
    ▼
Output: Vec<NerHit>
```

### Timeout Handling

- `NerConfig::timeout_ms` (default: 80ms) wraps ONNX inference via `tokio::time::timeout`
- Tokenizer + decode are deterministic (<5ms combined); timeout applies only to ONNX inference
- On timeout → `NerError::Timeout` → adapter decides fail-open or fail-closed

### Parallel Execution

NER runs **concurrently** with regex recognizers in Stage 2 (RecognizerExecution):

```
Stage 2: RecognizerExecution
    ├── NativeRecognizer (regex, context, validators)  ← synchronous, <10ms
    └── NerAdapter (via RemoteRecognizer extension)    ← async, <100ms
            runs concurrently via tokio::spawn_blocking
```

Results merge in Stage 3 (AnalysisAndSuppression) through standard overlap resolution. No special precedence for NER vs native hits — overlap resolution is profile-driven.

## Integration with pokrov-core

### RemoteRecognizer Adapter

The adapter lives in `pokrov-core` behind feature flag `#[cfg(feature = "ner")]`:

```rust
pub struct NerAdapter {
    engine: pokrov_ner::NerEngine,
    config: NerAdapterConfig,
}

pub struct NerAdapterConfig {
    pub enabled: bool,
    pub fail_mode: NerFailMode,              // FailOpen | FailClosed
    pub entity_types: Vec<NerEntityType>,    // [Person, Organization]
    pub timeout_ms: u64,
}

impl NerAdapter {
    pub async fn recognize(
        &self,
        text: &str,
        context: &RequestContext,
    ) -> Result<Vec<NormalizedHit>, DegradationState> {
        // Wrap engine.recognize() in spawn_blocking + timeout
        // Map NerError → DegradationState based on fail_mode
        // Emit degradation metadata on failure
    }
}
```

Feature gate in Cargo.toml:
```toml
[features]
default = []
ner = ["pokrov-ner"]
```

### Normalization into NormalizedHit

```rust
impl NerHit {
    fn into_normalized_hit(self) -> NormalizedHit {
        NormalizedHit {
            rule_id: format!("ner.{}.{}", self.entity.as_str(), self.language.as_str()),
            category: match self.entity {
                NerEntityType::Person => DetectionCategory::Pii,
                NerEntityType::Organization => DetectionCategory::CorporateMarkers,
            },
            score: self.score,
            start: self.start,
            end: self.end,
            matched_text: self.text.clone(),
            evidence_class: EvidenceClass::RemoteRecognizer,
            json_pointer: None,
        }
    }
}
```

### YAML Configuration

```yaml
sanitization:
  enabled: true
  default_profile: strict

  ner:
    enabled: true
    model_path: "./models/ner-rubert-tiny-news.onnx"
    tokenizer_path: "./models/ner-tokenizer.json"
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

### Audit Semantics

NER hits appear in audit as metadata-only:
- `rule_id`: `ner.person.ru`, `ner.organization.en`
- `category`: PII or CorporateMarkers
- `evidence_class`: RemoteRecognizer
- Hit counts, entity type distribution, confidence score ranges
- **Zero raw text** in audit — matched text exists only in `NormalizedHit` for policy evaluation, stripped before audit serialization

Degradation events include:
- `ner_timeout` — inference exceeded configured timeout
- `ner_error` — ONNX session or inference failure
- `fail_closed_applied` — request blocked due to NER degradation in fail-closed profile

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum NerError {
    #[error("ONNX session creation failed: {0}")]
    SessionInit(String),

    #[error("Model file not found: {path}")]
    ModelNotFound { path: PathBuf },

    #[error("Tokenizer file not found: {path}")]
    TokenizerNotFound { path: PathBuf },

    #[error("Inference timeout after {ms}ms")]
    Timeout { ms: u64 },

    #[error("ONNX inference failed: {0}")]
    InferenceFailed(String),

    #[error("Tokenization failed: {0}")]
    TokenizationFailed(String),
}
```

All errors propagate to the adapter, which maps them to `DegradationState` based on profile fail-mode.

## Testing Strategy

| Level | Scope | Method |
|---|---|---|
| Unit: tokenizer | WordPiece encoding, special tokens, overflow | `#[test]` with fixture strings |
| Unit: BIO decode | B/I/O → span conversion, edge cases | Table-driven tests |
| Unit: score filter | Threshold filtering, boundaries | Property testing |
| Integration: engine | Full pipeline: text → NerHit | Known-example fixtures |
| Integration: adapter | NerAdapter + DegradationState + fail modes | Mock NerEngine |
| Dataset: EN | Standard NER benchmarks (CoNLL-2003 subset) | F1/Precision/Recall |
| Dataset: RU | Russian NER corpus (MSU Wiki NER subset) | F1/Precision/Recall |
| Performance | Latency P95 < 100ms for typical agent prompts | Criterion benchmarks |
| Contract | NormalizedHit shape, EvidenceClass, audit metadata | Against frozen pipeline contracts |

## Deployment

### Model Distribution

ONNX model file + tokenizer.json distributed as release artifacts (not in git repo). Development script:

```bash
# One-time model conversion
python -m optimum.exporters.onnx \
  --model r1char9/ner-rubert-tiny-news \
  --task token-classification \
  --output_dir models/ner-rubert-tiny-news/

# Development setup
scripts/download-ner-model.sh
```

### Startup

- `NerEngine::new()` called once during bootstrap
- ONNX session pre-warm: ~200-500ms one-time cost
- Tokenizer load: ~50ms
- Health check: `/health/ner` endpoint verifies model loaded and inference succeeds on test string

### Runtime

- `spawn_blocking` for inference (CPU-bound work)
- ONNX Runtime manages its own thread pool internally
- Session reuse across requests (no per-request initialization)

## Alternatives Considered

### B: Pure Rust (candle/tract)
- Zero C++ dependencies
- Higher development effort (implement token classification pipeline manually)
- Potentially slower inference (pure Rust kernels vs ONNX Runtime's optimized C++)
- Risk of unsupported ONNX ops for complex transformer architectures

### C: Lightweight NLP without ML (regex + gazetteers + heuristics)
- Negligible latency (<1ms) and binary size (+0.5 MB)
- Good for known-entity lookup but very low recall on unseen entities
- Russian language support poor (no capitalization-based NER)
- Fails the core requirement: "arbitrary text" detection

## Open Questions for Implementation Phase

1. **EN quality validation:** Test `r1char9/ner-rubert-tiny-news` on English NER benchmarks. If F1 < 0.80, consider adding a second model for EN or fine-tuning rubert-tiny2 on combined EN+RU corpus.
2. **Model quantization:** Evaluate INT8 quantization for further latency reduction (trade-off: accuracy vs speed).
3. **Batch inference:** Support batching multiple text segments in a single ONNX call for throughput optimization.
4. **Language detection:** Whether to integrate explicit language detection or rely on model's multilingual capabilities.
5. **Model update mechanism:** How to deploy updated models without service restart (future consideration).
