use std::{io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn bootstrap_acceptance_contract_matches_fr_011() {
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
    let base_url = handle.base_url();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client should build");

    let health = client
        .get(format!("{base_url}/health"))
        .send()
        .await
        .expect("health request should succeed");
    assert_eq!(health.status(), StatusCode::OK);

    let ready =
        client.get(format!("{base_url}/ready")).send().await.expect("ready request should succeed");
    assert_eq!(ready.status(), StatusCode::OK);

    let shutdown_task = tokio::spawn(async move { handle.shutdown().await });
    let mut seen_not_ready = false;

    for _ in 0..15 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        match client.get(format!("{base_url}/ready")).send().await {
            Ok(response) if response.status() != StatusCode::OK => {
                seen_not_ready = true;
                break;
            }
            Err(_) => {
                seen_not_ready = true;
                break;
            }
            _ => {}
        }
    }

    assert!(seen_not_ready, "runtime should stop returning ready=200 while draining");

    shutdown_task.await.expect("shutdown task should join").expect("shutdown should succeed");
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
