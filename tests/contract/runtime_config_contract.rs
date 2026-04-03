use std::path::PathBuf;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/001-bootstrap-runtime/contracts/runtime-config.schema.yaml")
}

#[test]
fn runtime_config_schema_exposes_bootstrap_required_sections() {
    let raw = std::fs::read_to_string(contract_path()).expect("schema should exist");
    let schema: serde_yaml::Value =
        serde_yaml::from_str(&raw).expect("schema should be valid yaml");

    let required = schema["required"].as_sequence().expect("required must be sequence");
    let required_values = required.iter().filter_map(serde_yaml::Value::as_str).collect::<Vec<_>>();

    assert!(required_values.contains(&"server"));
    assert!(required_values.contains(&"logging"));
    assert!(required_values.contains(&"shutdown"));
}

#[test]
fn runtime_config_schema_restricts_secret_format_to_references() {
    let raw = std::fs::read_to_string(contract_path()).expect("schema should exist");
    let schema: serde_yaml::Value =
        serde_yaml::from_str(&raw).expect("schema should be valid yaml");

    let pattern = schema["properties"]["security"]["properties"]["api_keys"]["items"]["properties"]
        ["key"]["pattern"]
        .as_str()
        .expect("secret key pattern must be defined");

    assert_eq!(pattern, "^(env|file):.+$");
    assert_eq!(
        schema["additionalProperties"].as_bool(),
        Some(true),
        "reserved sections must be forward compatible"
    );
}
