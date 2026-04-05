use pokrov_core::types::EvaluationMode;

use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};

#[test]
fn foundation_trace_serialization_keeps_explain_and_audit_metadata_only() {
    let engine = foundation_engine();
    let raw_fragment = "Project Andromeda tenant-a token sk-test-12345678 and user@example.com";

    let trace = engine
        .trace_foundation_flow(foundation_request("foundation-security", EvaluationMode::Enforce))
        .expect("foundation trace should build");

    let explain = serde_json::to_string(&trace.explain).expect("explain should serialize");
    let audit = serde_json::to_string(&trace.audit).expect("audit should serialize");
    let executed = serde_json::to_string(&trace.executed).expect("executed should serialize");
    let degraded = serde_json::to_string(&trace.degraded).expect("degraded should serialize");

    assert!(!explain.contains(raw_fragment));
    assert!(!audit.contains(raw_fragment));
    assert!(!executed.contains(raw_fragment));
    assert!(!degraded.contains(raw_fragment));
    assert!(!explain.contains("sk-test-12345678"));
    assert!(!audit.contains("user@example.com"));
    assert!(!executed.contains("sk-test-12345678"));
    assert!(!degraded.contains("user@example.com"));
    let hits = serde_json::to_string(&trace.resolved_hits).expect("resolved hits should serialize");
    assert!(!hits.contains("sk-test-12345678"));
    assert!(!explain.contains("tenant-a"));
    assert!(!audit.contains("tenant-a"));
}
