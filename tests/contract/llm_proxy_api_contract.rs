use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/003-llm-proxy/contracts/llm-proxy-api.yaml")
}

fn hardening_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/005-hardening-release/contracts/hardening-api.yaml")
}

fn byok_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/006-byok-passthrough-auth/contracts/byok-auth-api.yaml")
}

#[test]
fn llm_contract_defines_chat_completions_and_non_stream_success_shape() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let operation = &api["paths"]["/v1/chat/completions"]["post"];
    assert!(operation.is_mapping(), "chat completions endpoint must exist");

    let success_schema =
        &operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"];
    assert_eq!(success_schema.as_str(), Some("#/components/schemas/ChatCompletionResponse"));
}

#[test]
fn llm_contract_exposes_policy_blocked_error_response() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let blocked_response = &api["paths"]["/v1/chat/completions"]["post"]["responses"]["403"];
    assert!(blocked_response.is_mapping(), "403 response must be declared");

    let error_code_values = api["components"]["schemas"]["ErrorResponse"]["properties"]["error"]
        ["properties"]["code"]["enum"]
        .as_sequence()
        .expect("error code enum should be present")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(
        error_code_values.contains(&"policy_blocked"),
        "policy_blocked code must be part of ErrorResponse enum"
    );
}

#[test]
fn hardening_contract_declares_predictable_rate_limit_error_shape_for_chat_path() {
    let raw = std::fs::read_to_string(hardening_contract_path()).expect("hardening contract should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("hardening contract should parse");

    let response = &api["paths"]["/v1/chat/completions"]["post"]["responses"]["429"];
    assert!(response.is_mapping(), "429 response must be declared for chat completions");
    assert!(response["headers"]["Retry-After"].is_mapping());
    assert!(response["headers"]["X-RateLimit-Limit"].is_mapping());
    assert!(response["headers"]["X-RateLimit-Remaining"].is_mapping());
    assert!(response["headers"]["X-RateLimit-Reset"].is_mapping());
}

#[test]
fn byok_contract_declares_gateway_and_upstream_auth_errors_for_chat_path() {
    let raw = std::fs::read_to_string(byok_contract_path()).expect("byok contract should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("byok contract should parse");

    let responses = &api["paths"]["/v1/chat/completions"]["post"]["responses"];
    assert!(responses["401"].is_mapping());
    assert!(responses["422"].is_mapping());
    assert_eq!(
        responses["401"]["content"]["application/json"]["schema"]["$ref"].as_str(),
        Some("#/components/schemas/GatewayAuthError")
    );
    assert_eq!(
        responses["422"]["content"]["application/json"]["schema"]["$ref"].as_str(),
        Some("#/components/schemas/UpstreamCredentialError")
    );
}
