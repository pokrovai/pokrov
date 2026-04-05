use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{
    start_mock_mcp_server, write_key_file, write_runtime_config, MockMcpMode,
};

#[tokio::test]
async fn mcp_validation_block_happens_before_upstream_execution_and_is_metadata_only() {
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
          argument_constraints:
            forbidden_keys:
              - command
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

    let raw_command = "cat /etc/passwd --token secret-value";
    let response = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer mcp-test-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"command": raw_command},
            "metadata": {"profile": "strict"}
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["error"]["code"], serde_json::json!("argument_validation_failed"));
    assert_eq!(upstream.request_count(), 0);

    let serialized = serde_json::to_string(&body).expect("response should serialize");
    assert!(!serialized.contains(raw_command));
    assert!(!serialized.contains("raw_arguments"));

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
