//! Shared fixtures for buffered Chat Completions output-policy tests.

use std::{path::Path, time::Duration};

use pokrov_core::rehydrate::{event_boundary_end, MAX_EVENT_BUFFER_BYTES};
use pokrov_runtime::bootstrap::RuntimeHandle;
use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};

use crate::{
    llm_proxy_test_support::{
        start_mock_provider, write_key_file, write_runtime_config, MockProviderHandle,
        MockProviderMode,
    },
    sse_tool_argument_test_support::{
        token_for, PROVIDER_API_KEY, REHYDRATION_KEY, RUNTIME_API_KEY, TOOL_CALL_ID, TOOL_EVENT_ID,
        TOOL_FRAGMENT, TOOL_MODEL, TOOL_NAME,
    },
};

pub(super) const POLICY_ROUTE: &str = "tool-buffered-policy";
pub(super) const PLAIN_ROUTE: &str = "tool-buffered-plain";

fn runtime_config(
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
    strict:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.tool_marker
          category: custom
          pattern: 'acme"corp'
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
        - id: custom.policy_marker
          category: custom
          pattern: "rewrite-me"
          action: replace
          replacement: "POLICY-RAN"
          priority: 200
          enabled: true
    custom:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.tool_marker
          category: custom
          pattern: 'acme"corp'
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
        - id: custom.token_mutator
          category: custom
          pattern: "__PKV_"
          action: replace
          replacement: "[POLICY]"
          priority: 200
          enabled: true
    minimal:
      mode_default: enforce
      categories:
        secrets: allow
        pii: allow
        corporate_markers: allow
        custom: allow
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.tool_marker
          category: custom
          pattern: 'acme"corp'
          action: replace
          replacement: "[PKV_TOKEN]"
          priority: 100
          enabled: true
        - id: custom.output_block
          category: custom
          pattern: "block-me"
          action: block
          priority: 200
          enabled: true
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: {policy_route}
      provider_id: openai
      output_sanitization: true
      enabled: true
    - model: {plain_route}
      provider_id: openai
      output_sanitization: false
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
    stream_sanitization_max_buffer_bytes: 4194304
"#,
        runtime_key = runtime_key.display(),
        provider_key = provider_key.display(),
        rehydration_key = rehydration_key.display(),
        provider_base = provider_base,
        policy_route = POLICY_ROUTE,
        plain_route = PLAIN_ROUTE,
    )
}

pub(super) struct BufferedHarness {
    pub(super) provider: MockProviderHandle,
    runtime: RuntimeHandle,
    client: Client,
}

impl BufferedHarness {
    pub(super) async fn start(body: String) -> Self {
        Self::start_with_mode(MockProviderMode::Sse { status: 200, body }).await
    }

    pub(super) async fn start_with_mode(mode: MockProviderMode) -> Self {
        let provider = start_mock_provider(mode).await;
        let runtime_key = write_key_file(RUNTIME_API_KEY);
        let provider_key = write_key_file(PROVIDER_API_KEY);
        let rehydration_key = write_key_file(REHYDRATION_KEY);
        let config_path = write_runtime_config(&runtime_config(
            &runtime_key,
            &provider_key,
            &rehydration_key,
            &provider.base_url,
        ));
        let runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
            .await
            .expect("runtime should start");
        let client =
            Client::builder().timeout(Duration::from_secs(5)).build().expect("client should build");
        Self { provider, runtime, client }
    }

    pub(super) async fn post(&self, model: &str, profile: &str) -> Response {
        self.client
            .post(format!("{}/v1/chat/completions", self.runtime.base_url()))
            .header("authorization", format!("Bearer {RUNTIME_API_KEY}"))
            .json(&request(model, profile))
            .send()
            .await
            .expect("request should complete")
    }

    pub(super) async fn metrics(&self) -> String {
        self.client
            .get(format!("{}/metrics", self.runtime.base_url()))
            .send()
            .await
            .expect("metrics request should succeed")
            .text()
            .await
            .expect("metrics body should be readable")
    }

    pub(super) async fn shutdown(self) {
        self.runtime.shutdown().await.expect("runtime should stop cleanly");
        self.provider.shutdown().await;
    }
}

fn request(model: &str, profile: &str) -> Value {
    json!({
        "model": model,
        "stream": true,
        "messages": [{
            "role": "user",
            "content": format!("please restore {TOOL_FRAGMENT} here")
        }],
        "metadata": {"profile": profile}
    })
}

pub(super) fn sse_event(payload: &str, ending: &str) -> String {
    format!("data: {payload}{ending}{ending}")
}

pub(super) fn argument_event(arguments: &str, choice: u64, tool: u64, ending: &str) -> String {
    let payload = json!({
        "id": TOOL_EVENT_ID,
        "object": "chat.completion.chunk",
        "created": 1_700_000_000,
        "model": TOOL_MODEL,
        "choices": [{
            "index": choice,
            "delta": {
                "tool_calls": [{
                    "index": tool,
                    "id": format!("{TOOL_CALL_ID}-{choice}-{tool}"),
                    "type": "function",
                    "function": {"name": TOOL_NAME, "arguments": arguments}
                }]
            },
            "finish_reason": null
        }]
    });
    sse_event(&payload.to_string(), ending)
}

/// One valid first Chat event whose source frame body (before the delimiter)
/// is exactly the streaming event budget. The compact payload is preserved by
/// policy; only the `data:` prefix normalization grows it by one byte.
pub(super) fn first_frame_at_event_budget(arguments: &str) -> String {
    fn payload(arguments: &str, filler: usize) -> String {
        json!({
            "id": TOOL_EVENT_ID,
            "object": "chat.completion.chunk",
            "created": 1_700_000_000,
            "model": TOOL_MODEL,
            "choices": [{
                "index": 0,
                "delta": {"tool_calls": [{
                    "index": 0,
                    "id": TOOL_CALL_ID,
                    "type": "function",
                    "function": {"name": TOOL_NAME, "arguments": arguments}
                }]},
                "finish_reason": null
            }],
            "filler": "x".repeat(filler)
        })
        .to_string()
    }

    let base_len = payload(arguments, 0).len();
    let filler = MAX_EVENT_BUFFER_BYTES - "data:".len() - base_len;
    let frame = format!("data:{}\n\n", payload(arguments, filler));
    assert_eq!(
        frame.trim_end_matches('\n').len(),
        MAX_EVENT_BUFFER_BYTES,
        "fixture first frame must sit exactly on the source budget"
    );
    frame
}

pub(super) fn finish_event(choice: u64, ending: &str) -> String {
    let payload = json!({
        "id": TOOL_EVENT_ID,
        "object": "chat.completion.chunk",
        "created": 1_700_000_000,
        "model": TOOL_MODEL,
        "choices": [{
            "index": choice,
            "delta": {},
            "finish_reason": "tool_calls"
        }]
    });
    sse_event(&payload.to_string(), ending)
}

pub(super) fn done_event(ending: &str) -> String {
    format!("data: [DONE]{ending}{ending}")
}

fn payloads(body: &str) -> Vec<String> {
    body.split(['\n', '\r'])
        .filter_map(|line| line.strip_prefix("data:").map(|value| value.trim().to_string()))
        .collect()
}

pub(super) fn json_payloads(body: &str) -> Vec<Value> {
    payloads(body)
        .into_iter()
        .filter(|payload| payload != "[DONE]")
        .map(|payload| {
            serde_json::from_str(&payload)
                .unwrap_or_else(|error| panic!("SSE payload must stay valid JSON: {error}"))
        })
        .collect()
}

/// Splits a buffered body into complete frames and the unterminated raw tail.
pub(super) fn split_complete_frames(body: &str) -> (Vec<&str>, &str) {
    let bytes = body.as_bytes();
    let mut frames = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = event_boundary_end(&bytes[cursor..]) {
        let end = cursor + offset;
        frames.push(&body[cursor..end]);
        cursor = end;
    }
    (frames, &body[cursor..])
}

pub(super) fn assembled_arguments(body: &str) -> String {
    let mut assembled = String::new();
    for payload in json_payloads(body) {
        let Some(choices) = payload.get("choices").and_then(Value::as_array) else { continue };
        for choice in choices {
            let Some(calls) = choice
                .get("delta")
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for call in calls {
                if let Some(arguments) = call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(Value::as_str)
                {
                    assembled.push_str(arguments);
                }
            }
        }
    }
    assembled
}

pub(super) fn assembled_arguments_for(body: &str, choice_index: u64, tool_index: u64) -> String {
    let mut assembled = String::new();
    for payload in json_payloads(body) {
        let Some(choices) = payload.get("choices").and_then(Value::as_array) else { continue };
        for choice in choices {
            if choice.get("index").and_then(Value::as_u64) != Some(choice_index) {
                continue;
            }
            let Some(calls) = choice
                .get("delta")
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for call in calls {
                if call.get("index").and_then(Value::as_u64) != Some(tool_index) {
                    continue;
                }
                if let Some(arguments) = call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(Value::as_str)
                {
                    assembled.push_str(arguments);
                }
            }
        }
    }
    assembled
}

pub(super) fn token_stream(ending: &str, token: &str) -> String {
    let split = token.len() / 2;
    let mut body = String::new();
    body.push_str(&argument_event(&format!("{{\"q\":\"{}", &token[..split]), 0, 0, ending));
    body.push_str(&argument_event("{\"note\":\"plain\"}", 0, 1, ending));
    body.push_str(&argument_event(
        &format!("{}\",\"unrelated\":42}}", &token[split..]),
        0,
        0,
        ending,
    ));
    body.push_str(&argument_event("{\"text\":\"choice-one\"}", 1, 0, ending));
    body.push_str(&finish_event(0, ending));
    body.push_str(&finish_event(1, ending));
    body.push_str(&done_event(ending));
    body
}

pub(super) fn plain_stream(ending: &str) -> String {
    let mut body = String::new();
    body.push_str(&argument_event("{\"text\":\"plain-args\"}", 0, 0, ending));
    body.push_str(&finish_event(0, ending));
    body.push_str(&done_event(ending));
    body
}

pub(super) async fn assert_policy_parity(body: String, ending: &str, expect_token: bool) {
    let token = token_for(TOOL_FRAGMENT);
    let harness = BufferedHarness::start(body).await;
    let policy = harness.post(POLICY_ROUTE, "strict").await;
    let plain = harness.post(PLAIN_ROUTE, "strict").await;
    assert_eq!(policy.status(), StatusCode::OK, "policy route must succeed");
    assert_eq!(plain.status(), StatusCode::OK, "plain route must succeed");
    let policy_body = policy.text().await.expect("policy body expected");
    let plain_body = plain.text().await.expect("plain body expected");

    assert_eq!(
        json_payloads(&policy_body),
        json_payloads(&plain_body),
        "parsed payloads must agree for {ending:?} framing"
    );
    assert_eq!(assembled_arguments(&policy_body), assembled_arguments(&plain_body));

    if expect_token {
        let inner: Value = serde_json::from_str(&assembled_arguments_for(&policy_body, 0, 0))
            .expect("restored inner arguments must stay valid JSON");
        assert_eq!(inner["q"], TOOL_FRAGMENT);
        assert_eq!(inner["unrelated"], 42);
        assert_eq!(assembled_arguments_for(&policy_body, 0, 1), "{\"note\":\"plain\"}");
        assert_eq!(assembled_arguments_for(&policy_body, 1, 0), "{\"text\":\"choice-one\"}");
        assert!(!policy_body.contains("__PKV_"));
    } else {
        assert_eq!(assembled_arguments_for(&policy_body, 0, 0), "{\"text\":\"plain-args\"}");
        assert!(!policy_body.contains("__PKV_"));
    }
    assert!(policy_body.contains(&format!("{ending}{ending}")));

    let captured = harness.provider.captured_requests().await;
    assert!(captured[0]["messages"][0]["content"].as_str().expect("content").contains(&token));

    harness.shutdown().await;
}

pub(super) async fn assert_event_budget_parity(body: String, expect_bom: bool) {
    let harness = BufferedHarness::start(body).await;
    let policy = harness.post(POLICY_ROUTE, "strict").await;
    let plain = harness.post(PLAIN_ROUTE, "strict").await;
    assert_eq!(policy.status(), StatusCode::OK, "policy route must succeed");
    assert_eq!(plain.status(), StatusCode::OK, "plain route must succeed");
    let policy_body = policy.text().await.expect("policy body expected");
    let plain_body = plain.text().await.expect("plain body expected");

    if expect_bom {
        for parsed in [&policy_body, &plain_body] {
            assert!(parsed.starts_with('\u{feff}'), "BOM must be forwarded first");
            assert_eq!(parsed.matches('\u{feff}').count(), 1, "exactly one BOM is forwarded");
        }
    }
    let policy_view = policy_body.strip_prefix('\u{feff}').unwrap_or(&policy_body);
    let plain_view = plain_body.strip_prefix('\u{feff}').unwrap_or(&plain_body);
    assert_eq!(json_payloads(policy_view), json_payloads(plain_view));
    assert_eq!(assembled_arguments(policy_view), assembled_arguments(plain_view));
    for view in [policy_view, plain_view] {
        assert!(view.contains("acme"), "the boundary frame must restore");
        assert!(!view.contains("__PKV_"));
        let inner: Value = serde_json::from_str(&assembled_arguments_for(view, 0, 0))
            .expect("inner arguments must stay valid JSON");
        assert_eq!(inner["q"], TOOL_FRAGMENT);
    }

    harness.shutdown().await;
}
