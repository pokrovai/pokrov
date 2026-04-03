use std::time::Duration;

use reqwest::StatusCode;

use crate::hardening_test_support::{count_prometheus_series, write_hardening_runtime_config};

#[tokio::test]
async fn metrics_endpoint_exposes_mandatory_hardening_series() {
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
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let health = client
        .get(format!("{}/health", runtime.base_url()))
        .send()
        .await
        .expect("health should respond");
    assert_eq!(health.status(), StatusCode::OK);

    let metrics = client
        .get(format!("{}/metrics", runtime.base_url()))
        .send()
        .await
        .expect("metrics should respond");
    assert_eq!(metrics.status(), StatusCode::OK);
    let payload = metrics.text().await.expect("metrics body should parse");

    assert!(payload.contains("pokrov_requests_total"));
    assert!(payload.contains("pokrov_blocked_total"));
    assert!(payload.contains("pokrov_rate_limit_events_total"));
    assert!(payload.contains("pokrov_upstream_errors_total"));
    assert!(payload.contains("pokrov_request_duration_seconds"));
    assert!(count_prometheus_series(&payload, "pokrov_requests_total") >= 1);

    runtime.shutdown().await.expect("runtime should stop cleanly");
}
