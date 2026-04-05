use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use crate::{
    policy::category_to_key,
    types::{
        AuditSummary, EvaluateDecision, EvaluateRequest, ExplainCategory, ExplainSummary, PolicyAction,
        ResolvedSpan,
    },
};

/// Builds the metadata-only explain summary from a completed evaluation.
pub fn build_explain_summary(
    profile_id: &str,
    mode: crate::types::EvaluationMode,
    decision: &EvaluateDecision,
    resolved_spans: &[ResolvedSpan],
) -> ExplainSummary {
    let mut actions_by_category: BTreeMap<String, PolicyAction> = BTreeMap::new();
    for span in resolved_spans {
        let key = category_to_key(span.category).to_string();
        actions_by_category
            .entry(key)
            .and_modify(|action| {
                if span.effective_action.strictness_rank() > action.strictness_rank() {
                    *action = span.effective_action;
                }
            })
            .or_insert(span.effective_action);
    }

    let categories = decision
        .hits_by_category
        .iter()
        .map(|(category, hits)| ExplainCategory {
            category: parse_category(category),
            hits: *hits,
            effective_action: actions_by_category
                .get(category)
                .copied()
                .unwrap_or(PolicyAction::Allow),
        })
        .collect::<Vec<_>>();

    ExplainSummary {
        profile_id: profile_id.to_string(),
        mode,
        final_action: decision.final_action,
        categories,
        rule_hits_total: decision.rule_hits_total,
        family_counts: foundation_family_counts(decision.rule_hits_total, resolved_spans.len() as u32),
        entity_counts: decision.hits_by_category.clone(),
        reason_codes: explain_reason_codes(resolved_spans),
        // Detector confidence is not captured in the resolved span contract.
        // Exporting precedence-derived buckets would misstate audit semantics.
        confidence_buckets: Vec::new(),
        provenance_summary: provenance_summary(resolved_spans),
        degradation_markers: Vec::new(),
    }
}

/// Builds the metadata-only audit summary from a completed evaluation.
pub fn build_audit_summary(
    request: &EvaluateRequest,
    effective_profile_id: &str,
    decision: &EvaluateDecision,
    resolved_spans: &[ResolvedSpan],
    duration: Duration,
) -> AuditSummary {
    AuditSummary {
        request_id: request.request_id.clone(),
        profile_id: effective_profile_id.to_string(),
        mode: request.mode,
        final_action: decision.final_action,
        rule_hits_total: decision.rule_hits_total,
        hits_by_category: decision.hits_by_category.clone(),
        family_counts: foundation_family_counts(decision.rule_hits_total, resolved_spans.len() as u32),
        duration_ms: duration.as_millis() as u64,
        path_class: request.path_class,
        degradation_metadata: Vec::new(),
    }
}

fn parse_category(value: &str) -> crate::types::DetectionCategory {
    match value {
        "secrets" => crate::types::DetectionCategory::Secrets,
        "pii" => crate::types::DetectionCategory::Pii,
        "corporate_markers" => crate::types::DetectionCategory::CorporateMarkers,
        _ => crate::types::DetectionCategory::Custom,
    }
}

fn foundation_family_counts(rule_hits_total: u32, resolved_hits_total: u32) -> BTreeMap<String, u32> {
    BTreeMap::from([
        ("normalized_hit".to_string(), rule_hits_total),
        ("resolved_hit".to_string(), resolved_hits_total),
    ])
}

fn explain_reason_codes(resolved_spans: &[ResolvedSpan]) -> Vec<String> {
    resolved_spans
        .iter()
        .map(|span| format!("{}:{:?}", category_to_key(span.category), span.effective_action))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn provenance_summary(resolved_spans: &[ResolvedSpan]) -> Vec<String> {
    resolved_spans
        .iter()
        .map(|span| {
            if span.winning_rule_id.starts_with("custom.") {
                "custom_rule".to_string()
            } else {
                "built_in_rule".to_string()
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::types::{
        DetectionCategory, EvaluateDecision, EvaluateRequest, EvaluationMode::*, PathClass,
        PolicyAction, ResolvedSpan, ResolvedSpanView,
    };

    use super::{build_audit_summary, build_explain_summary};

    #[test]
    fn explain_summary_contains_only_metadata() {
        let mut hits_by_category = BTreeMap::new();
        hits_by_category.insert("secrets".to_string(), 1);

        let decision = EvaluateDecision {
            final_action: PolicyAction::Block,
            rule_hits_total: 1,
            hits_by_category,
            resolved_spans: vec![ResolvedSpanView {
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Block,
                start: 10,
                end: 20,
            }],
            deterministic_signature: "abc".to_string(),
        };

        let explain = build_explain_summary(
            "strict",
            Enforce,
            &decision,
            &[ResolvedSpan {
                json_pointer: "/payload".to_string(),
                start: 10,
                end: 20,
                winning_rule_id: "rule".to_string(),
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Block,
                priority: 100,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            }],
        );

        assert_eq!(explain.rule_hits_total, 1);
        assert_eq!(explain.categories.len(), 1);
        assert_eq!(explain.categories[0].effective_action, PolicyAction::Block);
        assert_eq!(explain.family_counts.get("resolved_hit"), Some(&1));
        assert_eq!(explain.entity_counts.get("secrets"), Some(&1));
        assert!(explain.reason_codes.iter().any(|code| code.contains("secrets")));
    }

    #[test]
    fn explain_summary_does_not_infer_confidence_from_priority() {
        let decision = EvaluateDecision {
            final_action: PolicyAction::Redact,
            rule_hits_total: 1,
            hits_by_category: BTreeMap::from([("secrets".to_string(), 1)]),
            resolved_spans: vec![ResolvedSpanView {
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Redact,
                start: 10,
                end: 20,
            }],
            deterministic_signature: "sig".to_string(),
        };

        let explain = build_explain_summary(
            "strict",
            Enforce,
            &decision,
            &[ResolvedSpan {
                json_pointer: "/payload".to_string(),
                start: 10,
                end: 20,
                winning_rule_id: "custom.rule".to_string(),
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Redact,
                priority: 900,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            }],
        );

        assert!(
            explain.confidence_buckets.is_empty(),
            "priority is precedence metadata, not detector confidence"
        );
    }

    #[test]
    fn audit_summary_excludes_payload_fragments() {
        let request = EvaluateRequest {
            request_id: "req-1".to_string(),
            profile_id: String::new(),
            mode: DryRun,
            payload: serde_json::json!({"content": "secret"}),
            path_class: PathClass::Direct,
        };

        let decision = EvaluateDecision {
            final_action: PolicyAction::Redact,
            rule_hits_total: 1,
            hits_by_category: BTreeMap::from([("secrets".to_string(), 1)]),
            resolved_spans: Vec::new(),
            deterministic_signature: "sig".to_string(),
        };

        let audit = build_audit_summary(
            &request,
            "strict",
            &decision,
            &[ResolvedSpan {
                json_pointer: "/payload".to_string(),
                start: 10,
                end: 20,
                winning_rule_id: "custom.rule".to_string(),
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Redact,
                priority: 700,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            }],
            std::time::Duration::from_millis(7),
        );

        let serialized = serde_json::to_string(&audit).expect("audit must serialize");
        assert!(serialized.contains("\"request_id\":\"req-1\""));
        assert_eq!(audit.profile_id, "strict");
        assert!(!serialized.contains("\"content\":\"secret\""));
        assert_eq!(audit.family_counts.get("normalized_hit"), Some(&1));
        assert!(audit.degradation_metadata.is_empty());
    }
}
