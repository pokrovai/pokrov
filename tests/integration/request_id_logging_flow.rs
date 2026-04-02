use std::{io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn request_id_is_propagated_from_header_to_response() {
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

    let request_id = "external-correlation-id";
    let response = client
        .get(format!("{}/health", handle.base_url()))
        .header("x-request-id", request_id)
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let response_header_id = response
        .headers()
        .get("x-request-id")
        .expect("response must include x-request-id")
        .to_str()
        .expect("request id header must be valid utf-8")
        .to_string();

    let body: serde_json::Value = response.json().await.expect("json body expected");
    let response_body_id = body["request_id"].as_str().expect("request_id expected in body");

    assert_eq!(response_header_id, request_id);
    assert_eq!(response_body_id, request_id);

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn request_id_is_generated_when_header_is_missing() {
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

    let response = client
        .get(format!("{}/ready", handle.base_url()))
        .send()
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let response_header_id = response
        .headers()
        .get("x-request-id")
        .expect("response must include x-request-id")
        .to_str()
        .expect("request id header must be valid utf-8")
        .to_string();

    let body: serde_json::Value = response.json().await.expect("json body expected");
    let response_body_id = body["request_id"].as_str().expect("request_id expected in body");

    assert!(!response_header_id.is_empty());
    assert_eq!(response_body_id, response_header_id);

    handle.shutdown().await.expect("shutdown should succeed");
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
