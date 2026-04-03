use std::time::{Duration, Instant};

use reqwest::StatusCode;

use crate::hardening_test_support::write_hardening_runtime_config;

#[tokio::test]
async fn hardening_runtime_keeps_probe_latency_within_smoke_budget() {
    let config_path = write_hardening_runtime_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 600
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
"#,
    );

    let runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client should build");

    let started = Instant::now();
    for _ in 0..20 {
        let response = client
            .get(format!("{}/health", runtime.base_url()))
            .send()
            .await
            .expect("health should respond");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let avg_ms = started.elapsed().as_millis() / 20;
    assert!(
        avg_ms <= 50,
        "average probe latency should remain within 50 ms, got {avg_ms} ms"
    );

    runtime.shutdown().await.expect("runtime should stop cleanly");
}
