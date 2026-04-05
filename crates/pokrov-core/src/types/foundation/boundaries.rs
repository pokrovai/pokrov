use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

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

static FOUNDATION_STAGE_BOUNDARIES: LazyLock<Vec<PipelineStageBoundary>> = LazyLock::new(|| {
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
});

static FOUNDATION_EXTENSION_POINTS: LazyLock<Vec<ExtensionPointContract>> = LazyLock::new(|| {
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
        // Both runner contracts remain intentionally identical in v1.
        // Evaluation and baseline flows must consume the same metadata-only artifacts.
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
});

static FOUNDATION_EVALUATION_BOUNDARIES: LazyLock<Vec<EvaluationArtifactBoundary>> =
    LazyLock::new(|| {
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
    });

/// Returns the frozen stage-boundary definitions for downstream consumers.
pub fn foundation_stage_boundaries() -> Vec<PipelineStageBoundary> {
    FOUNDATION_STAGE_BOUNDARIES.clone()
}

/// Returns the supported extension-point contracts for later workstreams.
pub fn foundation_extension_points() -> Vec<ExtensionPointContract> {
    FOUNDATION_EXTENSION_POINTS.clone()
}

/// Returns the frozen repository-placement boundaries for evaluation artifacts.
pub fn foundation_evaluation_boundaries() -> Vec<EvaluationArtifactBoundary> {
    FOUNDATION_EVALUATION_BOUNDARIES.clone()
}

#[cfg(test)]
mod tests {
    use super::{foundation_extension_points, foundation_stage_boundaries};

    #[test]
    fn stage_boundaries_preserve_policy_and_mutation_ownership() {
        let boundaries = foundation_stage_boundaries();

        assert_eq!(boundaries.iter().filter(|boundary| boundary.owns_policy_decision).count(), 1);
        assert_eq!(boundaries.iter().filter(|boundary| boundary.may_mutate_payload).count(), 1);
        assert!(foundation_extension_points()
            .iter()
            .all(|extension_point| !extension_point.policy_ownership_allowed));
    }
}
