//! Release-mode latency budget for buffered and passthrough Chat tool-argument
//! restoration. The fixture is generated before timing; every measured request
//! drains the complete SSE body.

use std::time::{Duration, Instant};

use pokrov_runtime::bootstrap::RuntimeHandle;
use reqwest::{Client, StatusCode};
use serde_json::json;

use super::{
    llm_proxy_test_support::{
        start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
    },
    sse_tool_argument_test_support::{
        token_for, PROVIDER_API_KEY, REHYDRATION_KEY, RUNTIME_API_KEY, TOOL_FRAGMENT, TOOL_MODEL,
    },
};

const POLICY_ROUTE: &str = "tool-perf-policy";
const PLAIN_ROUTE: &str = "tool-perf-plain";
const TARGET_BYTES: usize = 32 * 1024;
const WARMUPS: usize = 5;
const SAMPLES: usize = 40;
const P95_BUDGET_MS: u64 = 50;

fn runtime_config(
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
      profile: custom
sanitization:
  enabled: true
  default_profile: custom
  rehydration_key: file:{rehydration_key}
  profiles:
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
    profile_id: custom
    output_sanitization: false
"#,
        runtime_key = runtime_key.display(),
        provider_key = provider_key.display(),
        rehydration_key = rehydration_key.display(),
        provider_base = provider_base,
        policy_route = POLICY_ROUTE,
        plain_route = PLAIN_ROUTE,
    )
}

fn argument_event(arguments: &str, tool: u64) -> String {
    let payload = json!({
        "id": "chatcmpl-perf",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": TOOL_MODEL,
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": tool,
                "id": format!("call_perf_{tool}"),
                "type": "function",
                "function": {"name": "lookup", "arguments": arguments}
            }]},
            "finish_reason": null
        }]
    });
    format!("data: {payload}\n\n")
}

fn finish_event() -> String {
    let payload = json!({
        "id": "chatcmpl-perf",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": TOOL_MODEL,
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "tool_calls"
        }]
    });
    format!("data: {payload}\n\n")
}

/// Builds roughly 32 KiB of interleaved Chat deltas for two tool calls. The
/// main call carries split, whole, and absent tokens; the second call keeps an
/// independent inner document so interleaving exercises per-pair state.
fn traffic_body(token: &str) -> String {
    let split = token.len() / 2;
    let mut body = String::with_capacity(TARGET_BYTES + 1024);
    body.push_str(&argument_event("{\"q\":\"", 0));
    body.push_str(&argument_event("{\"note\":\"", 1));
    let mut cycle = 0usize;
    while body.len() < TARGET_BYTES {
        body.push_str(&argument_event(&format!("chunk-{cycle}:"), 0));
        body.push_str(&argument_event(&format!("note-{cycle}"), 1));
        let split_token = match cycle % 3 {
            0 => {
                body.push_str(&argument_event(&token[..split], 0));
                true
            }
            1 => {
                body.push_str(&argument_event(token, 0));
                false
            }
            _ => {
                body.push_str(&argument_event("absent-token", 0));
                false
            }
        };
        body.push_str(&argument_event("-tail", 1));
        if split_token {
            // Complete the split token only after the interleaved call, so
            // cross-pair state cannot absorb the fragment.
            body.push_str(&argument_event(&token[split..], 0));
        } else {
            body.push_str(&argument_event("-tail", 0));
        }
        cycle += 1;
    }
    body.push_str(&argument_event("\"}", 0));
    body.push_str(&argument_event("\"}", 1));
    body.push_str(&finish_event());
    body.push_str("data: [DONE]\n\n");
    body
}

fn percentile(samples: &[Duration], p: usize) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let index = ((samples.len() - 1) * p) / 100;
    samples[index]
}

async fn drain(client: &Client, base_url: &str, request: &serde_json::Value) {
    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("authorization", format!("Bearer {RUNTIME_API_KEY}"))
        .json(request)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.bytes().await.expect("body should drain");
    let text = String::from_utf8_lossy(&body);
    assert!(!text.contains("__PKV_"), "restoration must cover the measured traffic");
    assert!(text.contains("acme"), "restored original must appear in the drained body");
}

#[tokio::test]
async fn llm_sse_tool_argument_overhead_stays_within_budget() {
    let token = token_for(TOOL_FRAGMENT);
    let body = traffic_body(&token);
    assert!(body.len() >= TARGET_BYTES, "fixture must cover 32 KiB: {}", body.len());
    assert!(body.len() < TARGET_BYTES + 4096, "fixture should stay near 32 KiB: {}", body.len());

    let provider = start_mock_provider(MockProviderMode::Sse { status: 200, body }).await;
    let runtime_key = write_key_file(RUNTIME_API_KEY);
    let provider_key = write_key_file(PROVIDER_API_KEY);
    let rehydration_key = write_key_file(REHYDRATION_KEY);
    let config_path = write_runtime_config(&runtime_config(
        &runtime_key,
        &provider_key,
        &rehydration_key,
        &provider.base_url,
    ));
    let runtime: RuntimeHandle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client =
        Client::builder().timeout(Duration::from_secs(3)).build().expect("client should build");

    let mut results = Vec::new();
    for model in [POLICY_ROUTE, PLAIN_ROUTE] {
        let request = json!({
            "model": model,
            "stream": true,
            "messages": [{
                "role": "user",
                "content": format!("please restore {TOOL_FRAGMENT} here")
            }],
            "metadata": {"profile": "custom"}
        });
        for _ in 0..WARMUPS {
            drain(&client, &runtime.base_url(), &request).await;
        }

        let mut latencies = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let response = client
                .post(format!("{}/v1/chat/completions", runtime.base_url()))
                .header("authorization", format!("Bearer {RUNTIME_API_KEY}"))
                .json(&request)
                .send()
                .await
                .expect("request should complete");
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.bytes().await.expect("body should drain");
            latencies.push(started.elapsed());
            let text = String::from_utf8_lossy(&body);
            assert!(!text.contains("__PKV_"), "{model}: tokens must restore");
            assert!(text.contains("acme"), "{model}: original must be visible");
        }

        latencies.sort_unstable();
        let p95 = percentile(&latencies, 95);
        let p99 = percentile(&latencies, 99);
        println!(
            "{model}: p95={}ms p99={}ms over {SAMPLES} requests",
            p95.as_millis(),
            p99.as_millis()
        );
        results.push((model, p95, p99));
    }

    for (model, p95, _) in &results {
        assert!(
            *p95 <= Duration::from_millis(P95_BUDGET_MS),
            "{model}: p95 latency must be <= {P95_BUDGET_MS}ms, got {}us",
            p95.as_micros()
        );
    }

    runtime.shutdown().await.expect("runtime should stop cleanly");
    provider.shutdown().await;
}
