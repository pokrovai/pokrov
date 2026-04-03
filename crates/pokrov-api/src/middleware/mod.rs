use axum::{
    body::Body,
    extract::State,
    http::{header, header::HeaderName, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::time::Instant;
use uuid::Uuid;

use crate::app::{AppState, RuntimeStateView};
use crate::middleware::request_id::normalize_or_generate_request_id;

pub mod request_id;
pub mod rate_limit;

const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub async fn request_id_middleware(
    _state: State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = normalize_or_generate_request_id(
        request.headers().get(&X_REQUEST_ID).and_then(|value| value.to_str().ok()),
    );

    request.extensions_mut().insert(request_id.clone());
    let mut response = next.run(request).await;

    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(X_REQUEST_ID, header_value);
    }

    response
}

pub async fn active_requests_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let request_id =
        request.extensions().get::<String>().cloned().unwrap_or_else(|| Uuid::new_v4().to_string());
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let policy_profile = resolve_policy_profile(&state, request.headers());
    let runtime_state = state.lifecycle.state();

    if matches!(runtime_state, RuntimeStateView::Draining | RuntimeStateView::Stopped)
        && path != "/ready"
    {
        tracing::info!(
            component = "runtime",
            action = "request_rejected",
            request_id = %request_id,
            method = %method,
            path = %path,
            reason = "runtime_draining"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    state.lifecycle.on_request_started();
    state.metrics.on_request_started();
    let started = Instant::now();
    tracing::info!(
        component = "runtime",
        action = "request_started",
        request_id = %request_id,
        method = %method,
        path = %path
    );

    let response = next.run(request).await;
    let status_code = response.status().as_u16();
    let decision = if (200..300).contains(&status_code) {
        "allowed"
    } else if status_code == 429 || status_code == 403 {
        "blocked"
    } else {
        "errored"
    };
    let route = normalize_route(&path);
    let path_class = classify_path(&path);

    state.lifecycle.on_request_finished();
    state.metrics.on_request_finished();
    state
        .metrics
        .on_request_outcome(route, path_class, status_code, decision);
    state.metrics.on_request_duration_seconds(
        route,
        path_class,
        decision,
        started.elapsed().as_secs_f64(),
    );
    if status_code == 429 {
        state
            .metrics
            .on_blocked_request(route, "rate_limit", policy_profile.as_deref().unwrap_or("custom"));
    }
    if status_code == 403 {
        state
            .metrics
            .on_blocked_request(route, "policy", policy_profile.as_deref().unwrap_or("custom"));
    }
    tracing::info!(
        component = "runtime",
        action = "request_finished",
        request_id = %request_id,
        method = %method,
        path = %path,
        status_code
    );
    response
}

fn resolve_policy_profile(state: &AppState, headers: &HeaderMap) -> Option<String> {
    parse_bearer_token(headers).and_then(|token| state.sanitization.profile_for_token(token))
}

fn parse_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn normalize_route(path: &str) -> &'static str {
    if path.starts_with("/v1/mcp/tools/") && path.ends_with("/invoke") {
        return "/v1/mcp/tools/{toolName}/invoke";
    }

    match path {
        "/health" => "/health",
        "/ready" => "/ready",
        "/metrics" => "/metrics",
        "/v1/sanitize/evaluate" => "/v1/sanitize/evaluate",
        "/v1/chat/completions" => "/v1/chat/completions",
        "/v1/mcp/tool-call" => "/v1/mcp/tool-call",
        _ => "other",
    }
}

fn classify_path(path: &str) -> &'static str {
    match path {
        "/health" | "/ready" | "/metrics" => "runtime",
        "/v1/sanitize/evaluate" => "sanitization",
        "/v1/chat/completions" => "llm",
        _ if path.starts_with("/v1/mcp") => "mcp",
        _ => "runtime",
    }
}
