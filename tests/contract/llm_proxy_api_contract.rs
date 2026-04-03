use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/003-llm-proxy/contracts/llm-proxy-api.yaml")
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
