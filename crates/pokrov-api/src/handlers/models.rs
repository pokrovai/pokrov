use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pokrov_proxy_llm::routing::ModelCatalogKind;
use serde::Serialize;

use super::request_context::{
    RequestContextHooks, UpstreamCredentialRequirement, resolve_request_context,
};
use crate::{
    app::{AppState, GatewayAuthContext},
    error::ApiError,
};

/// Returns routable model ids for auto-discovery, including canonical names and aliases.
pub async fn handle_models(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    Extension(gateway_auth): Extension<GatewayAuthContext>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
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
            map_error,
        },
    )
    .map_err(|error| error.with_response_metadata_mode(metadata_mode))?;

    let handler = state
        .llm
        .handler
        .clone()
        .ok_or_else(|| {
            ApiError::runtime_not_ready(request_id.clone(), "llm proxy is not ready")
                .with_response_metadata_mode(metadata_mode)
        })?;
    state.metrics.on_models_catalog_request();

    let data = handler
        .model_catalog()
        .iter()
        .map(|entry| ModelCatalogResponseEntry {
            id: entry.id.clone(),
            object: "model",
            canonical_model: entry.canonical_model.clone(),
            provider_id: entry.provider_id.clone(),
            kind: match entry.kind {
                ModelCatalogKind::Canonical => "canonical",
                ModelCatalogKind::Alias => "alias",
            },
        })
        .collect::<Vec<_>>();

    Ok((
        StatusCode::OK,
        Json(ModelCatalogResponse {
            object: "list",
            data,
        }),
    ))
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

fn map_error(error: ApiError) -> ApiError {
    error
}

#[derive(Debug, Serialize)]
pub struct ModelCatalogResponse {
    pub object: &'static str,
    pub data: Vec<ModelCatalogResponseEntry>,
}

#[derive(Debug, Serialize)]
pub struct ModelCatalogResponseEntry {
    pub id: String,
    pub object: &'static str,
    pub canonical_model: String,
    pub provider_id: String,
    pub kind: &'static str,
}
