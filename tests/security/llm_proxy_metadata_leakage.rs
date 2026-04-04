use std::time::Duration;

use reqwest::StatusCode;

use super::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn llm_proxy_responses_remain_metadata_only_without_raw_leakage_fields() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "token sk-test-12345678"},
                "finish_reason": "stop"
            }]
        }),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
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
        secrets: redact
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
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      output_sanitization: true
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: true
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
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    let serialized = serde_json::to_string(&body).expect("response should serialize");

    assert!(!serialized.contains("raw_prompt"));
    assert!(!serialized.contains("raw_response"));
    assert!(!serialized.contains("raw_rule_hits"));
    assert!(!serialized.contains("sk-test-12345678"));

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn llm_proxy_unknown_model_error_remains_metadata_only() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"id":"x","object":"chat.completion","choices":[]}),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
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
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      aliases: [openai/gpt-4o-mini]
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
response_envelope:
  pokrov_metadata:
    mode: suppressed
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
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "unknown/model",
            "stream": false,
            "messages": [{"role": "user", "content": "token sk-test-12345678"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    let serialized = serde_json::to_string(&body).expect("response should serialize");
    assert_eq!(body["error"]["code"], "model_not_routed");
    assert!(body.get("pokrov").is_none());
    assert!(!serialized.contains("sk-test-12345678"));
    assert_eq!(provider.request_count(), 0);

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}
