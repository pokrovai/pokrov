use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use crate::{
    policy::category_to_key,
    types::{
        foundation::action_key, AuditSummary, DegradedSummary, EvaluateDecision, EvaluateRequest,
        ExecutedSummary, ExplainCategory, ExplainSummary, PolicyAction, ResolvedSpan,
    },
};

/// Builds the metadata-only explain summary from a completed evaluation.
pub fn build_explain_summary(
    profile_id: &str,
    mode: crate::types::EvaluationMode,
    decision: &EvaluateDecision,
    resolved_spans: &[ResolvedSpan],
    executed: &ExecutedSummary,
    degraded: &DegradedSummary,
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
        family_counts: decision.hits_by_family.clone(),
        entity_counts: decision.hits_by_category.clone(),
        reason_codes: {
            let mut reason_codes = decision.reason_codes.clone();
            reason_codes.extend(explain_reason_codes(resolved_spans));
            reason_codes.sort();
            reason_codes.dedup();
            reason_codes
        },
        // Detector confidence is not captured in the resolved span contract.
        // Exporting precedence-derived buckets would misstate audit semantics.
        confidence_buckets: Vec::new(),
        provenance_summary: provenance_summary(resolved_spans),
        degradation_markers: explain_degradation_markers(executed, degraded),
    }
}

/// Builds the metadata-only audit summary from a completed evaluation.
pub fn build_audit_summary(
    request: &EvaluateRequest,
    effective_profile_id: &str,
    decision: &EvaluateDecision,
    _resolved_spans: &[ResolvedSpan],
    executed: &ExecutedSummary,
    degraded: &DegradedSummary,
    duration: Duration,
) -> AuditSummary {
    AuditSummary {
        request_id: request.request_id.clone(),
        profile_id: effective_profile_id.to_string(),
        mode: request.mode,
        final_action: decision.final_action,
        rule_hits_total: decision.rule_hits_total,
        hits_by_category: decision.hits_by_category.clone(),
        counts_by_family: decision.hits_by_family.clone(),
        duration_ms: duration.as_millis() as u64,
        path_class: request.path_class,
        degradation_metadata: audit_degradation_metadata(executed, degraded),
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

fn explain_reason_codes(resolved_spans: &[ResolvedSpan]) -> Vec<String> {
    resolved_spans
        .iter()
        .map(|span| {
            format!("{}:{}", category_to_key(span.category), action_key(span.effective_action))
        })
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

fn explain_degradation_markers(
    executed: &ExecutedSummary,
    degraded: &DegradedSummary,
) -> Vec<String> {
    let mut markers = Vec::new();
    if !executed.execution_enabled {
        markers.push("execution_disabled".to_string());
    }
    if degraded.is_degraded {
        markers.push("degraded_execution".to_string());
    }
    if degraded.fail_closed_applied {
        markers.push("fail_closed_applied".to_string());
    }
    markers
}

fn audit_degradation_metadata(
    executed: &ExecutedSummary,
    degraded: &DegradedSummary,
) -> Vec<String> {
    let mut metadata = Vec::new();
    metadata.push(format!("execution_enabled={}", executed.execution_enabled));
    metadata.push(format!("is_degraded={}", degraded.is_degraded));
    if degraded.fail_closed_applied {
        metadata.push("fail_closed_applied=true".to_string());
    }
    metadata.extend(
        degraded
            .missing_execution_paths
            .iter()
            .map(|path| format!("missing_execution_path={path}")),
    );
    metadata
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::types::{
        DegradedSummary, DetectionCategory, EvaluateDecision, EvaluateRequest, EvaluationMode,
        ExecutedSummary, PathClass, PolicyAction, ResolvedSpan,
    };

    use super::{build_audit_summary, build_explain_summary};

    fn sample_decision() -> EvaluateDecision {
        EvaluateDecision {
            final_action: PolicyAction::Redact,
            rule_hits_total: 1,
            deterministic_candidates_total: 1,
            suppressed_candidates_total: 0,
            hits_by_category: BTreeMap::from([("secrets".to_string(), 1)]),
            hits_by_family: BTreeMap::from([("resolved_hit".to_string(), 1)]),
            reason_codes: vec!["winner:builtin.secret".to_string()],
            resolved_locations: Vec::new(),
            replay_identity: "sig".to_string(),
        }
    }

    fn sample_resolved_span() -> ResolvedSpan {
        ResolvedSpan {
            json_pointer: "/payload".to_string(),
            start: 10,
            end: 20,
            winning_rule_id: "builtin.secret".to_string(),
            category: DetectionCategory::Secrets,
            effective_action: PolicyAction::Redact,
            priority: 900,
            replacement_template: None,
            suppressed_rule_ids: Vec::new(),
        }
    }

    #[test]
    fn explain_summary_contains_execution_and_degraded_markers() {
        let explain = build_explain_summary(
            "strict",
            EvaluationMode::DryRun,
            &sample_decision(),
            &[sample_resolved_span()],
            &ExecutedSummary {
                execution_enabled: false,
                stages_completed: vec!["input_normalization".to_string()],
                recognizer_families_executed: Vec::new(),
                transform_applied: false,
            },
            &DegradedSummary {
                is_degraded: true,
                reasons: vec!["recognizer_timeout".to_string()],
                fail_closed_applied: true,
                missing_execution_paths: vec!["recognizer_execution".to_string()],
            },
        );

        assert!(explain.degradation_markers.contains(&"execution_disabled".to_string()));
        assert!(explain.degradation_markers.contains(&"degraded_execution".to_string()));
        assert!(explain.degradation_markers.contains(&"fail_closed_applied".to_string()));
    }

    #[test]
    fn audit_summary_stays_metadata_only() {
        let raw_fragment = "sk-test-rawsecret-123";
        let request = EvaluateRequest {
            request_id: "req-1".to_string(),
            profile_id: String::new(),
            mode: EvaluationMode::Enforce,
            payload: serde_json::json!({"content": raw_fragment}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        };
        let audit = build_audit_summary(
            &request,
            "strict",
            &sample_decision(),
            &[sample_resolved_span()],
            &ExecutedSummary {
                execution_enabled: true,
                stages_completed: vec!["policy_resolution".to_string()],
                recognizer_families_executed: vec!["builtin".to_string()],
                transform_applied: true,
            },
            &DegradedSummary {
                is_degraded: false,
                reasons: Vec::new(),
                fail_closed_applied: false,
                missing_execution_paths: Vec::new(),
            },
            std::time::Duration::from_millis(7),
        );

        let serialized = serde_json::to_string(&audit).expect("audit should serialize");
        assert_eq!(audit.request_id, "req-1");
        assert_eq!(audit.profile_id, "strict");
        assert_eq!(audit.counts_by_family.get("resolved_hit"), Some(&1));
        assert!(!serialized.contains(raw_fragment));
    }
}
