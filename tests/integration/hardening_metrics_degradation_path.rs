use std::sync::Arc;

use axum::{body::Body, http::Request};
use reqwest::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn readiness_reports_degraded_when_metrics_rendering_fails_but_health_stays_live() {
    let lifecycle = Arc::new(pokrov_runtime::lifecycle::RuntimeLifecycle::new());
    lifecycle.set_config_loaded(true).await;
    lifecycle.set_llm_routes_loaded(true).await;
    lifecycle.set_mcp_routes_loaded(true).await;
    lifecycle.mark_ready().await;

    let metrics_registry = Arc::new(pokrov_metrics::registry::RuntimeMetricsRegistry::default());
    metrics_registry.set_force_render_failure(true);

    let app = pokrov_api::app::build_router(pokrov_api::app::AppState {
        lifecycle,
        metrics: metrics_registry.clone(),
        metrics_registry,
        sanitization: pokrov_api::app::SanitizationState::default(),
        rate_limit: pokrov_api::app::RateLimitState::default(),
        llm: pokrov_api::app::LlmProxyState::default(),
        mcp: pokrov_api::app::McpProxyState::default(),
        auth: pokrov_api::app::AuthState::default(),
    });

    let health = app
        .clone()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .expect("health request should complete");
    assert_eq!(health.status(), StatusCode::OK);

    let metrics = app
        .clone()
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .expect("metrics request should complete");
    assert_eq!(metrics.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let ready = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .expect("ready request should complete");
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = axum::body::to_bytes(ready.into_body(), usize::MAX)
        .await
        .expect("ready body should decode");
    let payload: serde_json::Value =
        serde_json::from_slice(&body).expect("ready body must be json");
    assert_eq!(payload["status"], "degraded");
    assert_eq!(payload["checks"]["runtime"], "ok");
}
