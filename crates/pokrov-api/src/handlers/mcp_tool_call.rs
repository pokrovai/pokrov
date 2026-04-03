use axum::{
    extract::{rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap},
    Json,
};
use pokrov_proxy_mcp::types::{McpToolCallRequest, McpToolCallResponse};

use crate::{app::AppState, error::ApiError};

/// Handles MCP tool-call requests and keeps local error responses in MCP envelope shape.
pub async fn handle_mcp_tool_call(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Result<Json<McpToolCallRequest>, JsonRejection>,
) -> Result<Json<McpToolCallResponse>, ApiError> {
    let body = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;

    let token = parse_bearer_token(&headers)
        .ok_or_else(|| {
            enforce_mcp_error_contract(ApiError::unauthorized(
                request_id.clone(),
                "missing bearer authorization",
            ))
        })?;

    let api_key_profile = state.sanitization.profile_for_token(token).ok_or_else(|| {
        enforce_mcp_error_contract(ApiError::unauthorized(
            request_id.clone(),
            "invalid API key or profile binding",
        ))
    })?;

    let handler = state.mcp.handler.clone().ok_or_else(|| {
        enforce_mcp_error_contract(ApiError::invalid_request(
            request_id.clone(),
            "mcp proxy is not configured",
        ))
    })?;

    let response = handler
        .handle_tool_call(request_id.clone(), body, &api_key_profile)
        .await
        .map_err(ApiError::from_mcp_proxy)?;

    Ok(Json(response))
}

fn parse_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn map_json_rejection(request_id: String, rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::BytesRejection(_) => enforce_mcp_error_contract(ApiError::payload_too_large(
            request_id,
            "request body exceeds configured size limit",
        )),
        _ => enforce_mcp_error_contract(ApiError::invalid_request(
            request_id,
            "invalid request body",
        )),
    }
}

fn enforce_mcp_error_contract(mut error: ApiError) -> ApiError {
    error.allowed = Some(false);
    error
}
