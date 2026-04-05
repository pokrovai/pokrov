use std::collections::BTreeMap;

use pokrov_core::types::{
    foundation_evaluation_corpora, foundation_quality_gates, DetectionMetrics, EvaluationCase,
    EvaluationCaseMode, EvaluationCaseSource, EvaluationMetricGroups, EvaluationMode,
    EvaluationReport, EvaluationResult, ParityMetrics, ParityReport, PolicyAction,
    ReadinessScoreboard, ReportOutputKind, RuntimeMetrics, SecurityMetrics,
    TransformationMetrics,
};

use crate::sanitization_analyzer_contract_test_support::{
    analyzer_contract_engine, analyzer_contract_request,
};

fn sample_case(mode: EvaluationCaseMode, source: EvaluationCaseSource) -> EvaluationCase {
    EvaluationCase {
        case_id: "case-eval-001".to_string(),
        language: "en".to_string(),
        mode,
        input: serde_json::json!({
            "messages": [
                {
                    "role": "user",
                    "content": "my card is 4111 1111 1111 1111",
                }
            ]
        }),
        expected_entities: vec!["card_like_number".to_string()],
        expected_operator_outcome: PolicyAction::Redact,
        expected_policy_outcome: PolicyAction::Redact,
        tags: vec!["synthetic".to_string(), "operator".to_string()],
        source,
        notes: Some("deterministic replay fixture".to_string()),
    }
}

fn sample_metric_groups() -> EvaluationMetricGroups {
    EvaluationMetricGroups {
        detection: DetectionMetrics {
            precision: 1.0,
            recall: 1.0,
            f2: 1.0,
            per_entity_breakdown: BTreeMap::from([("card_like_number".to_string(), 1.0)]),
            per_family_breakdown: BTreeMap::from([("checksum".to_string(), 1.0)]),
            per_language_breakdown: BTreeMap::from([("en".to_string(), 1.0)]),
        },
        parity: ParityMetrics {
            detection_delta_vs_presidio: 0.0,
            operator_delta_vs_presidio: 0.0,
            coverage_delta_vs_presidio: 0.0,
        },
        security: SecurityMetrics {
            leakage_checks_passed: true,
            fail_closed_correctness: true,
            adversarial_bypass_rate: 0.0,
        },
        runtime: RuntimeMetrics {
            p50_latency_ms: 2,
            p95_latency_ms: 5,
            native_recognizer_cost_ms: 2,
            remote_recognizer_cost_ms: 0,
        },
        transformation: TransformationMetrics {
            operator_correctness: 1.0,
            overlap_correctness: 1.0,
            json_validity_preservation: 1.0,
        },
    }
}

#[test]
fn evaluation_case_schema_exposes_required_fields_and_modes() {
    let case = sample_case(EvaluationCaseMode::StructuredJson, EvaluationCaseSource::Synthetic);
    let serialized = serde_json::to_value(case).expect("evaluation case should serialize");

    for required_field in [
        "case_id",
        "language",
        "mode",
        "input",
        "expected_entities",
        "expected_operator_outcome",
        "expected_policy_outcome",
        "tags",
        "source",
    ] {
        assert!(serialized.get(required_field).is_some(), "missing {required_field}");
    }

    let modes = [
        EvaluationCaseMode::Text,
        EvaluationCaseMode::StructuredJson,
        EvaluationCaseMode::BatchStructured,
        EvaluationCaseMode::ImageOcr,
    ]
    .iter()
    .map(|mode| serde_json::to_value(mode).expect("mode should serialize"))
    .collect::<Vec<_>>();

    assert_eq!(modes[0], serde_json::Value::String("text".to_string()));
    assert_eq!(modes[1], serde_json::Value::String("structured_json".to_string()));
    assert_eq!(modes[2], serde_json::Value::String("batch_structured".to_string()));
    assert_eq!(modes[3], serde_json::Value::String("image_ocr".to_string()));
}

#[test]
fn evaluation_result_reuses_runtime_contract_sections_without_adapters() {
    let engine = analyzer_contract_engine();
    let runtime = engine
        .evaluate(analyzer_contract_request(
            "eval-lab-runtime-compat",
            EvaluationMode::Enforce,
        ))
        .expect("runtime evaluation should succeed");
    let case = sample_case(EvaluationCaseMode::Text, EvaluationCaseSource::Synthetic);
    let result = EvaluationResult::from_runtime_contract(&case, &runtime);

    assert_eq!(result.case_id, case.case_id);
    assert_eq!(result.decision, runtime.decision);
    assert_eq!(result.explain, runtime.explain);
    assert_eq!(result.audit, runtime.audit);
    assert_eq!(result.executed, runtime.executed);
    assert_eq!(result.degraded, runtime.degraded);
    assert_eq!(result.transform.final_action, runtime.transform.final_action);
}

#[test]
fn report_and_scoreboard_schema_are_stable_and_gate_driven() {
    let metric_groups = sample_metric_groups();
    let report = EvaluationReport {
        report_id: "report-01".to_string(),
        total_cases: 10,
        passed_cases: 10,
        metric_groups: metric_groups.clone(),
        output_kinds: vec![
            ReportOutputKind::FamilySummary,
            ReportOutputKind::EntityBreakdown,
            ReportOutputKind::ParityReport,
            ReportOutputKind::SafetyAndLeakage,
            ReportOutputKind::ReadinessScoreboard,
        ],
    };
    let parity = ParityReport {
        report_id: "parity-01".to_string(),
        baseline: "presidio".to_string(),
        metrics: report.metric_groups.parity.clone(),
        family_deltas: BTreeMap::from([("checksum".to_string(), 0.0)]),
        entity_deltas: BTreeMap::from([("card_like_number".to_string(), 0.0)]),
    };
    let scoreboard = ReadinessScoreboard {
        scoreboard_id: "scoreboard-01".to_string(),
        gate_levels: foundation_quality_gates(),
        metric_groups,
        output_kinds: report.output_kinds.clone(),
    };

    assert_eq!(report.total_cases, report.passed_cases);
    assert_eq!(parity.baseline, "presidio");
    assert!(scoreboard.gate_levels.iter().any(|gate| !gate.blocking));
    assert!(scoreboard.gate_levels.iter().any(|gate| gate.blocking));
    assert!(scoreboard
        .gate_levels
        .iter()
        .all(|gate| !gate.required_outputs.is_empty()));
}

#[test]
fn corpus_definitions_freeze_minimum_requirements() {
    let corpora = foundation_evaluation_corpora();

    assert_eq!(corpora.len(), 3);
    assert!(corpora
        .iter()
        .any(|corpus| corpus.required_contents.contains(&"nested JSON cases".to_string())));
    assert!(corpora
        .iter()
        .any(|corpus| corpus.required_contents.contains(&"EN and RU prompts".to_string())));
    assert!(corpora
        .iter()
        .any(|corpus| corpus.required_contents.contains(&"unicode confusables".to_string())));
}
