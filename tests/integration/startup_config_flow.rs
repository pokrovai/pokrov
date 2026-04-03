use std::{io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn runtime_starts_with_valid_config_and_serves_probes() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
"#,
    );

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client should build");

    let health = client
        .get(format!("{}/health", handle.base_url()))
        .send()
        .await
        .expect("health request should succeed");
    assert_eq!(health.status(), StatusCode::OK);

    let ready = client
        .get(format!("{}/ready", handle.base_url()))
        .send()
        .await
        .expect("ready request should succeed");
    assert_eq!(ready.status(), StatusCode::OK);

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn runtime_rejects_invalid_config_before_ready() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 5000
  grace_period_ms: 1000
security:
  api_keys:
    - key: plaintext-secret
      profile: strict
"#,
    );

    let startup = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path).await;
    assert!(startup.is_err(), "invalid config must fail startup");
}

#[tokio::test]
async fn runtime_with_disabled_sanitization_reports_ready() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
sanitization:
  enabled: false
"#,
    );

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client should build");

    let ready = client
        .get(format!("{}/ready", handle.base_url()))
        .send()
        .await
        .expect("ready request should succeed");
    assert_eq!(ready.status(), StatusCode::OK);

    let body: serde_json::Value = ready.json().await.expect("json body expected");
    assert_eq!(body["checks"]["policy"], "ok");

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn runtime_with_unresolved_api_key_binding_starts_when_strict_resolution_disabled() {
    let missing_key_path = format!(
        "/tmp/pokrov-missing-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    let config_path = write_temp_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 1000
security:
  api_keys:
    - key: file:{missing_key_path}
      profile: strict
"#
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start when strict api key resolution is disabled");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client should build");

    let ready = client
        .get(format!("{}/ready", handle.base_url()))
        .send()
        .await
        .expect("ready request should succeed");
    assert_eq!(ready.status(), StatusCode::OK);

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn runtime_with_unresolved_api_key_binding_fails_when_strict_resolution_enabled() {
    let missing_key_path = format!(
        "/tmp/pokrov-missing-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    let config_path = write_temp_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 1000
security:
  fail_on_unresolved_api_keys: true
  api_keys:
    - key: file:{missing_key_path}
      profile: strict
"#
    ));

    let startup = tokio::time::timeout(
        Duration::from_secs(2),
        pokrov_runtime::bootstrap::run(pokrov_runtime::bootstrap::BootstrapArgs { config_path }),
    )
    .await
    .expect("startup should resolve quickly");

    let error = startup.expect_err("runtime startup must fail when strict api key resolution is enabled");
    assert!(error.to_string().contains("failed to resolve"));
}

#[tokio::test]
async fn runtime_with_unresolved_provider_key_starts_when_strict_resolution_disabled() {
    let missing_provider_key_path = format!(
        "/tmp/pokrov-missing-provider-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    let config_path = write_temp_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
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
        api_key: file:{missing_provider_key_path}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start when strict provider key resolution is disabled");
    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn runtime_with_unresolved_provider_key_fails_when_strict_provider_resolution_enabled() {
    let missing_provider_key_path = format!(
        "/tmp/pokrov-missing-provider-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    let config_path = write_temp_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 1000
security:
  fail_on_unresolved_provider_keys: true
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: file:{missing_provider_key_path}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#
    ));

    let startup = tokio::time::timeout(
        Duration::from_secs(2),
        pokrov_runtime::bootstrap::run(pokrov_runtime::bootstrap::BootstrapArgs { config_path }),
    )
    .await
    .expect("startup should resolve quickly");

    let error =
        startup.expect_err("runtime startup must fail when strict provider key resolution is enabled");
    assert!(error
        .to_string()
        .contains("failed to resolve 1 llm provider key binding(s)"));
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
