use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
};

#[tokio::test]
async fn rate_limit_budget_is_isolated_per_client_identity() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-key");
    let provider_key_path = write_key_file("provider-static-key");
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
  upstream_auth_mode: static
identity:
  resolution_order:
    - x_pokrov_client_id
rate_limit:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      requests_per_minute: 1
      token_units_per_minute: 10000
      burst_multiplier: 1.0
      enforcement_mode: enforce
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

    let payload = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": "hello"}]
    });

    let first_a = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-a")
        .json(&payload)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(first_a.status(), StatusCode::OK);

    let second_a = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-a")
        .json(&payload)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(second_a.status(), StatusCode::TOO_MANY_REQUESTS);

    let first_b = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-b")
        .json(&payload)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(first_b.status(), StatusCode::OK);

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn rate_limit_budget_is_shared_for_same_identity_across_gateway_keys() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_a_path = write_key_file("gateway-key-a");
    let gateway_key_b_path = write_key_file("gateway-key-b");
    let provider_key_path = write_key_file("provider-static-key");
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
    - key: file:{gateway_key_a}
      profile: strict
    - key: file:{gateway_key_b}
      profile: strict
auth:
  upstream_auth_mode: static
identity:
  resolution_order:
    - x_pokrov_client_id
rate_limit:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      requests_per_minute: 1
      token_units_per_minute: 10000
      burst_multiplier: 1.0
      enforcement_mode: enforce
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
        gateway_key_a = gateway_key_a_path.display(),
        gateway_key_b = gateway_key_b_path.display(),
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

    let payload = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": "hello"}]
    });

    let first = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key-a")
        .header("x-pokrov-client-id", "tenant-a")
        .json(&payload)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key-b")
        .header("x-pokrov-client-id", "tenant-a")
        .json(&payload)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}
