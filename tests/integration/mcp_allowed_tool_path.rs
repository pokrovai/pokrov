use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{
    MockMcpMode, start_mock_mcp_server, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn mcp_allowed_tool_call_returns_success_and_executes_upstream() {
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({
            "result": {
                "content": {"text": "ok"},
                "truncated": false
            }
        }),
    })
    .await;

    let runtime_key_path = write_key_file("mcp-test-key");
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
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 5000
    output_sanitization: false
  servers:
    - id: repo-tools
      endpoint: {upstream_base}
      enabled: true
      allowed_tools:
        - read_file
      blocked_tools: []
      tools:
        read_file:
          enabled: true
"#,
        runtime_key = runtime_key_path.display(),
        upstream_base = upstream.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer mcp-test-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"path": "src/lib.rs"},
            "metadata": {"profile": "strict"}
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["allowed"], serde_json::json!(true));
    assert_eq!(body["pokrov"]["profile"], serde_json::json!("strict"));
    assert_eq!(body["pokrov"]["server"], serde_json::json!("repo-tools"));
    assert_eq!(body["pokrov"]["tool"], serde_json::json!("read_file"));

    let captured = upstream.captured_requests().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0]["tool"], serde_json::json!("read_file"));

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
