use std::{fs, io::Write, time::Instant};

use reqwest::StatusCode;
use tempfile::NamedTempFile;

#[tokio::test]
async fn evaluate_overhead_meets_baseline_budget_and_is_deterministic() {
    let token = "perf-key";
    let (config_path, _key_path) = write_config_with_file_key(token);

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .expect("client should build");

    let request = serde_json::json!({
        "profile_id": "strict",
        "mode": "enforce",
        "payload": {
            "messages": [
                {
                    "role": "user",
                    "content": "token sk-test-12345678 email user@example.com"
                }
            ]
        }
    });

    for _ in 0..5 {
        let warmup = client
            .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
            .header("authorization", format!("Bearer {token}"))
            .json(&request)
            .send()
            .await
            .expect("warmup request should succeed");
        assert_eq!(warmup.status(), StatusCode::OK);
    }

    let mut latencies_ms = Vec::new();
    let mut decisions = Vec::new();

    for _ in 0..40 {
        let started = Instant::now();
        let response = client
            .post(format!("{}/v1/sanitize/evaluate", handle.base_url()))
            .header("authorization", format!("Bearer {token}"))
            .json(&request)
            .send()
            .await
            .expect("evaluate request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = response.json().await.expect("json body expected");
        decisions.push(body["final_action"].as_str().unwrap_or_default().to_string());
        latencies_ms.push(started.elapsed().as_millis() as u64);
    }

    latencies_ms.sort_unstable();
    let p95 = percentile(&latencies_ms, 95);
    let p99 = percentile(&latencies_ms, 99);

    assert!(p95 <= 50, "p95 evaluate latency must be <= 50ms, got {p95}ms");
    assert!(p99 <= 100, "p99 evaluate latency must be <= 100ms, got {p99}ms");

    let first_decision = decisions.first().cloned().unwrap_or_default();
    assert!(decisions.iter().all(|decision| decision == &first_decision));

    handle.shutdown().await.expect("shutdown should succeed");
}

fn percentile(samples: &[u64], p: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }

    let idx = ((samples.len() - 1) * p) / 100;
    samples[idx]
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
