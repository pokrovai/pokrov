use axum::{
    extract::{Path, rejection::JsonRejection, Extension, State},
    http::HeaderMap,
    Json,
};
use pokrov_proxy_mcp::types::{McpRequestMetadata, McpToolCallRequest, McpToolCallResponse};
use pokrov_proxy_mcp::audit::{McpAuthStageAuditEvent, McpRateLimitAuditEvent};
use serde::Deserialize;

use super::request_context::{RequestContextHooks, resolve_request_context};
use super::rate_limit::{estimate_json_token_units, evaluate_and_record_rate_limit};
use crate::{
    app::{AppState, GatewayAuthContext},
    error::ApiError,
};

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
    Extension(gateway_auth): Extension<GatewayAuthContext>,
    headers: HeaderMap,
    body: Result<Json<McpToolCallRequest>, JsonRejection>,
) -> Result<Json<McpToolCallResponse>, ApiError> {
    let body = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;

    let context = resolve_request_context(
        &state,
        &headers,
        &gateway_auth,
        &request_id,
        "/v1/mcp/tool-call",
        &RequestContextHooks {
            on_auth_stage: on_auth_stage,
            emit_auth_stage: emit_auth_stage,
            map_error: map_error,
        },
    )?;

    if let Some(decision) = evaluate_and_record_rate_limit(
        &state,
        "/v1/mcp/tool-call",
        &context.rate_limit_key,
        &context.rate_limit_profile,
        estimate_json_token_units(&body.arguments),
    )
    .await
    {
        if !matches!(decision.reason, crate::app::RateLimitReason::WithinBudget) {
            McpRateLimitAuditEvent {
                request_id: request_id.clone(),
                profile_id: context.profile_id.clone(),
                decision: if decision.allowed { "dry_run" } else { "blocked" }.to_string(),
                retry_after_ms: decision.retry_after_ms,
                limit: decision.limit,
                remaining: decision.remaining,
                reset_at_unix_ms: decision.reset_at_unix_ms,
            }
            .emit();
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
        .handle_tool_call(
            request_id.clone(),
            body,
            &context.profile_id,
            context.mode_label,
            context.upstream_credential.as_deref(),
        )
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

fn on_auth_stage(state: &AppState, mode: &'static str, stage: &'static str, decision: &'static str) {
    state.metrics.on_auth_decision(mode, stage, decision);
}

fn emit_auth_stage(
    request_id: &str,
    _endpoint: &'static str,
    mode: &'static str,
    stage: &'static str,
    decision: &'static str,
) {
    McpAuthStageAuditEvent {
        request_id: request_id.to_string(),
        auth_mode: mode,
        stage,
        decision,
    }
    .emit();
}

fn map_error(error: ApiError) -> ApiError {
    enforce_mcp_error_contract(error)
}

/// Handles path-based MCP invocations where `tool_name` is provided by the route segment.
pub async fn handle_mcp_tool_call_with_tool_name(
    Path(tool_name): Path<String>,
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    Extension(gateway_auth): Extension<GatewayAuthContext>,
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

    handle_mcp_tool_call(
        State(state),
        Extension(request_id),
        Extension(gateway_auth),
        headers,
        Ok(Json(body)),
    )
    .await
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
