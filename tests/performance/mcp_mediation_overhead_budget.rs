use std::time::{Duration, Instant};

use reqwest::StatusCode;

use super::mcp_test_support::{
    MockMcpMode, start_mock_mcp_server, write_key_file, write_runtime_config,
};

#[tokio::test]
async fn mcp_mediation_overhead_stays_within_budget() {
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

    let request = serde_json::json!({
        "server": "repo-tools",
        "tool": "read_file",
        "arguments": {"path": "src/lib.rs"},
        "metadata": {"profile": "strict"}
    });

    for _ in 0..5 {
        let warmup = client
            .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
            .header("authorization", "Bearer mcp-test-key")
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
            .post(format!("{}/v1/mcp/tool-call", handle.base_url()))
            .header("authorization", "Bearer mcp-test-key")
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
    upstream.shutdown().await;
}

fn percentile(samples: &[u64], p: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }

    let index = ((samples.len() - 1) * p) / 100;
    samples[index]
}
