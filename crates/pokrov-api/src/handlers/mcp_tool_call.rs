use axum::{
    extract::{Path, rejection::JsonRejection, Extension, State},
    http::HeaderMap,
    Json,
};
use pokrov_proxy_mcp::types::{McpRequestMetadata, McpToolCallRequest, McpToolCallResponse};
use pokrov_proxy_mcp::audit::{McpAuthStageAuditEvent, McpRateLimitAuditEvent};
use serde::Deserialize;

use super::rate_limit::{estimate_json_token_units, evaluate_and_record_rate_limit};
use crate::{
    app::{AppState, GatewayAuthMechanism},
    auth::{
        fingerprint_gateway_auth_subject, parse_bearer_token, parse_gateway_credential,
        parse_identity_from_headers, resolve_identity_from_sources,
    },
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
    headers: HeaderMap,
    body: Result<Json<McpToolCallRequest>, JsonRejection>,
) -> Result<Json<McpToolCallResponse>, ApiError> {
    let body = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;

    let mode_label = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => "static",
        pokrov_config::UpstreamAuthMode::Passthrough => "passthrough",
    };
    let gateway = parse_gateway_credential(&headers).ok_or_else(|| {
        state
            .metrics
            .on_auth_decision(mode_label, "gateway_auth", "fail");
        McpAuthStageAuditEvent {
            request_id: request_id.clone(),
            auth_mode: mode_label,
            stage: "gateway_auth",
            decision: "fail",
        }
        .emit();
        enforce_mcp_error_contract(ApiError::gateway_unauthorized(request_id.clone()))
    })?;

    let api_key_profile = state.sanitization.profile_for_token(gateway.token).ok_or_else(|| {
        state
            .metrics
            .on_auth_decision(mode_label, "gateway_auth", "fail");
        McpAuthStageAuditEvent {
            request_id: request_id.clone(),
            auth_mode: mode_label,
            stage: "gateway_auth",
            decision: "fail",
        }
        .emit();
        enforce_mcp_error_contract(ApiError::gateway_unauthorized(request_id.clone()))
    })?;
    state
        .metrics
        .on_auth_decision(mode_label, "gateway_auth", "pass");
    McpAuthStageAuditEvent {
        request_id: request_id.clone(),
        auth_mode: mode_label,
        stage: "gateway_auth",
        decision: "pass",
    }
    .emit();
    let (header_identity, ingress_identity) = parse_identity_from_headers(&headers);
    let gateway_auth_subject = fingerprint_gateway_auth_subject(gateway.token);
    let client_identity = resolve_identity_from_sources(
        state.auth.identity_resolution_order.as_slice(),
        header_identity,
        ingress_identity,
        Some(gateway_auth_subject.as_str()),
    )
    .unwrap_or(gateway_auth_subject.as_str());
    let profile_id = state
        .auth
        .identity_profile_bindings
        .get(client_identity)
        .cloned()
        .or_else(|| state.auth.fallback_policy_profile.clone())
        .unwrap_or_else(|| api_key_profile.clone());
    let rate_limit_profile = state
        .auth
        .identity_rate_limit_bindings
        .get(client_identity)
        .cloned()
        .unwrap_or_else(|| api_key_profile.clone());

    let upstream_credential = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => None,
        pokrov_config::UpstreamAuthMode::Passthrough => {
            if !matches!(gateway.mechanism, GatewayAuthMechanism::ApiKey) {
                state
                    .metrics
                    .on_auth_decision(mode_label, "upstream_credentials", "fail");
                McpAuthStageAuditEvent {
                    request_id: request_id.clone(),
                    auth_mode: mode_label,
                    stage: "upstream_credentials",
                    decision: "fail",
                }
                .emit();
                return Err(enforce_mcp_error_contract(
                    ApiError::passthrough_requires_api_key_gateway_auth(request_id),
                ));
            }
            let credential = parse_bearer_token(&headers).ok_or_else(|| {
                state
                    .metrics
                    .on_auth_decision(mode_label, "upstream_credentials", "fail");
                McpAuthStageAuditEvent {
                    request_id: request_id.clone(),
                    auth_mode: mode_label,
                    stage: "upstream_credentials",
                    decision: "fail",
                }
                .emit();
                enforce_mcp_error_contract(ApiError::upstream_credential_missing(
                    request_id.clone(),
                ))
            })?;
            state
                .metrics
                .on_auth_decision(mode_label, "upstream_credentials", "pass");
            McpAuthStageAuditEvent {
                request_id: request_id.clone(),
                auth_mode: mode_label,
                stage: "upstream_credentials",
                decision: "pass",
            }
            .emit();
            Some(credential.to_string())
        }
    };
    let rate_limit_key = client_identity.to_string();

    if let Some(decision) = evaluate_and_record_rate_limit(
        &state,
        "/v1/mcp/tool-call",
        &rate_limit_key,
        &rate_limit_profile,
        estimate_json_token_units(&body.arguments),
    )
    .await
    {
        if !matches!(decision.reason, crate::app::RateLimitReason::WithinBudget) {
            McpRateLimitAuditEvent {
                request_id: request_id.clone(),
                profile_id: profile_id.clone(),
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
            &profile_id,
            mode_label,
            upstream_credential.as_deref(),
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

/// Handles path-based MCP invocations where `tool_name` is provided by the route segment.
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
