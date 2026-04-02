use axum::{
    body::Body,
    extract::State,
    http::{header::HeaderName, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::app::{AppState, RuntimeStateView};
use crate::middleware::request_id::normalize_or_generate_request_id;

pub mod request_id;

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
    tracing::info!(
        component = "runtime",
        action = "request_started",
        request_id = %request_id,
        method = %method,
        path = %path
    );

    let response = next.run(request).await;
    let status_code = response.status().as_u16();

    state.lifecycle.on_request_finished();
    state.metrics.on_request_finished();
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
