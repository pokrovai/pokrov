use serde::{Deserialize, Serialize};

use crate::types::{DetectionCategory, DetectionHit, PolicyAction, ResolvedSpan};

/// Describes how a hit location is represented without carrying matched text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitLocationKind {
    JsonPointer,
}

/// Captures the detector provenance class without exposing source fragments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    BuiltInRule,
    DeterministicRule,
    CustomRule,
    RemoteRecognizer,
}

/// Marks whether a hit is still a candidate or already resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Candidate,
    Resolved,
    Rejected,
}

/// Marks suppression stage status for one deterministic candidate or resolved hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionStatus {
    None,
    SuppressedAllowlist,
    SuppressedNegativeContext,
}

/// Shared candidate hit shape for native and remote recognizer outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedHit {
    pub rule_id: String,
    pub category: DetectionCategory,
    pub location_kind: HitLocationKind,
    pub json_pointer: String,
    pub start: usize,
    pub end: usize,
    pub action_hint: PolicyAction,
    pub final_score: i32,
    pub family_priority: u16,
    pub priority: u16,
    pub evidence_class: EvidenceClass,
    pub validation_status: ValidationStatus,
    pub suppression_status: SuppressionStatus,
    pub reason_codes: Vec<String>,
    pub replacement_template_present: bool,
}

impl NormalizedHit {
    /// Converts the current runtime detection hit into the frozen shared hit family.
    pub fn from_detection_hit(hit: &DetectionHit) -> Self {
        Self {
            rule_id: hit.rule_id.clone(),
            category: hit.category,
            location_kind: HitLocationKind::JsonPointer,
            json_pointer: hit.json_pointer.clone(),
            start: hit.start,
            end: hit.end,
            action_hint: hit.action,
            final_score: i32::from(hit.priority),
            family_priority: hit.priority,
            priority: hit.priority,
            evidence_class: evidence_class_from_rule_id(&hit.rule_id),
            validation_status: ValidationStatus::Candidate,
            suppression_status: SuppressionStatus::None,
            reason_codes: Vec::new(),
            replacement_template_present: hit.replacement_template.is_some(),
        }
    }
}

/// Shared resolved-hit shape consumed by policy, explain, and transform planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedHit {
    pub winning_rule_id: String,
    pub category: DetectionCategory,
    pub json_pointer: String,
    pub start: usize,
    pub end: usize,
    pub effective_action_hint: PolicyAction,
    pub effective_score: i32,
    pub family_priority: u16,
    pub suppressed_rule_ids: Vec<String>,
    pub precedence_trace: Vec<String>,
    pub validation_status: ValidationStatus,
    pub suppression_status: SuppressionStatus,
    pub reason_codes: Vec<String>,
}

impl ResolvedHit {
    /// Converts a resolved runtime span into the shared resolved-hit family.
    pub fn from_resolved_span(span: &ResolvedSpan) -> Self {
        let mut precedence_trace =
            vec![format!("priority={}", span.priority), format!("winner={}", span.winning_rule_id)];
        if !span.suppressed_rule_ids.is_empty() {
            precedence_trace.push(format!("suppressed={}", span.suppressed_rule_ids.join(",")));
        }

        Self {
            winning_rule_id: span.winning_rule_id.clone(),
            category: span.category,
            json_pointer: span.json_pointer.clone(),
            start: span.start,
            end: span.end,
            effective_action_hint: span.effective_action,
            effective_score: i32::from(span.priority),
            family_priority: span.priority,
            suppressed_rule_ids: span.suppressed_rule_ids.clone(),
            precedence_trace,
            validation_status: ValidationStatus::Resolved,
            suppression_status: SuppressionStatus::None,
            reason_codes: vec![format!("winner:{}", span.winning_rule_id)],
        }
    }
}

fn evidence_class_from_rule_id(rule_id: &str) -> EvidenceClass {
    if rule_id.starts_with("deterministic.") {
        EvidenceClass::DeterministicRule
    } else if rule_id.starts_with("custom.") {
        EvidenceClass::CustomRule
    } else {
        // Remote recognizer provenance is reserved for v1 extension-point contracts.
        // Current runtime detectors emit only built-in and custom rule identifiers.
        EvidenceClass::BuiltInRule
    }
}

#[cfg(test)]
mod tests {
    use super::{NormalizedHit, ResolvedHit, SuppressionStatus, ValidationStatus};
    use crate::types::{DetectionCategory, DetectionHit, PolicyAction, ResolvedSpan};

    #[test]
    fn normalized_and_resolved_hits_stay_metadata_only() {
        let normalized = NormalizedHit::from_detection_hit(&DetectionHit {
            rule_id: "custom.card".to_string(),
            category: DetectionCategory::Secrets,
            json_pointer: "/messages/0/content".to_string(),
            start: 4,
            end: 12,
            action: PolicyAction::Redact,
            priority: 800,
            replacement_template: Some("[REDACTED]".to_string()),
        });
        let resolved = ResolvedHit::from_resolved_span(&ResolvedSpan {
            json_pointer: "/messages/0/content".to_string(),
            start: 4,
            end: 12,
            winning_rule_id: "custom.card".to_string(),
            category: DetectionCategory::Secrets,
            effective_action: PolicyAction::Redact,
            priority: 800,
            replacement_template: None,
            suppressed_rule_ids: vec!["custom.shadow".to_string()],
        });

        let normalized_json =
            serde_json::to_string(&normalized).expect("normalized hit must serialize");
        let resolved_json = serde_json::to_string(&resolved).expect("resolved hit must serialize");

        assert!(!normalized_json.contains("4111"));
        assert!(!resolved_json.contains("4111"));
        assert_eq!(resolved.validation_status, ValidationStatus::Resolved);
        assert_eq!(resolved.suppression_status, SuppressionStatus::None);
    }

    #[test]
    fn hit_evidence_tracks_custom_and_builtin_provenance() {
        let custom = NormalizedHit::from_detection_hit(&DetectionHit {
            rule_id: "custom.project".to_string(),
            category: DetectionCategory::Custom,
            json_pointer: "/messages/0/content".to_string(),
            start: 0,
            end: 5,
            action: PolicyAction::Redact,
            priority: 10,
            replacement_template: None,
        });
        let builtin = NormalizedHit::from_detection_hit(&DetectionHit {
            rule_id: "builtin.secret".to_string(),
            category: DetectionCategory::Secrets,
            json_pointer: "/messages/0/content".to_string(),
            start: 6,
            end: 11,
            action: PolicyAction::Mask,
            priority: 10,
            replacement_template: None,
        });

        assert_eq!(custom.evidence_class, super::EvidenceClass::CustomRule);
        assert_eq!(builtin.evidence_class, super::EvidenceClass::BuiltInRule);
    }

    #[test]
    fn hit_evidence_tracks_deterministic_provenance() {
        let deterministic = NormalizedHit::from_detection_hit(&DetectionHit {
            rule_id: "deterministic.payment_card.pattern.pan".to_string(),
            category: DetectionCategory::Secrets,
            json_pointer: "/messages/0/content".to_string(),
            start: 0,
            end: 8,
            action: PolicyAction::Block,
            priority: 100,
            replacement_template: None,
        });

        assert_eq!(deterministic.evidence_class, super::EvidenceClass::DeterministicRule);
    }
}
