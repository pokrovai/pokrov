use std::time::Duration;

use reqwest::StatusCode;

use crate::hardening_test_support::write_hardening_runtime_config;

#[tokio::test]
async fn invalid_auth_and_probe_paths_do_not_leak_sensitive_details() {
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
llm:
  providers:
    - id: openai
      base_url: http://127.0.0.1:1/v1
      auth:
        api_key: env:OPENAI_API_KEY
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
    );

    let runtime = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client should build");

    let unauthorized = client
        .post(format!("{}/v1/chat/completions", runtime.base_url()))
        .json(&serde_json::json!({"model":"gpt-4o-mini","messages":[{"role":"user","content":"hello"}]}))
        .send()
        .await
        .expect("unauthorized request should complete");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let unauthorized_body =
        unauthorized.text().await.expect("unauthorized body should be readable");
    assert!(unauthorized_body.contains("request_id"));
    assert!(!unauthorized_body.contains("Authorization"));

    let ready = client
        .get(format!("{}/ready", runtime.base_url()))
        .send()
        .await
        .expect("ready probe should respond");
    assert!(matches!(ready.status(), StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE));

    runtime.shutdown().await.expect("runtime should stop cleanly");
}
