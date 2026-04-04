use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn responses_non_stream_happy_path_returns_compatible_envelope() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "ok"},
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
      output_sanitization: false
      enabled: true
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
        .post(format!("{}/v1/responses", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "input": "hello"
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert!(body["request_id"].is_string());
    assert!(body["output"].is_array());
    assert!(body["pokrov"].is_object());

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}
