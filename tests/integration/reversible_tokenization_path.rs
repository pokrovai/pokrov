use std::time::Duration;

use pokrov_core::rehydrate::{RehydrationMap, TokenDeriver};
use reqwest::StatusCode;

use super::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
};
use super::mcp_test_support::{start_mock_mcp_server, MockMcpMode};

const REHYDRATION_KEY: &str = "pokrov-rehydration-test-key";
const ORG_MARKER: &str = "acme-corp";

fn token_for(fragment: &str) -> String {
    let deriver = TokenDeriver::new(REHYDRATION_KEY.as_bytes());
    let mut map = RehydrationMap::new();
    map.token_for(&deriver, fragment)
}

fn llm_config(
    runtime_key: &std::path::Path,
    provider_key: &std::path::Path,
    rehydration_key: &std::path::Path,
    provider_base: &str,
) -> String {
    format!(
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
  rehydration_key: file:{rehydration_key}
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
        secrets: mask
        pii: redact
        corporate_markers: mask
        custom: redact
      mask_visible_suffix: 4
    custom:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.org_marker
          category: custom
          pattern: "acme-corp"
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
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
      output_sanitization: false
      enabled: true
    - model: gpt-4o-mini-buffered
      provider_id: openai
      output_sanitization: true
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        runtime_key = runtime_key.display(),
        provider_key = provider_key.display(),
        rehydration_key = rehydration_key.display(),
        provider_base = provider_base,
    )
}

#[tokio::test]
async fn llm_json_round_trip_tokenizes_upstream_and_restores_client_response() {
    let token = token_for(ORG_MARKER);
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": format!("use {token}::init()")},
                "finish_reason": "stop"
            }]
        }),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
    let config_path = write_runtime_config(&llm_config(
        &runtime_key_path,
        &provider_key_path,
        &rehydration_key_path,
        &provider.base_url,
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
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{
                "role": "user",
                "content": "refactor acme-corp-utils crate"
            }],
            "metadata": {"profile": "custom"}
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    let serialized = serde_json::to_string(&body).expect("response should serialize");
    assert!(serialized.contains(ORG_MARKER), "client must see restored original");
    assert!(!serialized.contains("__PKV_"), "client must not see raw tokens");

    let captured = provider.captured_requests().await;
    assert_eq!(captured.len(), 1);
    let captured_text =
        serde_json::to_string(&captured[0]).expect("captured request should serialize");
    assert!(captured_text.contains(&token), "upstream must see the deterministic token");
    assert!(!captured_text.contains(ORG_MARKER), "upstream must not see the raw marker");

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn llm_sse_passthrough_restores_tokens_without_output_sanitization() {
    let token = token_for(ORG_MARKER);
    let provider = start_mock_provider(MockProviderMode::Sse {
        status: 200,
        body: format!(
            "data: {{\"id\":\"c1\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"use {token}::init\"}}}}]}}\n\ndata: {{\"id\":\"c1\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
        ),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
    let config_path = write_runtime_config(&llm_config(
        &runtime_key_path,
        &provider_key_path,
        &rehydration_key_path,
        &provider.base_url,
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
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": true,
            "messages": [{
                "role": "user",
                "content": "refactor acme-corp-utils crate"
            }],
            "metadata": {"profile": "custom"}
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");
    assert!(body.contains(ORG_MARKER), "passthrough stream must restore originals");
    assert!(!body.contains("__PKV_"), "stream must not leak tokens to the client");

    let captured = provider.captured_requests().await;
    let captured_text =
        serde_json::to_string(&captured[0]).expect("captured request should serialize");
    assert!(!captured_text.contains(ORG_MARKER));

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn llm_sse_passthrough_restores_token_split_across_http_chunks() {
    let token = token_for(ORG_MARKER);
    // Split the token mid-hex so no single HTTP chunk contains it whole.
    let (head, tail) = token.split_at(token.len() / 2);
    let provider = start_mock_provider(MockProviderMode::SseChunked {
        status: 200,
        delay_ms: 30,
        chunks: vec![
            format!("data: {{\"id\":\"c1\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"use {head}"),
            format!("{tail}::init\"}}}}]}}\n\ndata: [DONE]\n\n"),
        ],
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
    let config_path = write_runtime_config(&llm_config(
        &runtime_key_path,
        &provider_key_path,
        &rehydration_key_path,
        &provider.base_url,
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
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": true,
            "messages": [{
                "role": "user",
                "content": "refactor acme-corp-utils crate"
            }],
            "metadata": {"profile": "custom"}
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");
    assert!(body.contains(ORG_MARKER), "split token must be restored after both chunks arrive");
    assert!(!body.contains("__PKV_"), "client must not see raw tokens");

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn llm_sse_buffered_path_restores_tokens_after_output_policy() {
    let token = token_for(ORG_MARKER);
    let provider = start_mock_provider(MockProviderMode::Sse {
        status: 200,
        body: format!(
            "data: {{\"id\":\"c1\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"use {token}::init\"}}}}]}}\n\ndata: {{\"id\":\"c1\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
        ),
    })
    .await;

    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
    let config_path = write_runtime_config(&llm_config(
        &runtime_key_path,
        &provider_key_path,
        &rehydration_key_path,
        &provider.base_url,
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
        .header("authorization", "Bearer llm-test-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini-buffered",
            "stream": true,
            "messages": [{
                "role": "user",
                "content": "refactor acme-corp-utils crate"
            }],
            "metadata": {"profile": "custom"}
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");
    assert!(body.contains(ORG_MARKER), "buffered stream must restore originals");
    assert!(!body.contains("__PKV_"), "stream must not leak tokens to the client");

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

#[tokio::test]
async fn missing_rehydration_key_fails_config_validation() {
    let runtime_key_path = write_key_file("llm-test-key");
    let provider_key_path = write_key_file("provider-test-key");
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
        secrets: mask
        pii: redact
        corporate_markers: mask
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: strict.org_marker
          category: custom
          pattern: "acme-corp"
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
    custom:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
      mask_visible_suffix: 4
llm:
  providers:
    - id: openai
      base_url: http://127.0.0.1:1/v1
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
"#,
        runtime_key = runtime_key_path.display(),
        provider_key = provider_key_path.display(),
    ));

    let result = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path).await;
    assert!(result.is_err(), "marker rule without rehydration_key must fail closed");
}

#[tokio::test]
async fn mcp_sanitize_arguments_tokenizes_upstream_and_restores_output() {
    let token = token_for(ORG_MARKER);
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({
            "result": {
                "content": {
                    "text": format!("read {token}/src/lib.rs exposing secret-token-abc123"),
                    "count": 1
                }
            }
        }),
    })
    .await;

    let runtime_key_path = write_key_file("mcp-test-key");
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
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
      profile: custom
sanitization:
  enabled: true
  default_profile: custom
  rehydration_key: file:{rehydration_key}
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
        secrets: mask
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
    custom:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.org_marker
          category: custom
          pattern: "acme-corp"
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
        - id: custom.leaked_token
          category: custom
          pattern: "secret-token-[a-z0-9]+"
          action: redact
          priority: 100
          enabled: true
mcp:
  defaults:
    profile_id: custom
    upstream_timeout_ms: 5000
    output_sanitization: true
    sanitize_arguments: true
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
        rehydration_key = rehydration_key_path.display(),
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
            "arguments": {"path": "acme-corp/src/lib.rs"},
            "metadata": {"profile": "custom"}
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");
    let serialized = serde_json::to_string(&body).expect("response should serialize");
    assert!(serialized.contains(ORG_MARKER), "tool output must restore request-originated values");
    assert!(!serialized.contains("__PKV_"), "client must not see raw tokens");
    assert!(
        !serialized.contains("secret-token-abc123"),
        "output-native secrets must remain destructively sanitized"
    );

    let captured = upstream.captured_requests().await;
    let captured_text =
        serde_json::to_string(&captured[0]).expect("captured request should serialize");
    assert!(captured_text.contains(&token), "upstream tool must see the token");
    assert!(!captured_text.contains(ORG_MARKER), "upstream must not see the raw marker");

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}

#[tokio::test]
async fn mcp_arguments_pass_through_when_sanitize_arguments_disabled() {
    let upstream = start_mock_mcp_server(MockMcpMode::Json {
        status: 200,
        body: serde_json::json!({
            "result": {"content": {"text": "ok", "count": 1}}
        }),
    })
    .await;

    let runtime_key_path = write_key_file("mcp-test-key");
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
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
      profile: custom
sanitization:
  enabled: true
  default_profile: custom
  rehydration_key: file:{rehydration_key}
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
        secrets: mask
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
    custom:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.org_marker
          category: custom
          pattern: "acme-corp"
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
mcp:
  defaults:
    profile_id: custom
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
        rehydration_key = rehydration_key_path.display(),
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
            "arguments": {"path": "acme-corp/src/lib.rs"},
            "metadata": {"profile": "custom"}
        }))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let captured = upstream.captured_requests().await;
    let captured_text =
        serde_json::to_string(&captured[0]).expect("captured request should serialize");
    assert!(
        captured_text.contains(ORG_MARKER),
        "arguments must reach upstream verbatim when sanitize_arguments is off"
    );

    handle.shutdown().await.expect("shutdown should succeed");
    upstream.shutdown().await;
}
