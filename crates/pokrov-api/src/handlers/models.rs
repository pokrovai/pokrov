use axum::{
    extract::{Extension, State},
    http::{header, HeaderValue},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use super::request_context::{
    RequestContextHooks, UpstreamCredentialRequirement, resolve_request_context,
};
use crate::{
    app::{AppState, GatewayAuthContext},
    error::ApiError,
};

/// Returns routable model ids in an OpenAI-compatible catalog shape.
pub async fn handle_models(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    Extension(gateway_auth): Extension<GatewayAuthContext>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let metadata_mode = state.llm.response_metadata_mode;
    let _context = resolve_request_context(
        &state,
        &headers,
        &gateway_auth,
        &request_id,
        "/v1/models",
        UpstreamCredentialRequirement::Optional,
        &RequestContextHooks {
            on_auth_stage,
            emit_auth_stage,
            map_error: None,
        },
    )
    .map_err(|error| error.with_response_metadata_mode(metadata_mode))?;

    if state.llm.handler.is_none() {
        return Err(
            ApiError::runtime_not_ready(request_id.clone(), "llm proxy is not ready")
                .with_response_metadata_mode(metadata_mode),
        );
    }
    state.metrics.on_models_catalog_request();
    let payload = state.llm.model_catalog_payload.clone().ok_or_else(|| {
        ApiError::internal(request_id, "llm model catalog payload is not initialized")
            .with_response_metadata_mode(metadata_mode)
    })?;

    let mut response = (StatusCode::OK, payload.as_ref().clone()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

fn on_auth_stage(state: &AppState, mode: &'static str, stage: &'static str, decision: &'static str) {
    state.metrics.on_auth_decision(mode, stage, decision);
}

fn emit_auth_stage(
    request_id: &str,
    endpoint: &'static str,
    mode: &'static str,
    stage: &'static str,
    decision: &'static str,
) {
    pokrov_proxy_llm::audit::LLMAuthStageAuditEvent {
        request_id: request_id.to_string(),
        endpoint,
        auth_mode: mode,
        stage,
        decision,
    }
    .emit();
}
