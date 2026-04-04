use axum::{
    body::Body,
    extract::{rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use pokrov_proxy_llm::normalize::estimate_token_units;
use pokrov_proxy_llm::audit::{LLMAuthStageAuditEvent, LLMRateLimitAuditEvent};
use pokrov_proxy_llm::types::LLMProxyBody;
use serde_json::Value;

use super::rate_limit::evaluate_and_record_rate_limit;
use crate::{
    app::{AppState, GatewayAuthMechanism},
    auth::{
        fingerprint_gateway_auth_subject, parse_bearer_token, parse_gateway_credential,
        parse_identity_from_headers, resolve_identity_from_sources,
    },
    error::ApiError,
};

/// Handles OpenAI-compatible chat-completion requests through the policy and sanitization pipeline.
pub async fn handle_chat_completions(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Response, ApiError> {
    let payload = body
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
        LLMAuthStageAuditEvent {
            request_id: request_id.clone(),
            auth_mode: mode_label,
            stage: "gateway_auth",
            decision: "fail",
        }
        .emit();
        ApiError::gateway_unauthorized(request_id.clone())
    })?;

    let api_key_profile = state.sanitization.profile_for_token(gateway.token).ok_or_else(|| {
        state
            .metrics
            .on_auth_decision(mode_label, "gateway_auth", "fail");
        LLMAuthStageAuditEvent {
            request_id: request_id.clone(),
            auth_mode: mode_label,
            stage: "gateway_auth",
            decision: "fail",
        }
        .emit();
        ApiError::gateway_unauthorized(request_id.clone())
    })?;
    state
        .metrics
        .on_auth_decision(mode_label, "gateway_auth", "pass");
    LLMAuthStageAuditEvent {
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
                LLMAuthStageAuditEvent {
                    request_id: request_id.clone(),
                    auth_mode: mode_label,
                    stage: "upstream_credentials",
                    decision: "fail",
                }
                .emit();
                return Err(ApiError::passthrough_requires_api_key_gateway_auth(request_id));
            }

            let credential = parse_bearer_token(&headers).ok_or_else(|| {
                state
                    .metrics
                    .on_auth_decision(mode_label, "upstream_credentials", "fail");
                LLMAuthStageAuditEvent {
                    request_id: request_id.clone(),
                    auth_mode: mode_label,
                    stage: "upstream_credentials",
                    decision: "fail",
                }
                .emit();
                ApiError::upstream_credential_missing(request_id.clone())
            })?;
            state
                .metrics
                .on_auth_decision(mode_label, "upstream_credentials", "pass");
            LLMAuthStageAuditEvent {
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
        "/v1/chat/completions",
        &rate_limit_key,
        &rate_limit_profile,
        estimate_token_units(&payload),
    )
    .await
    {
        if !matches!(decision.reason, crate::app::RateLimitReason::WithinBudget) {
            LLMRateLimitAuditEvent {
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
            return Err(ApiError::rate_limit_exceeded(request_id, decision));
        }
    }

    let handler = state.llm.handler.clone().ok_or_else(|| {
        ApiError::invalid_request(request_id.clone(), "llm proxy is not configured")
    })?;

    let response = handler
        .handle_chat_completion(
            request_id.clone(),
            payload,
            &profile_id,
            state.auth.upstream_auth_mode,
            upstream_credential.as_deref(),
        )
        .await
        .map_err(|error| {
            if error.code().as_str().starts_with("upstream_") {
                let error_class = match error.upstream_status() {
                    Some(status) if (400..500).contains(&status) => "upstream_4xx",
                    Some(status) if (500..600).contains(&status) => "upstream_5xx",
                    _ => "transport",
                };
                state.metrics.on_upstream_error(
                    "/v1/chat/completions",
                    error.provider_id().unwrap_or("unknown"),
                    error_class,
                );
            }
            ApiError::from_llm_proxy(error)
        })?;

    match response.body {
        LLMProxyBody::Json(body) => Ok((response.status, Json(body)).into_response()),
        LLMProxyBody::Sse(body) => {
            let mut sse_response = Response::new(Body::from(body));
            *sse_response.status_mut() = response.status;
            sse_response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            sse_response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            );
            sse_response.headers_mut().insert(
                header::CONNECTION,
                HeaderValue::from_static("keep-alive"),
            );

            Ok(sse_response)
        }
        LLMProxyBody::SseStream(stream) => {
            let mut sse_response = Response::new(Body::from_stream(stream));
            *sse_response.status_mut() = response.status;
            sse_response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            sse_response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            );
            sse_response.headers_mut().insert(
                header::CONNECTION,
                HeaderValue::from_static("keep-alive"),
            );

            Ok(sse_response)
        }
    }
}

fn map_json_rejection(request_id: String, rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::BytesRejection(_) => {
            ApiError::payload_too_large(request_id, "request body exceeds configured size limit")
        }
        _ => ApiError::invalid_request(request_id, "invalid request body"),
    }
}
