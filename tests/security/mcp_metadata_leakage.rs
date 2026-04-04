use std::time::Duration;

use reqwest::StatusCode;

use super::mcp_test_support::{
    MockMcpMode, start_mock_mcp_server, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn mcp_response_and_errors_remain_metadata_only() {
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({
            "result": {
                "content": {"text": "secret-token-abc123", "flag": true}
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
        .header("x-pokrov-client-id", "tenant-sensitive-id")
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

    let serialized = serde_json::to_string(&body).expect("response should serialize");
    assert!(!serialized.contains("secret-token-abc123"));
    assert!(!serialized.contains("raw_arguments"));
    assert!(!serialized.contains("raw_tool_output"));
    assert!(!serialized.contains("raw_rule_matches"));
    assert!(!serialized.contains("tenant-sensitive-id"));

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
