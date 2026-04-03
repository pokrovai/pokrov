use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/001-bootstrap-runtime/contracts/runtime-api.yaml")
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
