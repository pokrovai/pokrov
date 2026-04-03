use std::{io::Write, sync::Arc, time::Duration};

use axum::{body::Body, http::Request};
use reqwest::StatusCode;
use tempfile::NamedTempFile;
use tower::ServiceExt;

#[tokio::test]
async fn ready_returns_not_ready_while_startup_is_pending() {
    let lifecycle = Arc::new(pokrov_runtime::lifecycle::RuntimeLifecycle::new());
    let app = pokrov_api::app::build_router(pokrov_api::app::AppState {
        lifecycle,
        metrics: Arc::new(pokrov_metrics::registry::RuntimeMetricsRegistry::default()),
    });

    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn draining_rejects_new_health_requests() {
    let lifecycle = Arc::new(pokrov_runtime::lifecycle::RuntimeLifecycle::new());
    lifecycle.set_config_loaded(true).await;
    lifecycle.mark_draining().await;
    let app = pokrov_api::app::build_router(pokrov_api::app::AppState {
        lifecycle,
        metrics: Arc::new(pokrov_metrics::registry::RuntimeMetricsRegistry::default()),
    });

    let health_response = app
        .clone()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .expect("health request should complete");
    assert_eq!(health_response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let ready_response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .expect("ready request should complete");
    assert_eq!(ready_response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn runtime_enters_draining_before_shutdown_with_inflight_request() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 1000
  grace_period_ms: 1500
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

    let inflight = {
        let client = client.clone();
        let url = format!("{}/health?delay_ms=350", base_url);
        tokio::spawn(async move { client.get(url).send().await })
    };

    tokio::time::sleep(Duration::from_millis(50)).await;

    let shutdown_task = tokio::spawn(async move { handle.shutdown().await });

    let mut seen_not_ready = false;
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(30)).await;
        match client.get(format!("{}/ready", base_url)).send().await {
            Ok(response) if response.status() == StatusCode::SERVICE_UNAVAILABLE => {
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

    assert!(seen_not_ready, "runtime should stop serving ready responses during draining");

    let inflight_result = inflight.await.expect("inflight task should finish");
    assert_eq!(inflight_result.expect("inflight response should succeed").status(), StatusCode::OK);

    shutdown_task.await.expect("shutdown task should join").expect("shutdown should succeed");
}

#[tokio::test]
async fn shutdown_fails_when_grace_period_is_exceeded() {
    let config_path = write_temp_config(
        r#"
server:
  host: 127.0.0.1
  port: 0
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 100
  grace_period_ms: 300
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
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client should build");

    let inflight = {
        let client = client.clone();
        let url = format!("{}/health?delay_ms=3000", base_url);
        tokio::spawn(async move { client.get(url).send().await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;

    let shutdown_result = tokio::time::timeout(Duration::from_secs(2), handle.shutdown())
        .await
        .expect("shutdown should return before test timeout");
    match shutdown_result {
        Err(pokrov_runtime::bootstrap::BootstrapError::Serve(error)) => {
            assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        }
        other => panic!("expected serve timeout error, got: {other:?}"),
    }

    let _ = tokio::time::timeout(Duration::from_secs(1), inflight).await;
}

fn write_temp_config(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp config should be created");
    file.write_all(content.as_bytes()).expect("temp config should be written");
    file.into_temp_path().keep().expect("temp config path")
}
