use pokrov_core::types::EvaluationMode;

use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};

#[test]
fn foundation_trace_serialization_keeps_explain_and_audit_metadata_only() {
    let engine = foundation_engine();
    let raw_fragment = "Project Andromeda token sk-test-12345678 and user@example.com";

    let trace = engine
        .trace_foundation_flow(foundation_request("foundation-security", EvaluationMode::Enforce))
        .expect("foundation trace should build");

    let explain = serde_json::to_string(&trace.explain).expect("explain should serialize");
    let audit = serde_json::to_string(&trace.audit).expect("audit should serialize");

    assert!(!explain.contains(raw_fragment));
    assert!(!audit.contains(raw_fragment));
    assert!(!explain.contains("sk-test-12345678"));
    assert!(!audit.contains("user@example.com"));
}
