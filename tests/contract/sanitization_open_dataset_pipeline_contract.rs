use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{Mutex, OnceLock},
};

use pokrov_core::types::{EvaluationMode, PolicyAction};
use serde_json::Value;

use crate::sanitization_analyzer_contract_test_support::{
    analyzer_contract_engine, analyzer_contract_request,
};
use crate::sanitization_dataset_report_test_support::{
    render_dataset_detector_gap_report, DATASET_DETECTOR_GAP_REPORT_PATH,
};
use crate::sanitization_dataset_test_support::{
    collect_snapshot_labels, dataset_cache_dir, expected_redacted_text,
    known_unsupported_dataset_labels, read_open_snapshot, replay_assertable_annotations,
    row_by_idx, row_text, supported_dataset_label_mapping, OPEN_SNAPSHOT_FILES,
};

const DOWNLOAD_SCRIPT: &str = "scripts/eval/download_open_datasets.sh";

fn run_open_dataset_download_script() -> Vec<PathBuf> {
    static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = DOWNLOAD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().expect("download lock should be available");

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOWNLOAD_SCRIPT);
    let cache_dir = dataset_cache_dir();
    fs::create_dir_all(&cache_dir).expect("cache directory should be created");

    let status = Command::new("bash")
        .arg(&script_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("download script should be executable");
    assert!(status.success(), "download script should succeed");

    OPEN_SNAPSHOT_FILES
        .iter()
        .map(|file| cache_dir.join(file))
        .collect()
}

#[test]
#[ignore = "network-dependent autoload test for open datasets"]
fn open_dataset_pipeline_autoloads_snapshots_for_all_sources() {
    let snapshot_paths = run_open_dataset_download_script();

    assert_eq!(snapshot_paths.len(), 4);
    for path in snapshot_paths {
        assert!(path.exists(), "snapshot should exist: {}", path.display());
        let metadata = fs::metadata(&path).expect("snapshot metadata should be readable");
        assert!(
            metadata.len() > 20,
            "snapshot should contain meaningful JSON: {}",
            path.display()
        );
        let payload = fs::read_to_string(&path).expect("snapshot payload should be readable");
        assert!(
            payload.contains("\"source_id\""),
            "snapshot should preserve source metadata: {}",
            path.display()
        );
    }
}

#[test]
#[ignore = "network-dependent runtime replay using open datasets"]
fn open_dataset_pipeline_replays_samples_through_runtime_engine() {
    let snapshot_paths = run_open_dataset_download_script();
    let engine = analyzer_contract_engine();
    let mut replayed_cases = 0_u32;

    for path in snapshot_paths {
        let snapshot_raw = fs::read_to_string(&path).expect("snapshot should be readable");
        let snapshot: Value =
            serde_json::from_str(&snapshot_raw).expect("snapshot should decode to JSON");

        if snapshot.get("source_kind").and_then(|value| value.as_str())
            != Some("huggingface_dataset")
        {
            continue;
        }

        let rows = snapshot
            .get("rows")
            .and_then(|value| value.as_array())
            .expect("huggingface snapshot should contain rows");
        for row in rows.iter().take(3) {
            let payload_row = row.get("row").unwrap_or(row);
            let sample = row_text(payload_row);
            let mut request = analyzer_contract_request(
                &format!("open-dataset-replay-{replayed_cases}"),
                EvaluationMode::Enforce,
            );
            request.payload = serde_json::json!({
                "messages": [
                    {
                        "role": "user",
                        "content": sample,
                    }
                ]
            });
            let result = engine
                .evaluate(request)
                .expect("open dataset sample should evaluate");
            assert!(
                !result.audit.request_id.is_empty(),
                "audit metadata should remain populated"
            );
            replayed_cases += 1;
        }
    }

    assert!(
        replayed_cases >= 3,
        "expected at least one replay sample per huggingface dataset"
    );
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_labels_are_explicitly_partitioned_into_supported_or_backlog() {
    let mut discovered = std::collections::BTreeSet::new();

    for file in OPEN_SNAPSHOT_FILES {
        let path = dataset_cache_dir().join(file);
        assert!(path.exists(), "cached snapshot should exist: {}", path.display());
        let snapshot = read_open_snapshot(file);
        discovered.extend(collect_snapshot_labels(&snapshot));
    }

    assert!(
        !discovered.is_empty(),
        "expected cached open snapshots to expose at least one dataset label"
    );

    let supported = supported_dataset_label_mapping();
    let unsupported = known_unsupported_dataset_labels();
    let unmapped = discovered
        .iter()
        .filter(|label| !supported.contains_key(label.as_str()) && !unsupported.contains(label.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let conflicting = supported
        .keys()
        .filter(|label| unsupported.contains(**label))
        .copied()
        .collect::<Vec<_>>();

    assert!(
        unmapped.is_empty(),
        "every discovered dataset label must be mapped or backlogged, unmapped: {unmapped:?}"
    );
    assert!(
        conflicting.is_empty(),
        "mapped labels must not also be listed as unsupported: {conflicting:?}"
    );
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_ai4privacy_rows_match_expected_block_and_email_redaction() {
    let snapshot = read_open_snapshot("open_ai4privacy_pii_masking_200k.json");
    let engine = analyzer_contract_engine();

    let block_row = row_by_idx(&snapshot, 15);
    let block_text = row_text(block_row);
    let block_annotations = replay_assertable_annotations(block_row);
    assert_eq!(block_annotations.len(), 1, "ai4privacy block row should expose one assertable annotation");
    assert_eq!(block_annotations[0].label, "CREDITCARDNUMBER");
    assert!(block_text.contains(block_annotations[0].value.as_str()));

    let mut block_request = analyzer_contract_request("ai4privacy-row-15", EvaluationMode::Enforce);
    block_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": block_text,
            }
        ]
    });
    let block_result = engine
        .evaluate(block_request)
        .expect("ai4privacy block row should evaluate");
    assert_eq!(block_result.decision.final_action, PolicyAction::Block);
    assert!(block_result.transform.sanitized_payload.is_none(), "blocked ai4privacy row must not expose sanitized payload");

    let redact_row = row_by_idx(&snapshot, 20);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "ai4privacy redact row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "EMAIL");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("ai4privacy-row-20", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("ai4privacy redact row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("ai4privacy redact row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "ai4privacy row 20 sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_nvidia_rows_match_expected_block_and_email_redaction() {
    let snapshot = read_open_snapshot("open_nvidia_nemotron_pii.json");
    let engine = analyzer_contract_engine();

    let block_row = row_by_idx(&snapshot, 4);
    let block_text = row_text(block_row);
    let block_annotations = replay_assertable_annotations(block_row);
    assert!(
        block_annotations.iter().any(|annotation| annotation.label == "credit_debit_card"),
        "nvidia block row should expose a card-like annotation"
    );

    let mut block_request = analyzer_contract_request("nvidia-row-4", EvaluationMode::Enforce);
    block_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": block_text,
            }
        ]
    });
    let block_result = engine
        .evaluate(block_request)
        .expect("nvidia block row should evaluate");
    assert_eq!(block_result.decision.final_action, PolicyAction::Block);
    assert!(block_result.transform.sanitized_payload.is_none(), "blocked nvidia row must not expose sanitized payload");

    let redact_row = row_by_idx(&snapshot, 23);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "nvidia redact row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "email");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("nvidia-row-23", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("nvidia redact row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("nvidia redact row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "nvidia row 23 sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_gretel_rows_match_expected_block_and_email_redaction() {
    let snapshot = read_open_snapshot("open_gretel_pii_masking_en_v1.json");
    let engine = analyzer_contract_engine();

    let block_row = row_by_idx(&snapshot, 12);
    let block_text = row_text(block_row);
    let block_annotations = replay_assertable_annotations(block_row);
    assert!(
        block_annotations.iter().any(|annotation| annotation.label == "credit_card_number"),
        "gretel block row should expose a card-like annotation"
    );

    let mut block_request = analyzer_contract_request("gretel-row-12", EvaluationMode::Enforce);
    block_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": block_text,
            }
        ]
    });
    let block_result = engine
        .evaluate(block_request)
        .expect("gretel block row should evaluate");
    assert_eq!(block_result.decision.final_action, PolicyAction::Block);
    assert!(block_result.transform.sanitized_payload.is_none(), "blocked gretel row must not expose sanitized payload");

    let redact_row = row_by_idx(&snapshot, 3);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "gretel redact row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "email");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("gretel-row-3", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("gretel redact row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("gretel redact row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "gretel row 3 sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_ai4privacy_row_match_expected_ipv4_redaction() {
    let snapshot = read_open_snapshot("open_ai4privacy_pii_masking_200k.json");
    let engine = analyzer_contract_engine();

    let redact_row = row_by_idx(&snapshot, 19);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "ai4privacy ipv4 row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "IPV4");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("ai4privacy-row-19-ipv4", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("ai4privacy ipv4 row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("ai4privacy ipv4 row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "ai4privacy row 19 ipv4 sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_nvidia_row_match_expected_url_redaction() {
    let snapshot = read_open_snapshot("open_nvidia_nemotron_pii.json");
    let engine = analyzer_contract_engine();

    let redact_row = row_by_idx(&snapshot, 2);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "nvidia url row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "url");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("nvidia-row-2-url", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("nvidia url row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("nvidia url row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "nvidia row 2 url sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_nvidia_row_match_expected_phone_redaction() {
    let snapshot = read_open_snapshot("open_nvidia_nemotron_pii.json");
    let engine = analyzer_contract_engine();

    let redact_row = row_by_idx(&snapshot, 3);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "nvidia phone row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "phone_number");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("nvidia-row-3-phone", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("nvidia phone row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("nvidia phone row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "nvidia row 3 phone sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_nvidia_row_match_expected_license_plate_redaction() {
    let snapshot = read_open_snapshot("open_nvidia_nemotron_pii.json");
    let engine = analyzer_contract_engine();

    let redact_row = row_by_idx(&snapshot, 18);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "nvidia license plate row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "license_plate");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("nvidia-row-18-license-plate", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("nvidia license plate row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("nvidia license plate row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "nvidia row 18 license plate sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn open_dataset_gretel_row_match_expected_medical_record_redaction() {
    let snapshot = read_open_snapshot("open_gretel_pii_masking_en_v1.json");
    let engine = analyzer_contract_engine();

    let redact_row = row_by_idx(&snapshot, 14);
    let redact_text = row_text(redact_row);
    let redact_annotations = replay_assertable_annotations(redact_row);
    assert_eq!(redact_annotations.len(), 1, "gretel medical record row should expose one assertable annotation");
    assert_eq!(redact_annotations[0].label, "medical_record_number");
    let expected_text = expected_redacted_text(&redact_text, &redact_annotations);

    let mut redact_request = analyzer_contract_request("gretel-row-14-medical-record", EvaluationMode::Enforce);
    redact_request.payload = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": redact_text,
            }
        ]
    });
    let redact_result = engine
        .evaluate(redact_request)
        .expect("gretel medical record row should evaluate");
    assert_eq!(redact_result.decision.final_action, PolicyAction::Redact);
    let actual_text = redact_result.transform.sanitized_payload
        .as_ref()
        .and_then(|payload| payload.pointer("/messages/0/content"))
        .and_then(Value::as_str)
        .expect("gretel medical record row should preserve sanitized text");
    assert_eq!(actual_text, expected_text, "gretel row 14 medical record sanitized text mismatch");
    assert!(!actual_text.contains(redact_annotations[0].value.as_str()));
}

#[test]
#[ignore = "local cached open snapshots only"]
fn dataset_detector_gap_report_is_up_to_date_for_cached_snapshots() {
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DATASET_DETECTOR_GAP_REPORT_PATH);
    let current = fs::read_to_string(&report_path)
        .unwrap_or_else(|error| panic!("report {} should be readable: {error}", report_path.display()));
    let report_date = current
        .lines()
        .find_map(|line| line.strip_prefix("Date: "))
        .expect("report should contain a Date line");
    let expected = render_dataset_detector_gap_report(report_date);

    assert_eq!(
        current, expected,
        "dataset detector gap report is stale, regenerate {}",
        report_path.display()
    );
}

#[test]
#[ignore = "manual regeneration path for local cached open snapshots"]
fn dataset_detector_gap_report_can_be_regenerated_when_requested() {
    if std::env::var("POKROV_WRITE_DATASET_DETECTOR_GAP_REPORT").as_deref() != Ok("1") {
        return;
    }

    let report_date = std::env::var("POKROV_DATASET_DETECTOR_GAP_REPORT_DATE")
        .unwrap_or_else(|_| "2026-04-06".to_string());
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DATASET_DETECTOR_GAP_REPORT_PATH);
    let rendered = render_dataset_detector_gap_report(&report_date);
    fs::write(&report_path, rendered)
        .unwrap_or_else(|error| panic!("report {} should be writable: {error}", report_path.display()));
}
