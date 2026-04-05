use std::{fs, io::Write, time::Duration};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn strict_profile_returns_deterministic_evaluation_for_same_payload() {
    let token = "sanitization-test-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let payload = serde_json::json!({
        "profile_id": "strict",
        "mode": "enforce",
        "payload": {
            "messages": [
                {
                    "role": "user",
                    "content": "my card is 4111 1111 1111 1111 and token sk-test-abc12345"
                }
            ]
        }
    });

    let first = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&payload)
        .send()
        .await
        .expect("first evaluate request should succeed");
    assert_eq!(first.status(), StatusCode::OK);
    let first_body: serde_json::Value = first.json().await.expect("json body expected");

    let second = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&payload)
        .send()
        .await
        .expect("second evaluate request should succeed");
    assert_eq!(second.status(), StatusCode::OK);
    let second_body: serde_json::Value = second.json().await.expect("json body expected");

    assert_eq!(first_body["final_action"], "block");
    assert!(first_body.get("sanitized_payload").is_none());
    assert_eq!(first_body["final_action"], second_body["final_action"]);
    assert_eq!(first_body["explain"], second_body["explain"]);
    assert_eq!(first_body["audit"]["rule_hits_total"], second_body["audit"]["rule_hits_total"]);

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn evaluate_returns_structured_error_for_invalid_json_shape() {
    let token = "sanitization-test-key";
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
        .header("x-request-id", "invalid-json-shape")
        .json(&serde_json::json!({
            "profile_id": "strict",
            "payload": {
                "messages": [{"role": "user", "content": "hello"}]
            }
        }))
        .send()
        .await
        .expect("evaluate request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let header_request_id = response
        .headers()
        .get("x-request-id")
        .expect("response must include x-request-id")
        .to_str()
        .expect("x-request-id must be valid utf-8")
        .to_string();
    let body: serde_json::Value = response.json().await.expect("json body expected");
    assert_eq!(body["request_id"], header_request_id);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert_eq!(body["error"]["message"], "invalid request body");

    handle.shutdown().await.expect("shutdown should succeed");
}

#[tokio::test]
async fn equivalent_plain_text_and_json_leaf_inputs_keep_same_hit_counts() {
    let token = "sanitization-test-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let direct_payload = serde_json::json!({
        "profile_id": "strict",
        "mode": "enforce",
        "payload": {
            "content": "card 4111 1111 1111 1111 token sk-test-abc12345"
        }
    });
    let nested_payload = serde_json::json!({
        "profile_id": "strict",
        "mode": "enforce",
        "payload": {
            "wrapper": {
                "content": "card 4111 1111 1111 1111 token sk-test-abc12345"
            }
        }
    });

    let direct = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&direct_payload)
        .send()
        .await
        .expect("direct request should succeed")
        .json::<serde_json::Value>()
        .await
        .expect("direct response json expected");
    let nested = client
        .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
        .header("authorization", format!("Bearer {token}"))
        .json(&nested_payload)
        .send()
        .await
        .expect("nested request should succeed")
        .json::<serde_json::Value>()
        .await
        .expect("nested response json expected");

    assert_eq!(direct["audit"]["rule_hits_total"], nested["audit"]["rule_hits_total"]);
    assert_eq!(direct["explain"]["rule_hits_total"], nested["explain"]["rule_hits_total"]);
    assert_eq!(direct["explain"]["reason_codes"], nested["explain"]["reason_codes"]);

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
