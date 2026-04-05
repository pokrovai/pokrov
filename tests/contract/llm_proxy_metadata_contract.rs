use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/003-llm-proxy/contracts/llm-proxy-api.yaml")
}

#[test]
fn llm_contract_requires_pokrov_metadata_fields() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let required = api["components"]["schemas"]["PokrovMetadata"]["required"]
        .as_sequence()
        .expect("PokrovMetadata.required must be present")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    for field in ["profile", "sanitized_input", "sanitized_output", "action", "rule_hits"] {
        assert!(required.contains(&field), "required metadata field '{field}' must be declared");
    }
}

#[test]
fn llm_contract_lists_forbidden_raw_fields() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let forbidden = api["x-safety-notes"]["forbidden_response_fields"]
        .as_sequence()
        .expect("x-safety-notes.forbidden_response_fields must exist")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(forbidden.contains(&"raw_prompt"));
    assert!(forbidden.contains(&"raw_response"));
    assert!(forbidden.contains(&"raw_rule_hits"));
}
