use std::{io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn runtime_starts_with_valid_config_and_serves_probes() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
"#,
    );

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client should build");

    let health = client
        .get(format!("{}/health", handle.base_url()))
        .send()
        .await
        .expect("health request should succeed");
    assert_eq!(health.status(), StatusCode::OK);

    let ready = client
        .get(format!("{}/ready", handle.base_url()))
        .send()
        .await
        .expect("ready request should succeed");
    assert_eq!(ready.status(), StatusCode::OK);

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn runtime_rejects_invalid_config_before_ready() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 5000
  grace_period_ms: 1000
security:
  api_keys:
    - key: plaintext-secret
      profile: strict
"#,
    );

    let startup = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path).await;
    assert!(startup.is_err(), "invalid config must fail startup");
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
