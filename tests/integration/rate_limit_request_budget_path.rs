use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{start_mock_provider, write_key_file, write_runtime_config, MockProviderMode};

#[tokio::test]
async fn returns_predictable_429_when_request_budget_is_exhausted() {
    let mock_provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"id":"resp-1","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"ok"}}]}),
    })
    .await;
    let api_key_path = write_key_file("hardening-client-key");
    let provider_key_path = write_key_file("hardening-provider-key");

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
      token_units_per_minute: 10000
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

    let request_body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [{"role":"user","content":"hello"}]
    });

    let first = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .bearer_auth("hardening-client-key")
        .json(&request_body)
        .send()
        .await
        .expect("first request should complete");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .bearer_auth("hardening-client-key")
        .json(&request_body)
        .send()
        .await
        .expect("second request should complete");
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

    let body: serde_json::Value = second.json().await.expect("429 body should parse");
    assert_eq!(body["error"]["code"], "rate_limit_exceeded");
    assert!(body["retry_after_ms"].as_u64().unwrap_or(0) > 0);
    assert!(body["limit"].as_u64().unwrap_or(0) >= 1);
    assert!(body["reset_at"].as_u64().unwrap_or(0) > 0);

    runtime.shutdown().await.expect("runtime should stop cleanly");
    mock_provider.shutdown().await;
}
