use std::{collections::BTreeMap, fs, path::PathBuf};

use pokrov_core::types::{
    foundation_baseline_run_matrix, foundation_baseline_systems, foundation_dataset_inventory,
    foundation_dataset_inventory_missing_metadata, foundation_evaluation_corpora,
    foundation_phase_one_a_starter_corpus, foundation_quality_gates,
    foundation_starter_corpus_missing_groups, BaselineRequirementTier, BaselineSystem,
    DatasetExecutionScope, DatasetRepositoryStatus, DetectionMetrics, EvaluationCase,
    EvaluationCaseMode, EvaluationCaseSource, EvaluationMetricGroups, EvaluationMode,
    EvaluationReport, EvaluationResult, ParityMetrics, ParityReport, PolicyAction,
    ReadinessScoreboard, ReportOutputKind, RuntimeMetrics, SecurityMetrics,
    StarterCorpusCaseGroup, TransformationMetrics,
};

use crate::sanitization_analyzer_contract_test_support::{
    analyzer_contract_engine, analyzer_contract_request,
};
use crate::sanitization_dataset_test_support::starter_expected_sanitized_payloads;

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

fn phase_one_a_dataset_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval/datasets/phase_1a_starter.jsonl")
}

fn load_phase_one_a_dataset_cases() -> Vec<EvaluationCase> {
    let fixture = fs::read_to_string(phase_one_a_dataset_fixture_path())
        .expect("phase 1A dataset fixture should be readable");

    fixture
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<EvaluationCase>(line)
                .expect("every starter dataset line should deserialize into EvaluationCase")
        })
        .collect()
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

#[test]
fn dataset_inventory_records_are_complete_and_boundary_safe() {
    let inventory = foundation_dataset_inventory();
    let missing = foundation_dataset_inventory_missing_metadata(&inventory);

    assert_eq!(inventory.len(), 8);
    assert!(missing.is_empty(), "missing metadata fields: {missing:?}");
    assert!(inventory
        .iter()
        .any(|entry| entry.dataset_id == "ai4privacy_pii_masking_200k"));
    assert!(inventory
        .iter()
        .any(|entry| entry.dataset_id == "nvidia_nemotron_pii"));
    assert!(inventory
        .iter()
        .any(|entry| entry.dataset_id == "gretel_pii_masking_en_v1"));
    assert!(inventory
        .iter()
        .any(|entry| entry.repository_status == DatasetRepositoryStatus::RepoSafe));
    assert!(inventory
        .iter()
        .any(|entry| entry.repository_status == DatasetRepositoryStatus::RestrictedOnly));
    assert!(inventory
        .iter()
        .any(|entry| entry.execution_scope == DatasetExecutionScope::CiSafe));
    assert!(inventory
        .iter()
        .any(|entry| entry.execution_scope == DatasetExecutionScope::LocalOnly));
}

#[test]
fn phase_one_a_starter_corpus_is_concrete_for_first_parity_runs() {
    let starter = foundation_phase_one_a_starter_corpus();
    let missing = foundation_starter_corpus_missing_groups(&starter);

    assert_eq!(starter.corpus_id, "phase_1a_starter_corpus");
    assert!(missing.is_empty(), "missing starter groups: {missing:?}");
    assert!(starter
        .required_groups
        .contains(&StarterCorpusCaseGroup::DeterministicPositives));
    assert!(starter
        .required_groups
        .contains(&StarterCorpusCaseGroup::StructuredJsonCases));
    assert!(starter
        .required_groups
        .contains(&StarterCorpusCaseGroup::AdversarialSmokeCases));
    assert_eq!(starter.target_volume.per_priority_family_min, 25);
    assert_eq!(starter.target_volume.per_priority_family_max, 40);
    assert_eq!(starter.target_volume.shared_hard_negatives, 100);
    assert_eq!(starter.target_volume.structured_json_cases, 50);
    assert_eq!(starter.target_volume.adversarial_smoke_cases, 30);
}

#[test]
fn baseline_run_matrix_is_reproducible_and_scope_aware() {
    let systems = foundation_baseline_systems();
    let runs = foundation_baseline_run_matrix();

    assert!(systems
        .iter()
        .any(|system| system.system == BaselineSystem::VanillaPresidio));
    assert!(systems
        .iter()
        .any(|system| system.system == BaselineSystem::TunedPresidio));
    assert!(systems
        .iter()
        .any(|system| system.system == BaselineSystem::PokrovCurrentNative));
    assert!(systems
        .iter()
        .any(|system| system.system == BaselineSystem::PokrovUpdatedNative));
    assert!(systems
        .iter()
        .any(|system| system.system == BaselineSystem::NlmScrubber));

    let mandatory_runs = runs
        .iter()
        .filter(|run| run.tier == BaselineRequirementTier::MandatoryDeterministic)
        .count();
    assert_eq!(mandatory_runs, 4);
    assert!(runs
        .iter()
        .any(|run| run.tier == BaselineRequirementTier::OptionalFutureWorkstreams));
    assert!(runs
        .iter()
        .all(|run| run.corpus_id == "phase_1a_starter_corpus"));
    assert!(runs.iter().all(|run| {
        run.required_metadata.contains(&"run_id".to_string())
            && run.required_metadata.contains(&"git_revision".to_string())
            && run.required_metadata.contains(&"metric_groups".to_string())
    }));
}

#[test]
fn starter_dataset_fixture_replays_through_runtime_engine() {
    let engine = analyzer_contract_engine();
    let cases = load_phase_one_a_dataset_cases();
    let expected_payloads = starter_expected_sanitized_payloads();

    assert!(!cases.is_empty(), "starter dataset fixture should contain cases");

    for case in cases {
        let mut request =
            analyzer_contract_request(&format!("fixture-{}", case.case_id), EvaluationMode::Enforce);
        request.payload = case.input.clone();
        request.effective_language = case.language.clone();

        let runtime = engine
            .evaluate(request)
            .expect("dataset fixture case should evaluate");
        let projection = EvaluationResult::from_runtime_contract(&case, &runtime);

        assert_eq!(
            projection.actual_operator_outcome, case.expected_operator_outcome,
            "operator outcome mismatch for {}",
            case.case_id
        );
        assert_eq!(
            projection.actual_policy_outcome, case.expected_policy_outcome,
            "policy outcome mismatch for {}",
            case.case_id
        );
        let expected_payload = expected_payloads
            .get(case.case_id.as_str())
            .unwrap_or_else(|| panic!("missing starter sanitized payload expectation for {}", case.case_id));
        assert_eq!(
            runtime.transform.sanitized_payload,
            *expected_payload,
            "sanitized payload mismatch for {}",
            case.case_id
        );
    }
}

#[test]
#[ignore = "requires local restricted dataset manifest in secured environment"]
fn restricted_dataset_manifest_is_valid_when_provided() {
    let manifest_path = std::env::var("POKROV_RESTRICTED_DATASET_MANIFEST")
        .expect("set POKROV_RESTRICTED_DATASET_MANIFEST to local restricted manifest path");
    let manifest_content =
        fs::read_to_string(manifest_path).expect("restricted dataset manifest should be readable");
    let manifest: serde_yaml::Value =
        serde_yaml::from_str(&manifest_content).expect("manifest should deserialize from yaml");
    let datasets = manifest
        .get("datasets")
        .and_then(|value| value.as_sequence())
        .expect("manifest.datasets should be a sequence");

    assert!(
        !datasets.is_empty(),
        "restricted dataset manifest should contain at least one dataset"
    );
    for entry in datasets {
        let dataset_id = entry
            .get("dataset_id")
            .and_then(|value| value.as_str())
            .expect("dataset_id should be present");
        let access_model = entry
            .get("access_model")
            .and_then(|value| value.as_str())
            .expect("access_model should be present");
        let local_path = entry
            .get("local_path")
            .and_then(|value| value.as_str())
            .expect("local_path should be present");
        let license_constraints = entry
            .get("license_constraints")
            .and_then(|value| value.as_str())
            .expect("license_constraints should be present");
        let ci_safe = entry
            .get("ci_safe")
            .and_then(|value| value.as_bool())
            .expect("ci_safe should be present");

        assert!(!dataset_id.trim().is_empty(), "dataset_id must not be empty");
        assert!(!access_model.trim().is_empty(), "access_model must not be empty");
        assert!(
            !local_path.trim().is_empty(),
            "local_path must not be empty"
        );
        assert!(
            !license_constraints.trim().is_empty(),
            "license_constraints must not be empty"
        );
        assert!(!ci_safe, "restricted manifest entries must remain local-only");
    }
}
