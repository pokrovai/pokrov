use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn byok_passthrough_blocks_when_provider_credential_is_missing() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-byok-key");
    let provider_key_path = write_key_file("provider-static-fallback");
    let config_path = write_runtime_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 900
security:
  api_keys:
    - key: file:{gateway_key}
      profile: strict
auth:
  upstream_auth_mode: passthrough
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        gateway_key = gateway_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("x-pokrov-api-key", "gateway-byok-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["error"]["code"], "upstream_credential_missing");
    assert_eq!(provider.request_count(), 0);

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn byok_passthrough_allows_models_catalog_without_provider_credential() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-byok-key");
    let provider_key_path = write_key_file("provider-static-fallback");
    let config_path = write_runtime_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 900
security:
  api_keys:
    - key: file:{gateway_key}
      profile: strict
auth:
  upstream_auth_mode: passthrough
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        gateway_key = gateway_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .get(format!("{}/v1/models", handle.base_url()))
        .header("x-pokrov-api-key", "gateway-byok-key")
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    let ids = body["data"]
        .as_array()
        .expect("data should be array")
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"gpt-4o-mini"));
    assert_eq!(provider.request_count(), 0);

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn byok_passthrough_accepts_single_bearer_for_openai_compatible_chat_path() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-byok-key");
    let provider_key_path = write_key_file("provider-static-fallback");
    let config_path = write_runtime_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 900
security:
  api_keys:
    - key: file:{gateway_key}
      profile: strict
auth:
  upstream_auth_mode: passthrough
  allow_single_bearer_passthrough: true
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        gateway_key = gateway_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-byok-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let forwarded_auth = provider.captured_authorization_headers().await;
    assert_eq!(forwarded_auth.len(), 1);
    assert_eq!(forwarded_auth[0].as_deref(), Some("Bearer gateway-byok-key"));

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn byok_passthrough_blocks_single_bearer_without_explicit_opt_in() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-byok-key");
    let provider_key_path = write_key_file("provider-static-fallback");
    let config_path = write_runtime_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 900
security:
  api_keys:
    - key: file:{gateway_key}
      profile: strict
auth:
  upstream_auth_mode: passthrough
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        gateway_key = gateway_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-byok-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["error"]["code"], "passthrough_requires_api_key_gateway_auth");
    assert_eq!(provider.request_count(), 0);

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}
