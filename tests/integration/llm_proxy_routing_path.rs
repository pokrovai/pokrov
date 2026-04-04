use std::time::Duration;

use reqwest::StatusCode;

use super::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn llm_proxy_routes_requests_deterministically_by_model() {
    let provider_a = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-a",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "a"}}]
        }),
    })
    .await;
    let provider_b = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-b",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4.1-mini",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "b"}}]
        }),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_a_key_path = write_key_file("provider-a-key");
    let provider_b_key_path = write_key_file("provider-b-key");
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
    - key: file:{runtime_key}
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
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
llm:
  providers:
    - id: openai_a
      base_url: {provider_a_base}
      auth:
        api_key: file:{provider_a_key}
      enabled: true
    - id: openai_b
      base_url: {provider_b_base}
      auth:
        api_key: file:{provider_b_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai_a
      aliases:
        - openai/gpt-4o-mini
      output_sanitization: false
      enabled: true
    - model: gpt-4.1-mini
      provider_id: openai_b
      output_sanitization: false
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        runtime_key = runtime_key_path.display(),
        provider_a_key = provider_a_key_path.display(),
        provider_b_key = provider_b_key_path.display(),
        provider_a_base = provider_a.base_url,
        provider_b_base = provider_b.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response_a = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("model A request should succeed");
    assert_eq!(response_a.status(), StatusCode::OK);

    let response_b = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4.1-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("model B request should succeed");
    assert_eq!(response_b.status(), StatusCode::OK);

    let response_alias = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "OPENAI/GPT-4O-MINI",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("alias request should succeed");
    assert_eq!(response_alias.status(), StatusCode::OK);

    assert_eq!(provider_a.request_count(), 2);
    assert_eq!(provider_b.request_count(), 1);

    handle.shutdown().await.expect("shutdown should succeed");
    provider_a.shutdown().await;
    provider_b.shutdown().await;
}

#[tokio::test]
async fn models_catalog_excludes_disabled_routes_and_providers() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-a",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "a"}}]
        }),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-a-key");
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
    - key: file:{runtime_key}
      profile: strict
sanitization:
  enabled: false
llm:
  providers:
    - id: openai_a
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
    - id: openai_disabled
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: false
  routes:
    - model: gpt-4o-mini
      provider_id: openai_a
      aliases: [openai/gpt-4o-mini]
      enabled: true
    - model: disabled/route
      provider_id: openai_a
      enabled: false
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        runtime_key = runtime_key_path.display(),
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
        .header("authorization", "Bearer llm-test-key")
        .send()
        .await
        .expect("models request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    let ids = body["data"]
        .as_array()
        .expect("data should be array")
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"gpt-4o-mini"));
    assert!(ids.contains(&"openai/gpt-4o-mini"));
    assert!(!ids.contains(&"disabled/route"));

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}
