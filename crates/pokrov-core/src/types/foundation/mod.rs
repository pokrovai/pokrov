mod boundaries;
mod hit_families;
mod transform;

pub use boundaries::{
    foundation_evaluation_boundaries, foundation_extension_points, foundation_stage_boundaries,
    EvaluationArtifactBoundary, EvaluationArtifactClass, ExtensionPointContract, ExtensionPointKind,
    PipelineStageBoundary, StageArtifact, StageId,
};
pub use hit_families::{EvidenceClass, HitLocationKind, NormalizedHit, ResolvedHit, ValidationStatus};
pub use transform::{FoundationTransformResult, TransformPlan};
pub(crate) use transform::action_key;

use serde::{Deserialize, Serialize};

use super::{AuditSummary, DegradedSummary, EvaluateRequest, EvaluateResult, EvaluationMode, ExplainSummary, PathClass};

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
    pub executed: super::ExecutedSummary,
    pub degraded: DegradedSummary,
}

impl FoundationExecutionTrace {
    /// Builds the shared foundation trace from canonical request/result contracts.
    pub fn from_contracts(
        request: &EvaluateRequest,
        result: &EvaluateResult,
        normalized_hits: Vec<NormalizedHit>,
        resolved_hits: Vec<ResolvedHit>,
        transform_plan: TransformPlan,
        transform_result: FoundationTransformResult,
    ) -> Self {
        Self {
            request_id: request.request_id.clone(),
            profile_id: result.profile_id.clone(),
            mode: request.mode,
            path_class: request.path_class,
            stage_boundaries: foundation_stage_boundaries(),
            extension_points: foundation_extension_points(),
            normalized_hits,
            resolved_hits,
            transform_plan,
            transform_result,
            explain: result.explain.clone(),
            audit: result.audit.clone(),
            evaluation_boundaries: foundation_evaluation_boundaries(),
            executed: result.executed.clone(),
            degraded: result.degraded.clone(),
        }
    }
}
