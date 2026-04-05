use pokrov_core::types::EvaluationMode;

use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};

#[test]
fn runtime_and_evaluation_flows_share_same_top_level_contract_families() {
    let engine = foundation_engine();

    let runtime_trace = engine
        .trace_foundation_flow(foundation_request("runtime-proof", EvaluationMode::Enforce))
        .expect("runtime trace should build");
    let evaluation_trace = engine
        .trace_foundation_flow(foundation_request("evaluation-proof", EvaluationMode::DryRun))
        .expect("evaluation trace should build");

    assert_eq!(runtime_trace.stage_boundaries, evaluation_trace.stage_boundaries);
    assert_eq!(runtime_trace.extension_points, evaluation_trace.extension_points);
    assert_eq!(runtime_trace.transform_plan.final_action, evaluation_trace.transform_plan.final_action);
    assert_eq!(runtime_trace.transform_result.final_action, evaluation_trace.transform_result.final_action);
    assert_eq!(runtime_trace.explain.family_counts.keys().collect::<Vec<_>>(), evaluation_trace.explain.family_counts.keys().collect::<Vec<_>>());
    assert_eq!(runtime_trace.audit.family_counts.keys().collect::<Vec<_>>(), evaluation_trace.audit.family_counts.keys().collect::<Vec<_>>());
}
