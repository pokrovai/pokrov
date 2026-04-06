use std::sync::Mutex;

use crate::types::foundation::{
    EvidenceClass, HitLocationKind, NormalizedHit, SuppressionStatus, ValidationStatus,
};
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

impl std::fmt::Debug for NerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NerAdapter").field("config", &self.config).finish_non_exhaustive()
    }
}

pub struct NerAdapter {
    engine: Mutex<pokrov_ner::NerEngine>,
    config: NerAdapterConfig,
}

impl NerAdapter {
    pub fn new(engine: pokrov_ner::NerEngine, config: NerAdapterConfig) -> Self {
        Self { engine: Mutex::new(engine), config }
    }

    pub fn recognize_sync(
        &self,
        text: &str,
        json_pointer: &str,
    ) -> Result<Vec<NormalizedHit>, NerAdapterError> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| NerAdapterError::EngineFailed("engine lock poisoned".to_string()))?;

        let hits = engine
            .recognize(text, &self.config.entity_types)
            .map_err(|e| NerAdapterError::EngineFailed(e.to_string()))?;

        let normalized: Vec<NormalizedHit> = hits
            .into_iter()
            .map(|hit| NormalizedHit {
                rule_id: format!("ner.{}.{}", hit.entity.as_str(), hit.language),
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
