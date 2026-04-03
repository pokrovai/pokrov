use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/002-sanitization-core/contracts/sanitization-evaluate-api.yaml")
}

#[test]
fn evaluate_contract_exposes_public_route_and_modes() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should be valid yaml");

    let post = &api["paths"]["/v1/sanitize/evaluate"]["post"];
    assert!(post.is_mapping());

    let mode = &api["components"]["schemas"]["EvaluateRequest"]["properties"]["mode"]["enum"];
    let values = mode
        .as_sequence()
        .expect("mode enum should be sequence")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(values.contains(&"enforce"));
    assert!(values.contains(&"dry_run"));
}

#[test]
fn evaluate_contract_requires_metadata_only_response_shape() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should be valid yaml");

    let response = &api["components"]["schemas"]["EvaluateResponse"]["required"];
    let required = response
        .as_sequence()
        .expect("required should be sequence")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(required.contains(&"request_id"));
    assert!(required.contains(&"final_action"));
    assert!(required.contains(&"explain"));
    assert!(required.contains(&"audit"));

    let forbidden = &api["x-safety-notes"]["forbidden_response_fields"];
    let values = forbidden
        .as_sequence()
        .expect("forbidden fields should be sequence")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();
    assert!(values.contains(&"raw_payload"));
    assert!(values.contains(&"raw_fragments"));
}
