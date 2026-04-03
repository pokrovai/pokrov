use std::path::PathBuf;
use std::{fs, io::Write};

use tempfile::NamedTempFile;

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

#[test]
fn runtime_config_loader_accepts_valid_llm_route_bindings() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 500
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: env:OPENAI_API_KEY
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: true
"#,
    );

    let loaded = pokrov_config::loader::load_runtime_config(&config_path);
    assert!(loaded.is_ok(), "valid llm route bindings must pass validation");
}

#[test]
fn runtime_config_loader_rejects_invalid_llm_route_bindings() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 500
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: env:OPENAI_API_KEY
      enabled: false
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: true
"#,
    );

    let loaded = pokrov_config::loader::load_runtime_config(&config_path);
    let error = loaded.expect_err("disabled provider binding must fail validation");
    assert!(error
        .to_string()
        .contains("must reference an existing enabled provider"));
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes())
        .expect("temp config should be written");
    let path = file.into_temp_path().keep().expect("temp config path should persist");
    fs::canonicalize(path).expect("temp config canonicalization should succeed")
}
