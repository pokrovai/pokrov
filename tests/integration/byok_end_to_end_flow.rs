use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
};

#[tokio::test]
async fn byok_supports_static_and_passthrough_modes_end_to_end() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-key");
    let provider_key_path = write_key_file("provider-static-key");

    let static_config = write_runtime_config(&format!(
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

    let passthrough_config = write_runtime_config(&format!(
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

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let static_runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(static_config)
        .await
        .expect("runtime should start");
    let static_response = client
        .post(format!("{}/v1/chat/completions", static_runtime.base_url()))
        .header("authorization", "Bearer gateway-key")
        .json(&serde_json::json!({"model":"gpt-4o-mini","messages":[{"role":"user","content":"hello"}]}))
        .send()
        .await
        .expect("request should complete");
    assert_eq!(static_response.status(), StatusCode::OK);
    static_runtime.shutdown().await.expect("shutdown should succeed");

    let passthrough_runtime =
        pokrov_runtime::bootstrap::spawn_runtime_for_tests(passthrough_config)
            .await
            .expect("runtime should start");
    let passthrough_response = client
        .post(format!("{}/v1/chat/completions", passthrough_runtime.base_url()))
        .header("x-pokrov-api-key", "gateway-key")
        .header("authorization", "Bearer provider-byok-key")
        .json(&serde_json::json!({"model":"gpt-4o-mini","messages":[{"role":"user","content":"hello"}]}))
        .send()
        .await
        .expect("request should complete");
    assert_eq!(passthrough_response.status(), StatusCode::OK);
    passthrough_runtime.shutdown().await.expect("shutdown should succeed");

    provider.shutdown().await;
}
