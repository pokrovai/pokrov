use std::time::{Duration, Instant};

use reqwest::StatusCode;

use super::llm_proxy_test_support::{
    MockProviderMode, start_mock_provider, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn llm_proxy_non_stream_overhead_stays_within_budget() {
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop"
            }]
        }),
    })
    .await;

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
        secrets: block
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
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
        runtime_key = runtime_key_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let request = serde_json::json!({
        "model": "gpt-4o-mini",
        "stream": false,
        "messages": [{"role": "user", "content": "hello"}]
    });

    for _ in 0..5 {
        let warmup = client
            .post(format!("{}/v1/chat/completions", handle.base_url()))
            .header("authorization", "Bearer llm-test-key")
            .json(&request)
            .send()
            .await
            .expect("warmup request should succeed");
        assert_eq!(warmup.status(), StatusCode::OK);
    }

    let mut latencies = Vec::new();
    for _ in 0..40 {
        let started = Instant::now();
        let response = client
            .post(format!("{}/v1/chat/completions", handle.base_url()))
            .header("authorization", "Bearer llm-test-key")
            .json(&request)
            .send()
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        latencies.push(started.elapsed().as_millis() as u64);
    }

    latencies.sort_unstable();
    let p95 = percentile(&latencies, 95);
    let p99 = percentile(&latencies, 99);

    assert!(p95 <= 50, "p95 latency must be <= 50ms, got {p95}ms");
    assert!(p99 <= 100, "p99 latency must be <= 100ms, got {p99}ms");

    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

fn percentile(samples: &[u64], p: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }

    let index = ((samples.len() - 1) * p) / 100;
    samples[index]
}
