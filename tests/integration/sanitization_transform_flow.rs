use std::{fs, io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn nested_payload_is_transformed_without_structure_break() {
    let token = "transform-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "profile_id": "custom",
            "mode": "enforce",
            "payload": {
                "messages": [
                    {
                        "content": "Project Andromeda email user@example.com token sk-test-12345678"
                    }
                ],
                "attempt": 3,
                "active": true
            }
        }))
        .send()
        .await
        .expect("evaluate request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");

    assert_eq!(body["final_action"], "redact");
    let sanitized = body["sanitized_payload"].clone();
    assert!(sanitized.is_object());
    assert_eq!(sanitized["attempt"], 3);
    assert_eq!(sanitized["active"], true);

    let content = sanitized["messages"][0]["content"]
        .as_str()
        .expect("sanitized nested content should be string");
    assert!(content.contains("[REDACTED]"));
    assert!(content.contains("[CODE]"));
    assert!(content.contains("****"));

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn block_outcome_does_not_include_sanitized_payload() {
    let token = "transform-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let response = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "profile_id": "strict",
            "mode": "enforce",
            "payload": {
                "content": "token sk-test-12345678"
            }
        }))
        .send()
        .await
        .expect("evaluate request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");

    assert_eq!(body["final_action"], "block");
    assert!(body.get("sanitized_payload").is_none());

    handle.shutdown().await.expect("shutdown should succeed");
}

fn write_config_with_file_key(token: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let mut key_file = NamedTempFile::new().expect("key file should be created");
    key_file.write_all(token.as_bytes()).expect("key file should be written");
    let key_path = key_file.into_temp_path().keep().expect("key path should persist");

    let key_path_display = key_path.display();

    let config = format!(
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
    - key: file:{key_path_display}
      profile: strict
    - key: file:{key_path_display}
      profile: custom
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
      mode_default: enforce
      categories:
        secrets: mask
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
      custom_rules:
        - id: custom.project_andromeda
          category: corporate_markers
          pattern: "(?i)project\\s+andromeda"
          action: replace
          replacement: "[CODE]"
          priority: 900
          enabled: true
"#
    );

    let config_file = NamedTempFile::new().expect("config should be created");
    fs::write(config_file.path(), config).expect("config should be written");
    let config_path = config_file.into_temp_path().keep().expect("config path should persist");

    (config_path, key_path)
}
