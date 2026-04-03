use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/004-mcp-mediation/contracts/mcp-mediation-api.yaml")
}

fn hardening_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/005-hardening-release/contracts/hardening-api.yaml")
}

#[test]
fn mcp_contract_defines_tool_call_endpoint_and_success_shape() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let operation = &api["paths"]["/v1/mcp/tool-call"]["post"];
    assert!(operation.is_mapping(), "mcp tool-call endpoint must exist");

    let success_schema = &operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"];
    assert_eq!(
        success_schema.as_str(),
        Some("#/components/schemas/McpToolCallResponse")
    );
}

#[test]
fn mcp_contract_exposes_blocked_validation_and_unsupported_variant_errors() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should parse");

    let status_403 = &api["paths"]["/v1/mcp/tool-call"]["post"]["responses"]["403"];
    let status_422 = &api["paths"]["/v1/mcp/tool-call"]["post"]["responses"]["422"];
    let status_503 = &api["paths"]["/v1/mcp/tool-call"]["post"]["responses"]["503"];
    assert!(status_403.is_mapping(), "403 response must be declared");
    assert!(status_422.is_mapping(), "422 response must be declared");
    assert!(status_503.is_mapping(), "503 response must be declared");

    let error_codes = api["components"]["schemas"]["McpErrorResponse"]["properties"]["error"]
        ["properties"]["code"]["enum"]
        .as_sequence()
        .expect("error code enum should exist")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    assert!(error_codes.contains(&"tool_call_blocked"));
    assert!(error_codes.contains(&"argument_validation_failed"));
    assert!(error_codes.contains(&"unsupported_variant"));
    assert!(error_codes.contains(&"upstream_unavailable"));
}

#[test]
fn hardening_contract_covers_mcp_invoke_path_and_rate_limit_error() {
    let raw = std::fs::read_to_string(hardening_contract_path()).expect("hardening contract should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("hardening contract should parse");

    let operation = &api["paths"]["/v1/mcp/tools/{toolName}/invoke"]["post"];
    assert!(operation.is_mapping(), "hardening MCP invoke endpoint must be declared");

    let rate_limit_response = &operation["responses"]["429"];
    assert!(rate_limit_response.is_mapping(), "hardening MCP invoke must declare 429 response");
    assert!(rate_limit_response["headers"]["Retry-After"].is_mapping());
}
