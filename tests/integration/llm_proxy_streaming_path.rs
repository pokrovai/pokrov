use std::time::Duration;

use reqwest::StatusCode;

use super::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
};

#[tokio::test]
async fn llm_proxy_preserves_openai_style_sse_stream_framing() {
    let provider = start_mock_provider(MockProviderMode::Sse {
        status: 200,
        body: "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n"
            .to_string(),
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
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": true,
            "messages": [{"role": "user", "content": "stream"}]
        }))
        .send()
        .await
        .expect("stream request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-request-id").and_then(|value| value.to_str().ok()).is_some());
    assert_eq!(
        response.headers().get("content-type").and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = response.text().await.expect("stream response should be readable");
    assert!(body.contains("data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}"));
    assert!(body.contains("data: [DONE]"));

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}
