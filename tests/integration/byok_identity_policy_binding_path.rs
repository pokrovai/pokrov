use std::time::Duration;

use reqwest::StatusCode;

use crate::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};
use crate::mcp_test_support::{start_mock_mcp_server, MockMcpMode};

#[tokio::test]
async fn identity_binding_controls_profile_selection() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;
    let mcp_server = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({"result": {"content": {"text": "ok"}}}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-key");
    let provider_key_path = write_key_file("provider-static-key");
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
    - key: file:{gateway_key}
      profile: strict
auth:
  upstream_auth_mode: static
identity:
  resolution_order:
    - x_pokrov_client_id
  profile_bindings:
    tenant-a: minimal
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 5000
    output_sanitization: false
  servers:
    - id: repo-tools
      endpoint: {mcp_base}
      enabled: true
      allowed_tools:
        - read_file
      blocked_tools: []
      tools:
        read_file:
          enabled: true
"#,
        gateway_key = gateway_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
        mcp_base = mcp_server.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response_a = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-a")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response_a.status(), StatusCode::OK);
    let body_a: serde_json::Value = response_a.json().await.expect("json body expected");
    assert_eq!(body_a["pokrov"]["profile"], "minimal");

    let response_b = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-b")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response_b.status(), StatusCode::OK);
    let body_b: serde_json::Value = response_b.json().await.expect("json body expected");
    assert_eq!(body_b["pokrov"]["profile"], "strict");

    let mcp_response_a = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-a")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"path": "src/lib.rs"},
            "metadata": {}
        }))
        .send()
        .await
        .expect("mcp request should complete");
    assert_eq!(mcp_response_a.status(), StatusCode::OK);
    let mcp_body_a: serde_json::Value = mcp_response_a.json().await.expect("mcp json body expected");
    assert_eq!(mcp_body_a["pokrov"]["profile"], "minimal");

    let mcp_response_b = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .header("x-pokrov-client-id", "tenant-b")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"path": "src/lib.rs"},
            "metadata": {}
        }))
        .send()
        .await
        .expect("mcp request should complete");
    assert_eq!(mcp_response_b.status(), StatusCode::OK);
    let mcp_body_b: serde_json::Value = mcp_response_b.json().await.expect("mcp json body expected");
    assert_eq!(mcp_body_b["pokrov"]["profile"], "strict");

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
    mcp_server.shutdown().await;
}

#[tokio::test]
async fn gateway_auth_subject_binding_controls_profile_selection() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
    })
    .await;
    let mcp_server = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({"result": {"content": {"text": "ok"}}}),
    })
    .await;

    let gateway_key_path = write_key_file("gateway-key");
    let provider_key_path = write_key_file("provider-static-key");
    let gateway_subject = "gw_8cb1c6bc8952bb3def9c7ff05b13fafc".to_string();
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
    - key: file:{gateway_key}
      profile: strict
auth:
  upstream_auth_mode: static
identity:
  resolution_order:
    - gateway_auth_subject
  profile_bindings:
    {gateway_subject}: minimal
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
mcp:
  defaults:
    profile_id: strict
    upstream_timeout_ms: 5000
    output_sanitization: false
  servers:
    - id: repo-tools
      endpoint: {mcp_base}
      enabled: true
      allowed_tools:
        - read_file
      blocked_tools: []
      tools:
        read_file:
          enabled: true
"#,
        gateway_key = gateway_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
        mcp_base = mcp_server.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/chat/completions", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["pokrov"]["profile"], "minimal");

    let mcp_response = client
        .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
        .header("authorization", "Bearer gateway-key")
        .json(&serde_json::json!({
            "server": "repo-tools",
            "tool": "read_file",
            "arguments": {"path": "src/lib.rs"},
            "metadata": {}
        }))
        .send()
        .await
        .expect("mcp request should complete");
    assert_eq!(mcp_response.status(), StatusCode::OK);
    let mcp_body: serde_json::Value = mcp_response.json().await.expect("mcp json body expected");
    assert_eq!(mcp_body["pokrov"]["profile"], "minimal");

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
    mcp_server.shutdown().await;
}
