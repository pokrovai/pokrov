//! Shared fixture for live Chat Completions tool-argument restoration tests.
//!
//! One quote-bearing request fragment is tokenized by a custom `replace` rule,
//! so the mock provider captures the deterministic token and tests can embed
//! the same token in `choices[].delta.tool_calls[].function.arguments`.
//! Restoring the fragment inside a JSON string value exercises inner-JSON
//! escaping as well as SSE transport framing.

use std::{path::Path, time::Duration};

use pokrov_core::rehydrate::{RehydrationMap, TokenDeriver};
use pokrov_runtime::bootstrap::RuntimeHandle;
use serde_json::{json, Value};

use crate::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderHandle, MockProviderMode,
};

pub const REHYDRATION_KEY: &str = "pokrov-sse-tool-argument-test-key";
pub const RUNTIME_API_KEY: &str = "llm-test-key";
pub const PROVIDER_API_KEY: &str = "provider-test-key";
pub const TOOL_MODEL: &str = "gpt-4o-mini";
pub const TOOL_FRAGMENT: &str = "acme\"corp";
pub const TOOL_EVENT_ID: &str = "chatcmpl-live-tool";
pub const TOOL_CALL_ID: &str = "call_live_0";
pub const TOOL_NAME: &str = "lookup";
pub const CREATED_AT: u64 = 1_700_000_000;
pub const DONE_EVENT: &str = "data: [DONE]\n\n";

/// Derived token for the fixture fragment under the runtime key.
pub fn token_for(fragment: &str) -> String {
    let deriver = TokenDeriver::new(REHYDRATION_KEY.as_bytes());
    let mut map = RehydrationMap::new();
    map.token_for(&deriver, fragment)
}

/// Runtime config with the reversible input rule and a live-only route.
pub fn runtime_config(
    runtime_key: &Path,
    provider_key: &Path,
    rehydration_key: &Path,
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
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.tool_marker
          category: custom
          pattern: 'acme"corp'
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

/// Streaming Chat Completions request that tokenizes the fixture fragment.
pub fn streaming_tool_request() -> Value {
    json!({
        "model": TOOL_MODEL,
        "stream": true,
        "messages": [{
            "role": "user",
            "content": format!("tokenize {TOOL_FRAGMENT} here")
        }],
        "metadata": {"profile": "custom"}
    })
}

/// One complete SSE data event for a JSON payload.
pub fn sse_data(payload: &Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(payload).expect("SSE payload should serialize"))
}

/// Chat chunk carrying one tool-argument delta for choice and tool index 0.
pub fn tool_argument_chunk(arguments: &str) -> Value {
    json!({
        "id": TOOL_EVENT_ID,
        "object": "chat.completion.chunk",
        "created": CREATED_AT,
        "model": TOOL_MODEL,
        "choices": [{
            "index": 0,
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": TOOL_CALL_ID,
                    "type": "function",
                    "function": {"name": TOOL_NAME, "arguments": arguments}
                }]
            },
            "finish_reason": null
        }]
    })
}

/// Complete SSE event carrying one tool-argument delta.
pub fn tool_arguments_event(arguments: &str) -> String {
    sse_data(&tool_argument_chunk(arguments))
}

/// Chat chunk that finishes the tool-calling choice without new arguments.
pub fn tool_finish_chunk() -> Value {
    json!({
        "id": TOOL_EVENT_ID,
        "object": "chat.completion.chunk",
        "created": CREATED_AT,
        "model": TOOL_MODEL,
        "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]
    })
}

/// Complete SSE event that finishes the tool-calling choice.
pub fn tool_finish_event() -> String {
    sse_data(&tool_finish_chunk())
}

/// Raw `data:` payloads in emission order, including `[DONE]`.
pub fn sse_payloads(body: &str) -> Vec<String> {
    body.split("\n\n")
        .flat_map(|frame| frame.lines())
        .filter_map(|line| line.strip_prefix("data:").map(|value| value.trim_start().to_string()))
        .collect()
}

/// Parsed JSON payloads in emission order, excluding the `[DONE]` marker.
pub fn json_payloads(body: &str) -> Vec<Value> {
    sse_payloads(body)
        .into_iter()
        .filter(|payload| payload != "[DONE]")
        .map(|payload| {
            serde_json::from_str(&payload)
                .unwrap_or_else(|error| panic!("SSE payload must stay valid JSON: {error}"))
        })
        .collect()
}

/// Concatenates every tool-call argument fragment in emission order.
pub fn assembled_tool_arguments(body: &str) -> String {
    let mut arguments = String::new();
    for payload in json_payloads(body) {
        let Some(choices) = payload.get("choices").and_then(Value::as_array) else {
            continue;
        };
        for choice in choices {
            let Some(calls) = choice
                .get("delta")
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for call in calls {
                if let Some(fragment) = call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(Value::as_str)
                {
                    arguments.push_str(fragment);
                }
            }
        }
    }
    arguments
}

/// Value of an unlabeled Prometheus counter line, if the series is present.
/// A labeled sample under the exact counter name and a duplicate sample are
/// rejected before any value is accepted.
pub fn counter_value(metrics: &str, name: &str) -> Option<u64> {
    let mut value = None;
    for line in metrics.lines() {
        let Some(rest) = line.strip_prefix(name) else { continue };
        if !rest.starts_with(' ') {
            assert!(!rest.starts_with('{'), "counter {name} must remain unlabeled, line: {line}");
            continue;
        }
        assert!(!rest.contains('{'), "counter {name} must remain unlabeled, line: {line}");
        assert!(value.is_none(), "counter {name} must appear exactly once, line: {line}");
        value = Some(rest.trim().parse().unwrap_or_else(|_| {
            panic!("counter {name} must render an integer value, line: {line}")
        }));
    }
    value
}

/// A started mock provider and runtime wired through the live Chat route.
pub struct LiveChatHarness {
    pub provider: MockProviderHandle,
    pub runtime: RuntimeHandle,
    pub client: reqwest::Client,
    pub token: String,
}

impl LiveChatHarness {
    /// Sends the streaming request that tokenizes the fixture fragment.
    pub async fn post_streaming_tool_request(&self) -> reqwest::Response {
        self.client
            .post(format!("{}/v1/chat/completions", self.runtime.base_url()))
            .header("authorization", format!("Bearer {RUNTIME_API_KEY}"))
            .json(&streaming_tool_request())
            .send()
            .await
            .expect("streaming request should succeed")
    }

    /// Scrapes the runtime's Prometheus endpoint.
    pub async fn metrics_body(&self) -> String {
        let response = self
            .client
            .get(format!("{}/metrics", self.runtime.base_url()))
            .send()
            .await
            .expect("metrics request should succeed");
        assert_eq!(response.status(), reqwest::StatusCode::OK, "metrics must be available");
        response.text().await.expect("metrics body should be readable")
    }

    /// Stops the runtime and the mock provider.
    pub async fn shutdown(self) {
        self.runtime.shutdown().await.expect("runtime should stop cleanly");
        self.provider.shutdown().await;
    }
}

/// Starts a fresh runtime backed by the given mock provider mode.
pub async fn start_live_chat_harness(mode: MockProviderMode) -> LiveChatHarness {
    let provider = start_mock_provider(mode).await;
    let runtime_key_path = write_key_file(RUNTIME_API_KEY);
    let provider_key_path = write_key_file(PROVIDER_API_KEY);
    let rehydration_key_path = write_key_file(REHYDRATION_KEY);
    let config_path = write_runtime_config(&runtime_config(
        &runtime_key_path,
        &provider_key_path,
        &rehydration_key_path,
        &provider.base_url,
    ));

    let runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client should build");

    LiveChatHarness { provider, runtime, client, token: token_for(TOOL_FRAGMENT) }
}
