use std::{
    io::Write,
    time::{Duration, Instant},
};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn probes_respond_within_bootstrap_smoke_budget() {
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
  grace_period_ms: 600
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

    let started = Instant::now();
    for _ in 0..30 {
        let health = client
            .get(format!("{}/health", handle.base_url()))
            .send()
            .await
            .expect("health should respond");
        assert_eq!(health.status(), StatusCode::OK);

        let ready = client
            .get(format!("{}/ready", handle.base_url()))
            .send()
            .await
            .expect("ready should respond");
        assert_eq!(ready.status(), StatusCode::OK);
    }

    let average_ms = started.elapsed().as_millis() / 60;
    assert!(
        average_ms <= 50,
        "probe average latency should stay within smoke budget; got {} ms",
        average_ms
    );

    handle.shutdown().await.expect("shutdown should succeed");
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
