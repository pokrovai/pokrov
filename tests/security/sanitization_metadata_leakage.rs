use std::{fs, io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn evaluate_outputs_do_not_leak_raw_fragments_in_audit_and_explain() {
    let token = "security-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let raw_fragment = "project andromeda user@example.com sk-test-rawsecret";

    let response = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "profile_id": "custom",
            "mode": "enforce",
            "payload": {
                "message": raw_fragment
            }
        }))
        .send()
        .await
        .expect("evaluate request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body expected");

    let explain = serde_json::to_string(&body["explain"]).expect("explain should serialize");
    let audit = serde_json::to_string(&body["audit"]).expect("audit should serialize");
    let executed = serde_json::to_string(&body["executed"]).expect("executed should serialize");
    let degraded = serde_json::to_string(&body["degraded"]).expect("degraded should serialize");

    assert!(!explain.contains(raw_fragment));
    assert!(!audit.contains(raw_fragment));
    assert!(!executed.contains(raw_fragment));
    assert!(!degraded.contains(raw_fragment));
    assert!(!explain.contains("user@example.com"));
    assert!(!audit.contains("sk-test-rawsecret"));
    assert!(!executed.contains("user@example.com"));
    assert!(!degraded.contains("sk-test-rawsecret"));

    handle.shutdown().await.expect("shutdown should succeed");
}

fn write_config_with_file_key(token: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let mut key_file = NamedTempFile::new().expect("key file should be created");
    key_file
        .write_all(token.as_bytes())
        .expect("key file should be written");
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
      profile: custom
sanitization:
  enabled: true
  default_profile: custom
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
        secrets: redact
        pii: redact
        corporate_markers: redact
      mask_visible_suffix: 4
"#
    );

    let config_file = NamedTempFile::new().expect("config should be created");
    fs::write(config_file.path(), config).expect("config should be written");
    let config_path = config_file.into_temp_path().keep().expect("config path should persist");

    (config_path, key_path)
}
