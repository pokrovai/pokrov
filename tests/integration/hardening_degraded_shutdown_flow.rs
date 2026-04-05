use std::time::Duration;

use reqwest::StatusCode;

use crate::hardening_test_support::write_hardening_runtime_config;

#[tokio::test]
async fn readiness_and_shutdown_follow_degradation_contract() {
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
    let base_url = runtime.base_url();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let ready =
        client.get(format!("{}/ready", base_url)).send().await.expect("ready should respond");
    assert_eq!(ready.status(), StatusCode::OK);

    runtime.shutdown().await.expect("shutdown should succeed");

    let after_shutdown = client.get(format!("{}/health", base_url)).send().await;
    assert!(after_shutdown.is_err(), "runtime should stop accepting requests after shutdown");
}
