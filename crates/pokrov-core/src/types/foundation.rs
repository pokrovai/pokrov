use serde::{Deserialize, Serialize};

use super::{
    AuditSummary, DetectionCategory, DetectionHit, EvaluateDecision, EvaluationMode, ExplainSummary,
    PathClass, PolicyAction, ResolvedSpan, TransformResult,
};

/// Identifies the frozen stage sequence for the sanitization pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageId {
    InputNormalization,
    RecognizerExecution,
    AnalysisAndSuppression,
    PolicyResolution,
    Transformation,
    SafeExplain,
    AuditSummary,
}

/// Names the approved data artifacts that may cross stage boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageArtifact {
    RawPayload,
    RequestContext,
    TraversalPayload,
    NormalizedHit,
    ResolvedHit,
    TransformPlan,
    TransformResult,
    ExplainSummary,
    AuditSummary,
}

/// Freezes the ownership and dependency rules for one pipeline stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineStageBoundary {
    pub stage_id: StageId,
    pub allowed_inputs: Vec<StageArtifact>,
    pub allowed_outputs: Vec<StageArtifact>,
    pub owns_policy_decision: bool,
    pub may_mutate_payload: bool,
    pub forbidden_responsibilities: Vec<String>,
}

/// Identifies extension points that may plug into the shared foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionPointKind {
    NativeRecognizer,
    RemoteRecognizer,
    StructuredProcessor,
    EvaluationRunner,
    BaselineRunner,
}

/// Encodes the bounded inputs and outputs for a supported extension point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPointContract {
    pub kind: ExtensionPointKind,
    pub accepted_inputs: Vec<StageArtifact>,
    pub produced_outputs: Vec<StageArtifact>,
    pub policy_ownership_allowed: bool,
    pub payload_mutation_allowed: bool,
}

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
    CustomRule,
    RemoteRecognizer,
}

/// Marks whether a hit is still a candidate or already resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Candidate,
    Resolved,
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
    pub priority: u16,
    pub evidence_class: EvidenceClass,
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
            priority: hit.priority,
            evidence_class: evidence_class_from_rule_id(&hit.rule_id),
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
    pub suppressed_rule_ids: Vec<String>,
    pub precedence_trace: Vec<String>,
    pub validation_status: ValidationStatus,
}

impl ResolvedHit {
    /// Converts a resolved runtime span into the shared resolved-hit family.
    pub fn from_resolved_span(span: &ResolvedSpan) -> Self {
        let mut precedence_trace =
            vec![format!("priority={}", span.priority), format!("winner={}", span.winning_rule_id)];
        if !span.suppressed_rule_ids.is_empty() {
            precedence_trace.push(format!(
                "suppressed={}",
                span.suppressed_rule_ids.join(",")
            ));
        }

        Self {
            winning_rule_id: span.winning_rule_id.clone(),
            category: span.category,
            json_pointer: span.json_pointer.clone(),
            start: span.start,
            end: span.end,
            effective_action_hint: span.effective_action,
            suppressed_rule_ids: span.suppressed_rule_ids.clone(),
            precedence_trace,
            validation_status: ValidationStatus::Resolved,
        }
    }
}

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

/// Distinguishes repo-safe fixtures from restricted external evaluation references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationArtifactClass {
    RepoSafeFixture,
    RestrictedExternalReference,
}

/// Freezes the repository-placement rules for evaluation artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationArtifactBoundary {
    pub artifact_class: EvaluationArtifactClass,
    pub commit_allowed: bool,
    pub access_metadata_required: bool,
    pub redistribution_allowed: bool,
}

/// Collects the shared contract families used by runtime and evaluation proofs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoundationExecutionTrace {
    pub request_id: String,
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub path_class: PathClass,
    pub stage_boundaries: Vec<PipelineStageBoundary>,
    pub extension_points: Vec<ExtensionPointContract>,
    pub normalized_hits: Vec<NormalizedHit>,
    pub resolved_hits: Vec<ResolvedHit>,
    pub transform_plan: TransformPlan,
    pub transform_result: FoundationTransformResult,
    pub explain: ExplainSummary,
    pub audit: AuditSummary,
    pub evaluation_boundaries: Vec<EvaluationArtifactBoundary>,
    pub executed: bool,
}

/// Returns the frozen stage-boundary definitions for downstream consumers.
pub fn foundation_stage_boundaries() -> Vec<PipelineStageBoundary> {
    vec![
        PipelineStageBoundary {
            stage_id: StageId::InputNormalization,
            allowed_inputs: vec![StageArtifact::RawPayload, StageArtifact::RequestContext],
            allowed_outputs: vec![StageArtifact::TraversalPayload, StageArtifact::RequestContext],
            owns_policy_decision: false,
            may_mutate_payload: false,
            forbidden_responsibilities: vec![
                "must not mutate payloads".to_string(),
                "must not choose policy actions".to_string(),
            ],
        },
        PipelineStageBoundary {
            stage_id: StageId::RecognizerExecution,
            allowed_inputs: vec![StageArtifact::TraversalPayload, StageArtifact::RequestContext],
            allowed_outputs: vec![StageArtifact::NormalizedHit],
            owns_policy_decision: false,
            may_mutate_payload: false,
            forbidden_responsibilities: vec![
                "must not resolve overlaps".to_string(),
                "must not emit audit payloads".to_string(),
            ],
        },
        PipelineStageBoundary {
            stage_id: StageId::AnalysisAndSuppression,
            allowed_inputs: vec![StageArtifact::NormalizedHit],
            allowed_outputs: vec![StageArtifact::ResolvedHit],
            owns_policy_decision: false,
            may_mutate_payload: false,
            forbidden_responsibilities: vec![
                "must not mutate payloads".to_string(),
                "must not own final policy action".to_string(),
            ],
        },
        PipelineStageBoundary {
            stage_id: StageId::PolicyResolution,
            allowed_inputs: vec![StageArtifact::ResolvedHit, StageArtifact::RequestContext],
            allowed_outputs: vec![StageArtifact::TransformPlan],
            owns_policy_decision: true,
            may_mutate_payload: false,
            forbidden_responsibilities: vec!["must not mutate payloads directly".to_string()],
        },
        PipelineStageBoundary {
            stage_id: StageId::Transformation,
            allowed_inputs: vec![StageArtifact::TransformPlan, StageArtifact::RawPayload],
            allowed_outputs: vec![StageArtifact::TransformResult],
            owns_policy_decision: false,
            may_mutate_payload: true,
            forbidden_responsibilities: vec![
                "must not re-run recognition".to_string(),
                "must not re-compute policy".to_string(),
            ],
        },
        PipelineStageBoundary {
            stage_id: StageId::SafeExplain,
            allowed_inputs: vec![StageArtifact::ResolvedHit, StageArtifact::TransformPlan],
            allowed_outputs: vec![StageArtifact::ExplainSummary],
            owns_policy_decision: false,
            may_mutate_payload: false,
            forbidden_responsibilities: vec![
                "must not include raw payload fragments".to_string(),
                "must not mutate payloads".to_string(),
            ],
        },
        PipelineStageBoundary {
            stage_id: StageId::AuditSummary,
            allowed_inputs: vec![
                StageArtifact::RequestContext,
                StageArtifact::TransformPlan,
                StageArtifact::TransformResult,
            ],
            allowed_outputs: vec![StageArtifact::AuditSummary],
            owns_policy_decision: false,
            may_mutate_payload: false,
            forbidden_responsibilities: vec![
                "must not include raw payload fragments".to_string(),
                "must not mutate payloads".to_string(),
            ],
        },
    ]
}

/// Returns the supported extension-point contracts for later workstreams.
pub fn foundation_extension_points() -> Vec<ExtensionPointContract> {
    vec![
        ExtensionPointContract {
            kind: ExtensionPointKind::NativeRecognizer,
            accepted_inputs: vec![StageArtifact::TraversalPayload, StageArtifact::RequestContext],
            produced_outputs: vec![StageArtifact::NormalizedHit],
            policy_ownership_allowed: false,
            payload_mutation_allowed: false,
        },
        ExtensionPointContract {
            kind: ExtensionPointKind::RemoteRecognizer,
            accepted_inputs: vec![StageArtifact::TraversalPayload, StageArtifact::RequestContext],
            produced_outputs: vec![StageArtifact::NormalizedHit],
            policy_ownership_allowed: false,
            payload_mutation_allowed: false,
        },
        ExtensionPointContract {
            kind: ExtensionPointKind::StructuredProcessor,
            accepted_inputs: vec![StageArtifact::TraversalPayload],
            produced_outputs: vec![StageArtifact::TraversalPayload],
            policy_ownership_allowed: false,
            payload_mutation_allowed: false,
        },
        ExtensionPointContract {
            kind: ExtensionPointKind::EvaluationRunner,
            accepted_inputs: vec![
                StageArtifact::NormalizedHit,
                StageArtifact::ResolvedHit,
                StageArtifact::TransformResult,
                StageArtifact::ExplainSummary,
                StageArtifact::AuditSummary,
            ],
            produced_outputs: vec![StageArtifact::AuditSummary],
            policy_ownership_allowed: false,
            payload_mutation_allowed: false,
        },
        ExtensionPointContract {
            kind: ExtensionPointKind::BaselineRunner,
            accepted_inputs: vec![
                StageArtifact::NormalizedHit,
                StageArtifact::ResolvedHit,
                StageArtifact::TransformResult,
                StageArtifact::ExplainSummary,
                StageArtifact::AuditSummary,
            ],
            produced_outputs: vec![StageArtifact::AuditSummary],
            policy_ownership_allowed: false,
            payload_mutation_allowed: false,
        },
    ]
}

/// Returns the frozen repository-placement boundaries for evaluation artifacts.
pub fn foundation_evaluation_boundaries() -> Vec<EvaluationArtifactBoundary> {
    vec![
        EvaluationArtifactBoundary {
            artifact_class: EvaluationArtifactClass::RepoSafeFixture,
            commit_allowed: true,
            access_metadata_required: false,
            redistribution_allowed: true,
        },
        EvaluationArtifactBoundary {
            artifact_class: EvaluationArtifactClass::RestrictedExternalReference,
            commit_allowed: false,
            access_metadata_required: true,
            redistribution_allowed: false,
        },
    ]
}

fn evidence_class_from_rule_id(rule_id: &str) -> EvidenceClass {
    if rule_id.starts_with("custom.") {
        EvidenceClass::CustomRule
    } else {
        EvidenceClass::BuiltInRule
    }
}

fn transform_operator_mapping(span: &ResolvedSpan, mask_visible_suffix: u8) -> String {
    match span.effective_action {
        PolicyAction::Mask => {
            format!("{}:{}:visible_suffix={}", span.winning_rule_id, action_key(span.effective_action), mask_visible_suffix)
        }
        PolicyAction::Replace => format!(
            "{}:{}:template={}",
            span.winning_rule_id,
            action_key(span.effective_action),
            span.replacement_template.as_deref().unwrap_or("[REPLACED]")
        ),
        _ => format!("{}:{}", span.winning_rule_id, action_key(span.effective_action)),
    }
}

fn action_key(action: PolicyAction) -> &'static str {
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
    use super::{
        foundation_extension_points, foundation_stage_boundaries, FoundationTransformResult,
        NormalizedHit, ResolvedHit, TransformPlan,
    };
    use crate::types::{
        DetectionCategory, DetectionHit, EvaluateDecision, EvaluationMode, PolicyAction, ResolvedSpan,
        TransformResult,
    };

    #[test]
    fn stage_boundaries_preserve_policy_and_mutation_ownership() {
        let boundaries = foundation_stage_boundaries();

        assert_eq!(
            boundaries.iter().filter(|boundary| boundary.owns_policy_decision).count(),
            1
        );
        assert_eq!(
            boundaries.iter().filter(|boundary| boundary.may_mutate_payload).count(),
            1
        );
        assert!(foundation_extension_points()
            .iter()
            .all(|extension_point| !extension_point.policy_ownership_allowed));
    }

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

        let normalized_json = serde_json::to_string(&normalized).expect("normalized hit must serialize");
        let resolved_json = serde_json::to_string(&resolved).expect("resolved hit must serialize");

        assert!(!normalized_json.contains("4111"));
        assert!(!resolved_json.contains("4111"));
        assert_eq!(resolved.validation_status, super::ValidationStatus::Resolved);
    }

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

        let plan =
            TransformPlan::from_decision(EvaluationMode::Enforce, &resolved_spans, &decision, 4);

        assert_eq!(plan.final_action, PolicyAction::Redact);
        assert_eq!(plan.transform_order[0], "stable_span_order");
        assert!(plan.per_hit_operator_mapping[0].contains("custom.card"));
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

        let replace_plan =
            TransformPlan::from_decision(EvaluationMode::Enforce, &resolved_spans, &decision, 4);
        let alternate_replace_spans = vec![replace_span_alt];
        let alternate_replace_plan = TransformPlan::from_decision(
            EvaluationMode::Enforce,
            &alternate_replace_spans,
            &decision,
            4,
        );

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
