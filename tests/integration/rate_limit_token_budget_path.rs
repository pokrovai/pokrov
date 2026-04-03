use std::time::Duration;

use reqwest::StatusCode;

use crate::mcp_test_support::{start_mock_mcp_server, write_key_file, write_runtime_config, MockMcpMode};

#[tokio::test]
async fn blocks_mcp_request_when_token_budget_is_exhausted() {
    let mock_mcp = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({"content":{"ok":true}}),
    })
    .await;
    let api_key_path = write_key_file("mcp-hardening-key");

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
      token_units_per_minute: 8
      burst_multiplier: 1.0
      enforcement_mode: enforce
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
        endpoint = mock_mcp.base_url
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
        "arguments": {"path":"src/main.rs"},
        "metadata": {"transport":"http_json","variant":"tool_call"}
    });

    let first = client
        .post(format!("{}/v1/mcp/tool-call", runtime.base_url()))
        .bearer_auth("mcp-hardening-key")
        .json(&request_body)
        .send()
        .await
        .expect("first request should complete");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .post(format!("{}/v1/mcp/tool-call", runtime.base_url()))
        .bearer_auth("mcp-hardening-key")
        .json(&request_body)
        .send()
        .await
        .expect("second request should complete");
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

    let body: serde_json::Value = second.json().await.expect("429 body should parse");
    assert_eq!(body["error"]["code"], "rate_limit_exceeded");
    assert_eq!(body["allowed"], false);

    runtime.shutdown().await.expect("runtime should stop cleanly");
    mock_mcp.shutdown().await;
}
