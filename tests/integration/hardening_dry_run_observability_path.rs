use std::time::Duration;

use reqwest::StatusCode;

use super::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
};
use super::mcp_test_support::{start_mock_mcp_server, MockMcpMode};

#[tokio::test]
async fn llm_dry_run_rate_limit_is_observable_without_blocking_client() {
    let mock_provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "resp-1",
            "object": "chat.completion",
            "choices": [{"message": {"role": "assistant", "content": "ok"}}]
        }),
    })
    .await;

    let api_key_path = write_key_file("dry-run-client-key");
    let provider_key_path = write_key_file("dry-run-provider-key");

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
      token_units_per_minute: 1000
      burst_multiplier: 1.0
      enforcement_mode: dry_run
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
        base_url = mock_provider.base_url,
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
        "messages": [{"role": "user", "content": "hello"}]
    });

    let first = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .bearer_auth("dry-run-client-key")
        .json(&request_body)
        .send()
        .await
        .expect("first request should complete");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .bearer_auth("dry-run-client-key")
        .json(&request_body)
        .send()
        .await
        .expect("second request should complete");
    assert_eq!(second.status(), StatusCode::OK);

    let metrics = client
        .get(format!("{}/metrics", runtime.base_url()))
        .send()
        .await
        .expect("metrics should respond")
        .text()
        .await
        .expect("metrics payload should decode");
    assert!(metrics.contains("pokrov_rate_limit_events_total"));
    assert!(metrics.contains("decision=\"dry_run\""));
    assert!(metrics.contains("route=\"/v1/chat/completions\""));

    runtime.shutdown().await.expect("runtime should stop cleanly");
    mock_provider.shutdown().await;
}

#[tokio::test]
async fn mcp_dry_run_token_limit_is_observable_without_blocking_client() {
    let mock_mcp = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({"content": {"ok": true}}),
    })
    .await;

    let api_key_path = write_key_file("dry-run-mcp-key");

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
      requests_per_minute: 100
      token_units_per_minute: 5
      burst_multiplier: 1.0
      enforcement_mode: dry_run
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 5000
    output_sanitization: false
  servers:
    - id: repo-tools
      endpoint: {endpoint}
      enabled: true
      allowed_tools:
        - read_file
"#,
        api_key_path = api_key_path.display(),
        endpoint = mock_mcp.base_url,
    ));

    let runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let request_body = serde_json::json!({
        "server": "repo-tools",
        "tool": "read_file",
        "arguments": {"path": "src/main.rs"},
        "metadata": {"transport": "http_json", "variant": "tool_call"}
    });

    let first = client
        .post(format!("{}/v1/mcp/tool-call", runtime.base_url()))
        .bearer_auth("dry-run-mcp-key")
        .json(&request_body)
        .send()
        .await
        .expect("first request should complete");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .post(format!("{}/v1/mcp/tool-call", runtime.base_url()))
        .bearer_auth("dry-run-mcp-key")
        .json(&request_body)
        .send()
        .await
        .expect("second request should complete");
    assert_eq!(second.status(), StatusCode::OK);

    assert_eq!(mock_mcp.request_count(), 2);

    let metrics = client
        .get(format!("{}/metrics", runtime.base_url()))
        .send()
        .await
        .expect("metrics should respond")
        .text()
        .await
        .expect("metrics payload should decode");
    assert!(metrics.contains("pokrov_rate_limit_events_total"));
    assert!(metrics.contains("decision=\"dry_run\""));
    assert!(metrics.contains("route=\"/v1/mcp/tool-call\""));

    runtime.shutdown().await.expect("runtime should stop cleanly");
    mock_mcp.shutdown().await;
}
