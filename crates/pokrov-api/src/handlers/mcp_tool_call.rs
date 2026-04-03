use axum::{
    extract::{Path, rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap},
    Json,
};
use pokrov_proxy_mcp::types::{McpRequestMetadata, McpToolCallRequest, McpToolCallResponse};
use pokrov_proxy_mcp::audit::McpRateLimitAuditEvent;
use serde::Deserialize;

use crate::{app::AppState, error::ApiError};

#[derive(Debug, Deserialize)]
struct McpToolInvokePathRequest {
    server: String,
    arguments: serde_json::Value,
    #[serde(default)]
    metadata: McpRequestMetadata,
}

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

    if let Some(limiter) = state.rate_limit.limiter.as_ref() {
        let estimated_units = estimate_mcp_token_units(&body.arguments);
        let decision = limiter
            .evaluate(token, &api_key_profile, estimated_units)
            .await;
        if !matches!(decision.reason, crate::app::RateLimitReason::WithinBudget) {
            McpRateLimitAuditEvent {
                request_id: request_id.clone(),
                profile_id: api_key_profile.clone(),
                decision: if decision.allowed { "dry_run" } else { "blocked" }.to_string(),
                retry_after_ms: decision.retry_after_ms,
                limit: decision.limit,
                remaining: decision.remaining,
                reset_at_unix_ms: decision.reset_at_unix_ms,
            }
            .emit();
            state.metrics.on_rate_limit_event(
                "/v1/mcp/tool-call",
                match decision.reason {
                    crate::app::RateLimitReason::RequestBudgetExhausted => "requests",
                    crate::app::RateLimitReason::TokenBudgetExhausted => "token_units",
                    crate::app::RateLimitReason::WithinBudget => "requests",
                },
                if decision.allowed { "dry_run" } else { "blocked" },
                &api_key_profile,
            );
        }
        if !decision.allowed {
            return Err(enforce_mcp_error_contract(ApiError::rate_limit_exceeded(
                request_id,
                decision,
            )));
        }
    }

    let handler = state.mcp.handler.clone().ok_or_else(|| {
        enforce_mcp_error_contract(ApiError::invalid_request(
            request_id.clone(),
            "mcp proxy is not configured",
        ))
    })?;

    let response = handler
        .handle_tool_call(request_id.clone(), body, &api_key_profile)
        .await
        .map_err(|error| {
            let code = error.code().as_str();
            if code == "upstream_error" || code == "upstream_unavailable" {
                let error_class = match error.upstream_status() {
                    Some(status) if (400..500).contains(&status) => "upstream_4xx",
                    Some(status) if (500..600).contains(&status) => "upstream_5xx",
                    _ => "transport",
                };
                state
                    .metrics
                    .on_upstream_error("/v1/mcp/tool-call", "mcp", error_class);
            }
            ApiError::from_mcp_proxy(error)
        })?;

    Ok(Json(response))
}

pub async fn handle_mcp_tool_call_with_tool_name(
    Path(tool_name): Path<String>,
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Result<Json<serde_json::Value>, JsonRejection>,
) -> Result<Json<McpToolCallResponse>, ApiError> {
    let path_body = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;
    let path_body: McpToolInvokePathRequest =
        serde_json::from_value(path_body).map_err(|_| {
            enforce_mcp_error_contract(ApiError::invalid_request(
                request_id.clone(),
                "invalid request body",
            ))
        })?;
    let body = McpToolCallRequest {
        server: path_body.server,
        // The path-based invoke endpoint treats the URL segment as the source of truth for tool id.
        tool: tool_name,
        arguments: path_body.arguments,
        metadata: path_body.metadata,
    };

    handle_mcp_tool_call(State(state), Extension(request_id), headers, Ok(Json(body))).await
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

fn estimate_mcp_token_units(arguments: &serde_json::Value) -> u32 {
    let bytes = serde_json::to_vec(arguments).map(|value| value.len()).unwrap_or(0);
    ((bytes / 4) as u32).max(1)
}
