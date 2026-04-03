use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{
    MockMcpMode, start_mock_mcp_server, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn mcp_allowed_path_sanitizes_upstream_output() {
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({
            "result": {
                "content": {
                    "text": "token secret-token-abc123 is present",
                    "count": 1
                }
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
        secrets: redact
        pii: redact
        corporate_markers: mask
        custom: redact
      mask_visible_suffix: 4
      custom_rules:
        - id: strict.secret_token
          category: custom
          pattern: "(?i)secret-token-[a-z0-9]+"
          action: redact
          priority: 100
          enabled: true
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 5000
    output_sanitization: true
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
          output_sanitization: true
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
    assert_eq!(body["sanitized"], serde_json::json!(true));

    let serialized = serde_json::to_string(&body).expect("response should serialize");
    assert!(!serialized.contains("secret-token-abc123"));

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}

#[tokio::test]
async fn mcp_output_policy_block_returns_output_blocked_reason_and_calls_upstream() {
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({
            "result": {
                "content": {
                    "text": "forbidden token secret-token-abc123",
                    "count": 1
                }
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
        secrets: redact
        pii: redact
        corporate_markers: mask
        custom: redact
      mask_visible_suffix: 4
      custom_rules:
        - id: strict.output_block
          category: custom
          pattern: "(?i)secret-token-[a-z0-9]+"
          action: block
          priority: 100
          enabled: true
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 5000
    output_sanitization: true
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
          output_sanitization: true
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["allowed"], serde_json::json!(false));
    assert_eq!(body["error"]["code"], serde_json::json!("tool_call_blocked"));
    assert_eq!(
        body["error"]["details"]["reason"],
        serde_json::json!("output_blocked")
    );
    assert!(body["error"]["details"]["violation_count"].as_u64().unwrap_or(0) >= 1);

    assert_eq!(upstream.request_count(), 1);

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
