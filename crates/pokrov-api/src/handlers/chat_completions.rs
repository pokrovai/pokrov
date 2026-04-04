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

use super::request_context::{
    RequestContextHooks, UpstreamCredentialRequirement, resolve_request_context,
};
use super::rate_limit::evaluate_and_record_rate_limit;
use crate::{
    app::{AppState, GatewayAuthContext},
    error::ApiError,
};

/// Handles OpenAI-compatible chat-completion requests through the policy and sanitization pipeline.
pub async fn handle_chat_completions(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    Extension(gateway_auth): Extension<GatewayAuthContext>,
    headers: HeaderMap,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Response, ApiError> {
    let metadata_mode = state.llm.response_metadata_mode;
    let payload = body
        .map(|Json(body)| body)
        .map_err(|rejection| {
            map_json_rejection(request_id.clone(), rejection).with_response_metadata_mode(metadata_mode)
        })?;

    let context = resolve_request_context(
        &state,
        &headers,
        &gateway_auth,
        &request_id,
        "/v1/chat/completions",
        UpstreamCredentialRequirement::Required,
        &RequestContextHooks {
            on_auth_stage: on_auth_stage,
            emit_auth_stage: emit_auth_stage,
            map_error: map_error,
        },
    )
    .map_err(|error| error.with_response_metadata_mode(metadata_mode))?;

    if let Some(decision) = evaluate_and_record_rate_limit(
        &state,
        "/v1/chat/completions",
        &context.rate_limit_key,
        &context.rate_limit_profile,
        estimate_token_units(&payload),
    )
    .await
    {
        if !matches!(decision.reason, crate::app::RateLimitReason::WithinBudget) {
            LLMRateLimitAuditEvent {
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
            return Err(
                ApiError::rate_limit_exceeded(request_id, decision)
                    .with_response_metadata_mode(metadata_mode),
            );
        }
    }

    let handler = state.llm.handler.clone().ok_or_else(|| {
        ApiError::runtime_not_ready(request_id.clone(), "llm proxy is not ready")
            .with_response_metadata_mode(metadata_mode)
    })?;

    let response = handler
        .handle_chat_completion(
            request_id.clone(),
            payload,
            &context.profile_id,
            state.auth.upstream_auth_mode,
            context.upstream_credential.as_deref(),
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
            ApiError::from_llm_proxy(error).with_response_metadata_mode(metadata_mode)
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
    LLMAuthStageAuditEvent {
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

fn map_json_rejection(request_id: String, rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::BytesRejection(_) => {
            ApiError::payload_too_large(request_id, "request body exceeds configured size limit")
        }
        _ => ApiError::invalid_request(request_id, "invalid request body"),
    }
}
