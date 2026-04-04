use axum::{
    body::Body,
    extract::{rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use pokrov_config::model::ResponseMetadataMode;
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

/// Handles Codex-compatible responses requests via deterministic mapping to chat-completions flow.
pub async fn handle_responses(
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
    let is_stream_request = payload.get("stream").and_then(Value::as_bool).unwrap_or(false);

    let context = resolve_request_context(
        &state,
        &headers,
        &gateway_auth,
        &request_id,
        "/v1/responses",
        UpstreamCredentialRequirement::Required,
        &RequestContextHooks {
            on_auth_stage: on_auth_stage,
            emit_auth_stage: emit_auth_stage,
            map_error: map_error,
        },
    )
    .map_err(|error| error.with_response_metadata_mode(metadata_mode))?;

    let estimated_units = estimate_responses_token_units(&payload);
    if let Some(decision) = evaluate_and_record_rate_limit(
        &state,
        "/v1/responses",
        &context.rate_limit_key,
        &context.rate_limit_profile,
        estimated_units,
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
        .handle_responses(
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
                state.metrics.on_responses_upstream_error(
                    error.provider_id().unwrap_or("unknown"),
                    error_class,
                );
            }
            if is_stream_request && error.code().as_str().starts_with("upstream_") {
                ApiError::responses_stream_terminated(request_id.clone())
                    .with_response_metadata_mode(metadata_mode)
            } else {
                ApiError::from_llm_proxy_for_responses(error)
                    .with_response_metadata_mode(metadata_mode)
            }
        })?;

    match response.body {
        LLMProxyBody::Json(body) => Ok((
            response.status,
            Json(map_chat_to_responses_envelope(
                body,
                state.llm.response_metadata_mode,
            )),
        )
            .into_response()),
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
    state.metrics.on_responses_auth_stage(mode, stage, decision);
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

fn map_chat_to_responses_envelope(body: Value, metadata_mode: ResponseMetadataMode) -> Value {
    let request_id = body.get("request_id").cloned().unwrap_or(Value::Null);
    let pokrov = body.get("pokrov").cloned();
    let model = body.get("model").cloned();
    let output = body
        .get("choices")
        .and_then(Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .filter_map(|choice| choice.get("message"))
                .map(|message| {
                    let role = message
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("assistant")
                        .to_string();
                    let text = message
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    serde_json::json!({
                        "type": "message",
                        "role": role,
                        "content": [{"type":"output_text","text":text}],
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut envelope = serde_json::Map::from_iter([
        ("request_id".to_string(), request_id),
        ("output".to_string(), Value::Array(output)),
    ]);
    if let Some(model) = model {
        envelope.insert("model".to_string(), model);
    }
    if metadata_mode == ResponseMetadataMode::Enabled {
        if let Some(pokrov) = pokrov {
            envelope.insert("pokrov".to_string(), pokrov);
        }
    }
    Value::Object(envelope)
}

fn estimate_responses_token_units(payload: &Value) -> u32 {
    match payload {
        Value::Object(map) => map
            .get("input")
            .map(estimate_responses_token_units)
            .unwrap_or(1),
        Value::String(text) => ((text.chars().count() as u32) / 4).max(1),
        Value::Array(items) => items
            .iter()
            .fold(0u32, |acc, item| acc.saturating_add(estimate_responses_token_units(item)))
            .max(1),
        Value::Number(_) => 1,
        Value::Bool(_) | Value::Null => 1,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use pokrov_config::model::ResponseMetadataMode;

    use super::{estimate_responses_token_units, map_chat_to_responses_envelope};

    #[test]
    fn wraps_chat_response_into_responses_output_envelope() {
        let chat = json!({
            "request_id":"req-1",
            "model":"gpt-4o-mini",
            "choices":[{"message":{"role":"assistant","content":"ok"}}],
            "pokrov":{"profile":"strict","action":"allow","sanitized_input":false,"sanitized_output":false,"rule_hits":0}
        });

        let responses = map_chat_to_responses_envelope(chat, ResponseMetadataMode::Enabled);
        assert_eq!(responses["request_id"], "req-1");
        assert_eq!(responses["output"][0]["type"], "message");
        assert_eq!(responses["output"][0]["content"][0]["text"], "ok");
        assert_eq!(responses["pokrov"]["profile"], "strict");
    }

    #[test]
    fn omits_proxy_metadata_in_suppressed_mode() {
        let chat = json!({
            "request_id":"req-1",
            "choices":[{"message":{"role":"assistant","content":"ok"}}],
            "pokrov":{"profile":"strict"}
        });
        let responses = map_chat_to_responses_envelope(chat, ResponseMetadataMode::Suppressed);
        assert!(responses.get("pokrov").is_none());
    }

    #[test]
    fn estimates_tokens_from_input_field_only() {
        let payload = json!({"input":["abcd","abcdefgh"]});
        assert_eq!(estimate_responses_token_units(&payload), 3);
    }
}
