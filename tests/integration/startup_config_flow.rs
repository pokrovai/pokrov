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
async fn runtime_rejects_passthrough_mode_without_gateway_api_keys() {
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
  grace_period_ms: 6000
auth:
  upstream_auth_mode: passthrough
security:
  api_keys: []
"#,
    );

    let startup = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path).await;
    let error = match startup {
        Ok(_) => panic!("invalid passthrough config must fail startup"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("passthrough_requires_api_key_gateway_auth"));
}

#[tokio::test]
async fn runtime_accepts_passthrough_mode_without_gateway_api_keys_in_mesh_mtls() {
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
  grace_period_ms: 6000
auth:
  upstream_auth_mode: passthrough
  gateway_auth_mode: mesh_mtls
  mesh:
    identity_header: x-forwarded-client-cert
    require_header: true
security:
  api_keys: []
"#,
    );

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("mesh mTLS passthrough should start without gateway api keys");

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn runtime_rejects_passthrough_mesh_mode_when_identity_header_is_optional() {
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
  grace_period_ms: 6000
auth:
  upstream_auth_mode: passthrough
  gateway_auth_mode: mesh_mtls
  mesh:
    identity_header: x-forwarded-client-cert
    require_header: false
security:
  api_keys: []
"#,
    );

    let startup = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path).await;
    let error = match startup {
        Ok(_) => panic!("invalid mesh passthrough config must fail startup"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("passthrough_requires_mesh_identity_header"));
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
        pokrov_runtime::bootstrap::run(pokrov_runtime::bootstrap::BootstrapArgs {
            config_path: Some(config_path),
            release_evidence_output: None,
            release_id: None,
            evidence_artifacts: Vec::new(),
        }),
    )
    .await
    .expect("startup should resolve quickly");

    let error =
        startup.expect_err("runtime startup must fail when strict api key resolution is enabled");
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
        pokrov_runtime::bootstrap::run(pokrov_runtime::bootstrap::BootstrapArgs {
            config_path: Some(config_path),
            release_evidence_output: None,
            release_id: None,
            evidence_artifacts: Vec::new(),
        }),
    )
    .await
    .expect("startup should resolve quickly");

    let error = startup
        .expect_err("runtime startup must fail when strict provider key resolution is enabled");
    assert!(error.to_string().contains("failed to resolve 1 llm provider key binding(s)"));
}

#[tokio::test]
async fn runtime_rejects_alias_conflict_configuration_on_startup() {
    let runtime_key_path = format!(
        "/tmp/pokrov-runtime-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    std::fs::write(&runtime_key_path, "llm-test-key").expect("runtime key file must be created");
    let provider_key_path = format!(
        "/tmp/pokrov-provider-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    std::fs::write(&provider_key_path, "provider-key").expect("provider key file must be created");

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
    - key: file:{runtime_key_path}
      profile: strict
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: file:{provider_key_path}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      aliases: [team/default]
      enabled: true
    - model: gpt-4.1-mini
      provider_id: openai
      aliases: [TEAM/DEFAULT]
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#
    ));

    let startup = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path).await;
    let error = match startup {
        Ok(_) => panic!("alias conflict configuration must fail startup"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("alias_conflict_after_normalization"));
}

#[tokio::test]
async fn runtime_starts_with_deterministic_recognizer_profile_config() {
    let runtime_key_path = format!(
        "/tmp/pokrov-runtime-key-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos()
    );
    std::fs::write(&runtime_key_path, "deterministic-startup-key")
        .expect("runtime key file must be created");

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
    - key: file:{runtime_key_path}
      profile: strict
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      mode_default: enforce
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
      mask_visible_suffix: 4
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
      deterministic_recognizers:
        - id: payment_card
          category: secrets
          action: block
          family_priority: 600
          enabled: true
          patterns:
            - id: pan
              expression: "\\b(?:\\d[ -]*?){{13,16}}\\b"
              base_score: 200
              normalization: alnum_lowercase
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
"#
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start with deterministic recognizer config");
    handle.shutdown().await.expect("shutdown should succeed");
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
