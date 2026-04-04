use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/007-codex-agent-compat/contracts/codex-responses-api.yaml")
}

#[test]
fn responses_contract_declares_sync_and_stream_success_shapes() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let operation = &api["paths"]["/v1/responses"]["post"];
    assert!(operation.is_mapping(), "responses endpoint must exist");
    assert!(operation["responses"]["200"]["content"]["application/json"].is_mapping());
    assert!(operation["responses"]["200"]["content"]["text/event-stream"].is_mapping());
}

#[test]
fn responses_contract_declares_auth_and_rate_limit_errors() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let responses = &api["paths"]["/v1/responses"]["post"]["responses"];
    assert!(responses["401"].is_mapping());
    assert!(responses["422"].is_mapping());
    assert!(responses["429"].is_mapping());
}

#[test]
fn responses_contract_declares_error_codes_for_subset_and_auth_paths() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let codes = api["components"]["schemas"]["ErrorResponse"]["properties"]["error"]["properties"]
        ["code"]["enum"]
        .as_sequence()
        .expect("error code enum should exist")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"invalid_request"));
    assert!(codes.contains(&"unsupported_request_subset"));
    assert!(codes.contains(&"gateway_unauthorized"));
    assert!(codes.contains(&"passthrough_requires_api_key_gateway_auth"));
    assert!(codes.contains(&"upstream_credential_missing"));
}
