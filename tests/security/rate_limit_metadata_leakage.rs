use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{start_mock_provider, write_key_file, write_runtime_config, MockProviderMode};

#[tokio::test]
async fn rate_limit_response_stays_metadata_only() {
    let mock_provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"id":"resp-1","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"ok"}}]}),
    })
    .await;
    let api_key_path = write_key_file("security-hardening-key");
    let provider_key_path = write_key_file("security-provider-key");

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
  grace_period_ms: 600
security:
  api_keys:
    - key: file:{api_key_path}
      profile: strict
rate_limit:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      requests_per_minute: 1
      token_units_per_minute: 100
      burst_multiplier: 1.0
      enforcement_mode: enforce
llm:
  providers:
    - id: openai
      base_url: {base_url}
      auth:
        api_key: file:{provider_key_path}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        api_key_path = api_key_path.display(),
        provider_key_path = provider_key_path.display(),
        base_url = mock_provider.base_url
    ));

    let runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let payload = serde_json::json!({
        "model":"gpt-4o-mini",
        "messages":[{"role":"user","content":"secret should not leak sk-prod-123"}]
    });

    let _ = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .bearer_auth("security-hardening-key")
        .header("x-pokrov-client-id", "tenant-sensitive-id")
        .json(&payload)
        .send()
        .await
        .expect("first request should complete");

    let blocked = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .bearer_auth("security-hardening-key")
        .header("x-pokrov-client-id", "tenant-sensitive-id")
        .json(&payload)
        .send()
        .await
        .expect("second request should complete");
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    let text = blocked.text().await.expect("response body should be readable");
    assert!(text.contains("rate_limit_exceeded"));
    assert!(!text.contains("sk-prod-123"));
    assert!(!text.contains("messages"));
    assert!(!text.contains("tenant-sensitive-id"));

    runtime.shutdown().await.expect("runtime should stop cleanly");
    mock_provider.shutdown().await;
}
