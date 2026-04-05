use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};
use pokrov_core::types::{
    foundation_evaluation_boundaries, foundation_extension_points, foundation_stage_boundaries,
    ExtensionPointKind, StageArtifact, StageId, ValidationStatus,
};

#[test]
fn foundation_contract_exports_all_approved_stage_boundaries() {
    let boundaries = foundation_stage_boundaries();

    assert_eq!(boundaries.len(), 7);
    assert_eq!(boundaries[0].stage_id, StageId::InputNormalization);
    assert_eq!(boundaries[1].stage_id, StageId::RecognizerExecution);
    assert_eq!(boundaries[2].stage_id, StageId::AnalysisAndSuppression);
    assert_eq!(boundaries[3].stage_id, StageId::PolicyResolution);
    assert_eq!(boundaries[4].stage_id, StageId::Transformation);
    assert_eq!(boundaries[5].stage_id, StageId::SafeExplain);
    assert_eq!(boundaries[6].stage_id, StageId::AuditSummary);

    assert!(boundaries[3].owns_policy_decision);
    assert!(boundaries[4].may_mutate_payload);
    assert!(boundaries[1].allowed_outputs.contains(&StageArtifact::NormalizedHit));
}

#[test]
fn foundation_contract_exports_extension_points_and_eval_boundaries() {
    let extension_points = foundation_extension_points();
    let artifact_boundaries = foundation_evaluation_boundaries();

    assert!(extension_points
        .iter()
        .any(|point| point.kind == ExtensionPointKind::NativeRecognizer));
    assert!(extension_points
        .iter()
        .any(|point| point.kind == ExtensionPointKind::RemoteRecognizer));
    assert!(extension_points
        .iter()
        .any(|point| point.kind == ExtensionPointKind::StructuredProcessor));
    assert!(extension_points
        .iter()
        .any(|point| point.kind == ExtensionPointKind::EvaluationRunner));
    assert!(extension_points.iter().any(|point| point.kind == ExtensionPointKind::BaselineRunner));

    assert_eq!(artifact_boundaries.len(), 2);
    assert!(artifact_boundaries.iter().any(|boundary| boundary.commit_allowed));
    assert!(artifact_boundaries.iter().any(|boundary| !boundary.commit_allowed));
}

#[test]
fn foundation_contract_exposes_validation_and_reason_metadata_without_raw_fragments() {
    let trace = foundation_engine()
        .trace_foundation_flow(foundation_request(
            "foundation-validation-contract",
            pokrov_core::types::EvaluationMode::Enforce,
        ))
        .expect("foundation trace should be built");

    assert!(trace
        .resolved_hits
        .iter()
        .all(|hit| hit.validation_status == ValidationStatus::Resolved));
    assert!(trace
        .resolved_hits
        .iter()
        .all(|hit| hit.reason_codes.iter().all(|code| !code.contains("sk-test"))));
}
