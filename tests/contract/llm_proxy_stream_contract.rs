use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/003-llm-proxy/contracts/llm-proxy-api.yaml")
}

fn proxy_ux_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/008-proxy-ux-solution/contracts/proxy-ux-api.yaml")
}

#[test]
fn llm_contract_declares_model_not_routed_error() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let response_422 = &api["paths"]["/v1/chat/completions"]["post"]["responses"]["422"];
    assert!(response_422.is_mapping(), "422 response for model_not_routed must exist");

    let error_code_values = api["components"]["schemas"]["ErrorResponse"]["properties"]["error"]
        ["properties"]["code"]["enum"]
        .as_sequence()
        .expect("error code enum should be present")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(
        error_code_values.contains(&"model_not_routed"),
        "model_not_routed code must be part of ErrorResponse enum"
    );
}

#[test]
fn llm_contract_declares_stream_response_content_type_and_headers() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let response_200 = &api["paths"]["/v1/chat/completions"]["post"]["responses"]["200"];
    assert!(
        response_200["content"]["text/event-stream"].is_mapping(),
        "stream content-type must be declared for 200 response"
    );
    assert!(
        response_200["headers"]["X-Request-Id"].is_mapping(),
        "stream response must include X-Request-Id header"
    );
}

#[test]
fn proxy_ux_contract_lists_alias_conflict_error_code() {
    let raw = std::fs::read_to_string(proxy_ux_contract_path()).expect("proxy ux contract should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("proxy ux contract should parse");

    let error_code_values = api["components"]["schemas"]["ErrorResponse"]["properties"]["error"]["properties"]["code"]
        ["enum"]
        .as_sequence()
        .expect("error code enum should be present")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(
        error_code_values.contains(&"alias_conflict"),
        "alias_conflict code must be part of ErrorResponse enum"
    );
}
