use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{
    MockMcpMode, start_mock_mcp_server, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn mcp_partial_argument_match_returns_single_deny_and_allows_corrected_retry() {
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
            required_keys:
              - path
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

    let denied = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer mcp-test-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {
                "path": "src/lib.rs",
                "command": "cat /etc/passwd"
            },
            "metadata": {"profile": "strict"}
        }))
        .send()
        .await
        .expect("denied request should complete");

    assert_eq!(denied.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let denied_body: serde_json::Value = denied.json().await.expect("json body expected");
    assert_eq!(denied_body["allowed"], serde_json::json!(false));
    assert_eq!(
        denied_body["error"]["code"],
        serde_json::json!("argument_validation_failed")
    );
    assert_eq!(upstream.request_count(), 0);

    let corrected = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer mcp-test-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {
                "path": "src/lib.rs"
            },
            "metadata": {"profile": "strict"}
        }))
        .send()
        .await
        .expect("corrected retry should complete");

    assert_eq!(corrected.status(), StatusCode::OK);
    assert_eq!(upstream.request_count(), 1);

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
