use std::time::Instant;

use pokrov_core::types::EvaluationMode;

use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};

#[test]
fn foundation_trace_overhead_stays_within_local_budget() {
    let engine = foundation_engine();
    let request = foundation_request("foundation-perf", EvaluationMode::Enforce);
    let mut samples = Vec::new();

    for idx in 0..32 {
        let started = Instant::now();
        let trace = engine
            .trace_foundation_flow(foundation_request(
                &format!("foundation-perf-{idx}"),
                request.mode,
            ))
            .expect("foundation trace should build");
        samples.push(started.elapsed().as_millis() as u64);
        assert_eq!(trace.transform_plan.final_action, trace.transform_result.final_action);
    }

    samples.sort_unstable();
    let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
    let p95 = samples[p95_index];

    assert!(p95 <= 25, "foundation trace p95 must stay <= 25ms, got {p95}ms");
}
