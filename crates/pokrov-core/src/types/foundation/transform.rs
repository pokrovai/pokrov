use serde::{Deserialize, Serialize};

use crate::types::{EvaluateDecision, EvaluationMode, PolicyAction, ResolvedSpan, TransformResult};

/// Shared transform-planning contract that keeps policy and mutation responsibilities separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformPlan {
    pub final_action: PolicyAction,
    pub per_hit_operator_mapping: Vec<String>,
    pub transform_order: Vec<String>,
    pub mode: EvaluationMode,
}

impl TransformPlan {
    /// Builds the stable transform plan view from the current runtime decision.
    pub fn from_decision(
        mode: EvaluationMode,
        resolved_spans: &[ResolvedSpan],
        decision: &EvaluateDecision,
        mask_visible_suffix: u8,
    ) -> Self {
        let per_hit_operator_mapping = resolved_spans
            .iter()
            .map(|span| transform_operator_mapping(span, mask_visible_suffix))
            .collect::<Vec<_>>();

        let transform_order = if decision.final_action == PolicyAction::Block {
            vec!["policy_block".to_string()]
        } else if resolved_spans.is_empty() {
            vec!["pass_through".to_string()]
        } else {
            vec!["stable_span_order".to_string(), "json_string_leaf_mutation".to_string()]
        };

        Self {
            final_action: decision.final_action,
            per_hit_operator_mapping,
            transform_order,
            mode,
        }
    }
}

/// Captures only the transform metadata that may cross the proof boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundationTransformResult {
    pub final_action: PolicyAction,
    pub blocked: bool,
    pub transformed_fields_count: u32,
}

impl FoundationTransformResult {
    /// Drops sanitized payload content while preserving transform execution metadata.
    pub fn from_transform_result(result: &TransformResult) -> Self {
        Self {
            final_action: result.final_action,
            blocked: result.blocked,
            transformed_fields_count: result.transformed_fields_count,
        }
    }
}

fn transform_operator_mapping(span: &ResolvedSpan, mask_visible_suffix: u8) -> String {
    match span.effective_action {
        PolicyAction::Mask => {
            format!(
                "{}:{}:visible_suffix={}",
                span.winning_rule_id,
                action_key(span.effective_action),
                mask_visible_suffix
            )
        }
        PolicyAction::Replace => format!(
            "{}:{}:template={}",
            span.winning_rule_id,
            action_key(span.effective_action),
            // replacement_template is profile/admin configuration and never user payload content.
            span.replacement_template.as_deref().unwrap_or("[REPLACED]")
        ),
        _ => format!("{}:{}", span.winning_rule_id, action_key(span.effective_action)),
    }
}

pub(crate) fn action_key(action: PolicyAction) -> &'static str {
    match action {
        PolicyAction::Allow => "allow",
        PolicyAction::Mask => "mask",
        PolicyAction::Replace => "replace",
        PolicyAction::Redact => "redact",
        PolicyAction::Block => "block",
    }
}

#[cfg(test)]
mod tests {
    use super::{FoundationTransformResult, TransformPlan};
    use crate::types::{
        DetectionCategory, EvaluateDecision, EvaluationMode, PolicyAction, ResolvedSpan, TransformResult,
    };

    #[test]
    fn transform_plan_tracks_runtime_decision_without_recomputing_policy() {
        let resolved_spans = vec![ResolvedSpan {
            json_pointer: "/messages/0/content".to_string(),
            start: 4,
            end: 12,
            winning_rule_id: "custom.card".to_string(),
            category: DetectionCategory::Secrets,
            effective_action: PolicyAction::Redact,
            priority: 800,
            replacement_template: None,
            suppressed_rule_ids: vec!["custom.shadow".to_string()],
        }];
        let decision = EvaluateDecision {
            final_action: PolicyAction::Redact,
            rule_hits_total: 1,
            hits_by_category: std::collections::BTreeMap::from([("secrets".to_string(), 1)]),
            resolved_spans: Vec::new(),
            deterministic_signature: "sig".to_string(),
        };

        let plan = TransformPlan::from_decision(EvaluationMode::Enforce, &resolved_spans, &decision, 4);

        assert_eq!(plan.final_action, PolicyAction::Redact);
        assert_eq!(plan.transform_order[0], "stable_span_order");
        assert!(plan.per_hit_operator_mapping[0].contains("custom.card"));
    }

    #[test]
    fn transform_plan_respects_block_and_empty_hits_order() {
        let no_hits = Vec::new();
        let block_decision = EvaluateDecision {
            final_action: PolicyAction::Block,
            rule_hits_total: 1,
            hits_by_category: std::collections::BTreeMap::from([("secrets".to_string(), 1)]),
            resolved_spans: Vec::new(),
            deterministic_signature: "sig-block".to_string(),
        };
        let allow_decision = EvaluateDecision {
            final_action: PolicyAction::Allow,
            rule_hits_total: 0,
            hits_by_category: std::collections::BTreeMap::new(),
            resolved_spans: Vec::new(),
            deterministic_signature: "sig-allow".to_string(),
        };

        let block_plan = TransformPlan::from_decision(EvaluationMode::Enforce, &no_hits, &block_decision, 4);
        let allow_plan = TransformPlan::from_decision(EvaluationMode::Enforce, &no_hits, &allow_decision, 4);

        assert_eq!(block_plan.transform_order, vec!["policy_block".to_string()]);
        assert_eq!(allow_plan.transform_order, vec!["pass_through".to_string()]);
    }

    #[test]
    fn transform_plan_changes_when_replace_template_changes() {
        let replace_span = ResolvedSpan {
            json_pointer: "/message".to_string(),
            start: 11,
            end: 16,
            winning_rule_id: "custom.replace".to_string(),
            category: DetectionCategory::Custom,
            effective_action: PolicyAction::Replace,
            priority: 400,
            replacement_template: Some("[FIRST]".to_string()),
            suppressed_rule_ids: Vec::new(),
        };
        let replace_span_alt = ResolvedSpan {
            replacement_template: Some("[SECOND]".to_string()),
            ..replace_span.clone()
        };
        let resolved_spans = vec![replace_span];
        let decision = EvaluateDecision {
            final_action: PolicyAction::Replace,
            rule_hits_total: 1,
            hits_by_category: std::collections::BTreeMap::from([("custom".to_string(), 1)]),
            resolved_spans: Vec::new(),
            deterministic_signature: "sig".to_string(),
        };

        let replace_plan = TransformPlan::from_decision(EvaluationMode::Enforce, &resolved_spans, &decision, 4);
        let alternate_replace_spans = vec![replace_span_alt];
        let alternate_replace_plan =
            TransformPlan::from_decision(EvaluationMode::Enforce, &alternate_replace_spans, &decision, 4);

        assert_ne!(
            replace_plan, alternate_replace_plan,
            "replacement template differences must be observable in the exported plan"
        );
    }

    #[test]
    fn foundation_transform_result_drops_sanitized_payload() {
        let trace_result = FoundationTransformResult::from_transform_result(&TransformResult {
            final_action: PolicyAction::Redact,
            sanitized_payload: Some(serde_json::json!({"message": "secret"})),
            blocked: false,
            transformed_fields_count: 1,
        });

        let serialized =
            serde_json::to_string(&trace_result).expect("trace transform result must serialize");

        assert_eq!(trace_result.final_action, PolicyAction::Redact);
        assert_eq!(trace_result.transformed_fields_count, 1);
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("sanitized_payload"));
    }
}
