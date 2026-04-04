use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn responses_passthrough_missing_upstream_credential_is_blocked() {
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
        .post(format!("{}/v1/responses", handle.base_url()))
        .header("x-pokrov-api-key", "gateway-byok-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "input": "hello"
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
