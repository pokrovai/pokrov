use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{write_key_file, write_runtime_config};

#[tokio::test]
async fn mcp_upstream_unavailable_maps_to_503() {
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
    upstream_timeout_ms: 100
    output_sanitization: false
  servers:
    - id: repo-tools
      endpoint: http://127.0.0.1:1
      enabled: true
      allowed_tools:
        - read_file
      blocked_tools: []
      tools:
        read_file:
          enabled: true
"#,
        runtime_key = runtime_key_path.display(),
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

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["allowed"], serde_json::json!(false));
    assert_eq!(body["error"]["code"], serde_json::json!("upstream_unavailable"));

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn mcp_suppressed_mode_omits_pokrov_metadata_on_error() {
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
response_envelope:
  pokrov_metadata:
    mode: suppressed
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 100
    output_sanitization: false
  servers:
    - id: repo-tools
      endpoint: http://127.0.0.1:1
      enabled: true
      allowed_tools:
        - read_file
      blocked_tools: []
      tools:
        read_file:
          enabled: true
"#,
        runtime_key = runtime_key_path.display(),
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

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert!(body.get("pokrov").is_none());
    assert_eq!(body["error"]["code"], serde_json::json!("upstream_unavailable"));

    handle.shutdown().await.expect("shutdown should succeed");
}
