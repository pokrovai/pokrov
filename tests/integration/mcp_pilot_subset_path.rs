use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{
    MockMcpMode, start_mock_mcp_server, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn mcp_pilot_subset_accepts_http_json_and_rejects_unsupported_variant() {
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({"result": {"content": {"ok": true}}}),
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

    let allowed = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer mcp-test-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"path": "src/lib.rs"},
            "metadata": {
                "profile": "strict",
                "transport": "http_json",
                "variant": "tool_call"
            }
        }))
        .send()
        .await
        .expect("allowed request should complete");
    assert_eq!(allowed.status(), StatusCode::OK);

    let unsupported = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer mcp-test-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"path": "src/lib.rs"},
            "metadata": {
                "profile": "strict",
                "transport": "sse"
            }
        }))
        .send()
        .await
        .expect("unsupported request should complete");

    assert_eq!(unsupported.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = unsupported.json().await.expect("json body expected");
    assert_eq!(body["allowed"], serde_json::json!(false));
    assert_eq!(body["error"]["code"], serde_json::json!("unsupported_variant"));

    assert_eq!(upstream.request_count(), 1);

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
