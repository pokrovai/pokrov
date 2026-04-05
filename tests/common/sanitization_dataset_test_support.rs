use std::{collections::{BTreeMap, BTreeSet}, path::PathBuf};

use serde_json::Value;

pub const OPEN_DATASET_CACHE_DIR: &str = "tests/fixtures/eval/datasets/open-cache";
pub const OPEN_SNAPSHOT_FILES: [&str; 4] = [
    "open_ai4privacy_pii_masking_200k.json",
    "open_nvidia_nemotron_pii.json",
    "open_gretel_pii_masking_en_v1.json",
    "open_presidio_research_repo.json",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DatasetAnnotation {
    pub label: String,
    pub value: String,
}

pub fn dataset_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(OPEN_DATASET_CACHE_DIR)
}

pub fn read_open_snapshot(file_name: &str) -> Value {
    let path = dataset_cache_dir().join(file_name);
    let snapshot_raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("snapshot {} should be readable: {error}", path.display()));
    serde_json::from_str(&snapshot_raw)
        .unwrap_or_else(|error| panic!("snapshot {} should decode to JSON: {error}", path.display()))
}

pub fn starter_expected_sanitized_payloads() -> BTreeMap<&'static str, Option<Value>> {
    BTreeMap::from([
        (
            "starter-text-allow-001",
            Some(serde_json::json!({
                "messages": [
                    {
                        "role": "user",
                        "content": "status update with no sensitive markers",
                    }
                ]
            })),
        ),
        ("starter-text-block-pan-001", None),
        (
            "starter-text-redact-marker-001",
            Some(serde_json::json!({
                "messages": [
                    {
                        "role": "user",
                        "content": "[REDACTED] launch memo",
                    }
                ]
            })),
        ),
        ("starter-text-block-bearer-001", None),
        ("starter-text-block-sk-codex-001", None),
        ("starter-json-block-pan-001", None),
        (
            "starter-json-allow-001",
            Some(serde_json::json!({
                "tool_args": {
                    "customer_id": "cust_public_12345",
                    "status": "ok",
                }
            })),
        ),
    ])
}

pub fn supported_dataset_label_mapping() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("CREDITCARDNUMBER", "card_like_number"),
        ("EMAIL", "email"),
        ("IPV4", "ip_address"),
        ("STREET", "en_address_like_high_risk"),
        ("credit_card_number", "card_like_number"),
        ("credit_debit_card", "card_like_number"),
        ("email", "email"),
        ("ipv4", "ip_address"),
        ("phone", "phone_number"),
        ("phone_number", "phone_number"),
        ("street_address", "en_address_like_high_risk"),
        ("url", "url_or_domain"),
    ])
}

pub fn replay_assertable_dataset_labels() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "CREDITCARDNUMBER",
        "EMAIL",
        "credit_card_number",
        "credit_debit_card",
        "email",
    ])
}

pub fn known_unsupported_dataset_labels() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "ACCOUNTNUMBER",
        "AGE",
        "BUILDINGNUMBER",
        "CITY",
        "COUNTY",
        "CREDITCARDISSUER",
        "DATE",
        "DOB",
        "EYECOLOR",
        "FIRSTNAME",
        "GENDER",
        "HEIGHT",
        "IPV6",
        "JOBAREA",
        "JOBTITLE",
        "LASTNAME",
        "MASKEDNUMBER",
        "MIDDLENAME",
        "NEARBYGPSCOORDINATE",
        "PASSWORD",
        "PHONEIMEI",
        "PIN",
        "PREFIX",
        "STATE",
        "TIME",
        "USERAGENT",
        "VEHICLEVIN",
        "VEHICLEVRM",
        "account_number",
        "address",
        "age",
        "bank_routing_number",
        "biometric_identifier",
        "blood_type",
        "certificate_license_number",
        "city",
        "company_name",
        "coordinate",
        "country",
        "county",
        "customer_id",
        "cvv",
        "date",
        "date_of_birth",
        "date_time",
        "device_identifier",
        "education_level",
        "employee_id",
        "employment_status",
        "first_name",
        "gender",
        "health_plan_beneficiary_number",
        "last_name",
        "license_plate",
        "mac_address",
        "medical_record_number",
        "name",
        "occupation",
        "password",
        "pin",
        "political_view",
        "race_ethnicity",
        "religious_belief",
        "sexuality",
        "ssn",
        "state",
        "swift_bic",
        "time",
        "unique_identifier",
        "user_name",
        "vehicle_identifier",
    ])
}

pub fn row_by_idx<'a>(snapshot: &'a Value, row_idx: usize) -> &'a Value {
    snapshot
        .get("rows")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|entry| {
                entry
                    .get("row_idx")
                    .and_then(Value::as_u64)
                    .map(|index| index == row_idx as u64)
                    .unwrap_or(false)
            })
        })
        .and_then(|entry| entry.get("row").or(Some(entry)))
        .unwrap_or_else(|| panic!("row_idx {row_idx} should exist in snapshot"))
}

pub fn row_text(row: &Value) -> String {
    for key in ["source_text", "text", "content", "prompt", "sentence", "input", "document"] {
        if let Some(text) = row.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return text.to_string();
            }
        }
    }

    panic!("row should expose a replayable text field");
}

pub fn collect_snapshot_labels(snapshot: &Value) -> BTreeSet<String> {
    let Some(rows) = snapshot.get("rows").and_then(Value::as_array) else {
        return BTreeSet::new();
    };

    rows.iter()
        .flat_map(|entry| row_annotations(entry.get("row").unwrap_or(entry)))
        .map(|annotation| annotation.label)
        .collect()
}

pub fn replay_assertable_annotations(row: &Value) -> Vec<DatasetAnnotation> {
    let supported = replay_assertable_dataset_labels();
    row_annotations(row)
        .into_iter()
        .filter(|annotation| supported.contains(annotation.label.as_str()))
        .collect()
}

pub fn expected_redacted_text(source: &str, annotations: &[DatasetAnnotation]) -> String {
    let mut result = source.to_string();
    let mut replacements = annotations
        .iter()
        .map(|annotation| annotation.value.as_str())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    replacements.sort_by_key(|value| usize::MAX - value.len());
    replacements.dedup();

    for value in replacements {
        result = result.replace(value, "[REDACTED]");
    }

    result
}

fn row_annotations(row: &Value) -> Vec<DatasetAnnotation> {
    if let Some(items) = row.get("privacy_mask").and_then(Value::as_array) {
        return items
            .iter()
            .filter_map(|item| {
                let label = item.get("label").and_then(Value::as_str)?;
                let value = item.get("value").and_then(Value::as_str)?;
                Some(DatasetAnnotation {
                    label: label.to_string(),
                    value: value.to_string(),
                })
            })
            .collect();
    }

    if let Some(raw) = row.get("spans").and_then(Value::as_str) {
        return parse_yaml_annotations(raw, "text", "label");
    }

    if let Some(raw) = row.get("entities").and_then(Value::as_str) {
        return parse_entity_annotations(raw);
    }

    Vec::new()
}

fn parse_yaml_annotations(raw: &str, value_key: &str, label_key: &str) -> Vec<DatasetAnnotation> {
    let parsed: serde_yaml::Value = serde_yaml::from_str(raw)
        .unwrap_or_else(|error| panic!("annotation payload should parse as yaml flow sequence: {error}"));
    let Some(items) = parsed.as_sequence() else {
        return Vec::new();
    };

    items.iter()
        .filter_map(|item| {
            let label = item.get(label_key)?.as_str()?;
            let value = item.get(value_key)?.as_str()?;
            Some(DatasetAnnotation {
                label: label.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

fn parse_entity_annotations(raw: &str) -> Vec<DatasetAnnotation> {
    let parsed: serde_yaml::Value = serde_yaml::from_str(raw)
        .unwrap_or_else(|error| panic!("entity payload should parse as yaml flow sequence: {error}"));
    let Some(items) = parsed.as_sequence() else {
        return Vec::new();
    };

    let mut annotations = Vec::new();
    for item in items {
        let Some(value) = item.get("entity").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        let Some(types) = item.get("types").and_then(serde_yaml::Value::as_sequence) else {
            continue;
        };
        for label in types.iter().filter_map(serde_yaml::Value::as_str) {
            annotations.push(DatasetAnnotation {
                label: label.to_string(),
                value: value.to_string(),
            });
        }
    }

    annotations
}
