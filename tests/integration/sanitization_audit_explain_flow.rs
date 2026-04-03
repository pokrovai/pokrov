use std::{fs, io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn dry_run_keeps_decision_parity_and_returns_metadata_only_explain() {
    let token = "audit-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let request_payload = serde_json::json!({
        "profile_id": "strict",
        "payload": {
            "messages": [
                {
                    "role": "user",
                    "content": "Project Andromeda and token sk-test-12345678"
                }
            ]
        }
    });

    let enforce_response = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "profile_id": request_payload["profile_id"],
            "mode": "enforce",
            "payload": request_payload["payload"],
        }))
        .send()
        .await
        .expect("enforce request should succeed");
    assert_eq!(enforce_response.status(), StatusCode::OK);
    let enforce_body: serde_json::Value = enforce_response.json().await.expect("json body expected");

    let dry_run_response = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "profile_id": request_payload["profile_id"],
            "mode": "dry_run",
            "payload": request_payload["payload"],
        }))
        .send()
        .await
        .expect("dry-run request should succeed");
    assert_eq!(dry_run_response.status(), StatusCode::OK);
    let dry_run_body: serde_json::Value = dry_run_response.json().await.expect("json body expected");

    assert_eq!(enforce_body["final_action"], dry_run_body["final_action"]);
    assert_eq!(enforce_body["explain"]["rule_hits_total"], dry_run_body["explain"]["rule_hits_total"]);
    assert_eq!(enforce_body["audit"]["rule_hits_total"], dry_run_body["audit"]["rule_hits_total"]);
    assert_eq!(enforce_body["executed"], true);
    assert_eq!(dry_run_body["executed"], false);

    let explain_json = serde_json::to_string(&dry_run_body["explain"]).expect("explain should serialize");
    let audit_json = serde_json::to_string(&dry_run_body["audit"]).expect("audit should serialize");
    assert!(!explain_json.contains("sk-test-12345678"));
    assert!(!audit_json.contains("sk-test-12345678"));

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
      profile: strict
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
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
"#
    );

    let config_file = NamedTempFile::new().expect("config should be created");
    fs::write(config_file.path(), config).expect("config should be written");
    let config_path = config_file.into_temp_path().keep().expect("config path should persist");

    (config_path, key_path)
}
