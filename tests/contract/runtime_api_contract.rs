use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/001-bootstrap-runtime/contracts/runtime-api.yaml")
}

fn mcp_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/004-mcp-mediation/contracts/mcp-mediation-api.yaml")
}

fn hardening_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/005-hardening-release/contracts/hardening-api.yaml")
}

#[test]
fn runtime_api_contract_contains_expected_probe_routes() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should be valid yaml");

    assert!(api["paths"]["/health"]["get"].is_mapping());
    assert!(api["paths"]["/ready"]["get"].is_mapping());
}

#[test]
fn runtime_api_contract_readiness_statuses_and_headers_match_expectations() {
    let raw = std::fs::read_to_string(contract_path()).expect("contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("contract should be valid yaml");

    let ready_responses = &api["paths"]["/ready"]["get"]["responses"];
    assert!(ready_responses["200"].is_mapping());
    assert!(ready_responses["503"].is_mapping());

    let header_name = "X-Request-Id";
    assert!(
        ready_responses["200"]["headers"][header_name].is_mapping(),
        "ready 200 must expose request id header"
    );
    assert!(
        ready_responses["503"]["headers"][header_name].is_mapping(),
        "ready 503 must expose request id header"
    );
}

#[test]
fn runtime_contract_suite_covers_mcp_mediation_route() {
    let raw = std::fs::read_to_string(mcp_contract_path()).expect("mcp contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("mcp contract should be valid yaml");

    assert!(
        api["paths"]["/v1/mcp/tool-call"]["post"].is_mapping(),
        "mcp tool call route must be covered by contract suite"
    );
}

#[test]
fn hardening_runtime_contract_includes_metrics_endpoint() {
    let raw = std::fs::read_to_string(hardening_contract_path()).expect("hardening contract file should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("hardening contract should be valid yaml");

    assert!(api["paths"]["/metrics"]["get"].is_mapping());
}

#[test]
fn hardening_runtime_contract_declares_degraded_ready_state() {
    let raw = std::fs::read_to_string(hardening_contract_path()).expect("hardening contract should exist");
    let api: serde_yaml::Value = serde_yaml::from_str(&raw).expect("hardening contract should parse");

    let ready_503 = &api["paths"]["/ready"]["get"]["responses"]["503"];
    assert!(ready_503.is_mapping(), "hardening /ready must declare 503");

    let statuses = api["components"]["schemas"]["ReadyResponse"]["properties"]["status"]["enum"]
        .as_sequence()
        .expect("ReadyResponse.status enum should be defined")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();
    assert!(
        statuses.contains(&"degraded"),
        "ReadyResponse.status must declare degraded state"
    );
}
