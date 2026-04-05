use pokrov_core::types::{StageArtifact, StageId};

use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};

#[test]
fn stage_boundary_trace_matches_frozen_foundation_sequence() {
    let engine = foundation_engine();

    let trace = engine
        .trace_foundation_flow(foundation_request(
            "foundation-stage-trace",
            pokrov_core::types::EvaluationMode::Enforce,
        ))
        .expect("foundation trace should build");

    let stage_ids =
        trace.stage_boundaries.iter().map(|boundary| boundary.stage_id).collect::<Vec<_>>();

    assert_eq!(
        stage_ids,
        vec![
            StageId::InputNormalization,
            StageId::RecognizerExecution,
            StageId::AnalysisAndSuppression,
            StageId::PolicyResolution,
            StageId::Transformation,
            StageId::SafeExplain,
            StageId::AuditSummary,
        ]
    );
    assert!(trace
        .stage_boundaries
        .iter()
        .any(|boundary| boundary.allowed_outputs.contains(&StageArtifact::TransformResult)));
}
