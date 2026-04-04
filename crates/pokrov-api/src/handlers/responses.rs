use axum::{
    body::Body,
    extract::{rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use pokrov_proxy_llm::audit::{LLMAuthStageAuditEvent, LLMRateLimitAuditEvent};
use pokrov_proxy_llm::types::LLMProxyBody;
use serde_json::Value;

use super::rate_limit::evaluate_and_record_rate_limit;
use crate::{
    app::{AppState, GatewayAuthContext, GatewayAuthMechanism},
    auth::{
        fingerprint_gateway_auth_subject, parse_bearer_token, parse_gateway_credential,
        parse_identity_from_headers, resolve_identity_from_sources,
    },
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
    let payload = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;
    let is_stream_request = payload.get("stream").and_then(Value::as_bool).unwrap_or(false);

    let mode_label = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => "static",
        pokrov_config::UpstreamAuthMode::Passthrough => "passthrough",
    };
    if !gateway_auth.authenticated {
        state
            .metrics
            .on_responses_auth_stage(mode_label, "gateway_auth", "fail");
        LLMAuthStageAuditEvent {
            request_id: request_id.clone(),
            endpoint: "/v1/responses",
            auth_mode: mode_label,
            stage: "gateway_auth",
            decision: "fail",
        }
        .emit();
        return Err(ApiError::gateway_unauthorized(request_id.clone()));
    }
    state
        .metrics
        .on_responses_auth_stage(mode_label, "gateway_auth", "pass");
    LLMAuthStageAuditEvent {
        request_id: request_id.clone(),
        endpoint: "/v1/responses",
        auth_mode: mode_label,
        stage: "gateway_auth",
        decision: "pass",
    }
    .emit();
    let (header_identity, ingress_identity) = parse_identity_from_headers(&headers);
    let gateway_auth_subject = gateway_auth.auth_subject.clone().unwrap_or_else(|| {
        parse_gateway_credential(&headers)
            .map(|credential| fingerprint_gateway_auth_subject(credential.token))
            .unwrap_or_else(|| "gateway_authenticated".to_string())
    });
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
        .or_else(|| {
            parse_gateway_credential(&headers)
                .and_then(|gateway| state.sanitization.profile_for_token(gateway.token))
        })
        .unwrap_or_else(|| "strict".to_string());
    let rate_limit_profile = state
        .auth
        .identity_rate_limit_bindings
        .get(client_identity)
        .cloned()
        .or_else(|| {
            parse_gateway_credential(&headers)
                .and_then(|gateway| state.sanitization.profile_for_token(gateway.token))
        })
        .unwrap_or_else(|| profile_id.clone());

    let upstream_credential = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => None,
        pokrov_config::UpstreamAuthMode::Passthrough => {
            let gateway_credential = parse_gateway_credential(&headers);
            let gateway_mechanism =
                gateway_credential.as_ref().map(|credential| credential.mechanism);
            let credential = match gateway_mechanism {
                Some(GatewayAuthMechanism::ApiKey) => parse_bearer_token(&headers)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        state
                            .metrics
                            .on_responses_auth_stage(mode_label, "upstream_credentials", "fail");
                        LLMAuthStageAuditEvent {
                            request_id: request_id.clone(),
                            endpoint: "/v1/responses",
                            auth_mode: mode_label,
                            stage: "upstream_credentials",
                            decision: "fail",
                        }
                        .emit();
                        ApiError::upstream_credential_missing(request_id.clone())
                    })?,
                // Codex compatibility path can forward a single bearer token for both
                // gateway authorization and upstream passthrough on /v1/responses.
                Some(GatewayAuthMechanism::Bearer) => gateway_credential
                    .map(|credential| credential.token.to_string())
                    .ok_or_else(|| ApiError::upstream_credential_missing(request_id.clone()))?,
                Some(GatewayAuthMechanism::InternalMtls)
                | Some(GatewayAuthMechanism::MeshMtls)
                | None => parse_bearer_token(&headers)
                    .map(str::to_string)
                    .ok_or_else(|| ApiError::upstream_credential_missing(request_id.clone()))?,
            };
            state
                .metrics
                .on_responses_auth_stage(mode_label, "upstream_credentials", "pass");
            LLMAuthStageAuditEvent {
                request_id: request_id.clone(),
                endpoint: "/v1/responses",
                auth_mode: mode_label,
                stage: "upstream_credentials",
                decision: "pass",
            }
            .emit();
            Some(credential)
        }
    };
    let rate_limit_key = client_identity.to_string();

    let estimated_units = estimate_responses_token_units(&payload);
    if let Some(decision) = evaluate_and_record_rate_limit(
        &state,
        "/v1/responses",
        &rate_limit_key,
        &rate_limit_profile,
        estimated_units,
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
        .handle_responses(
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
                state.metrics.on_responses_upstream_error(
                    error.provider_id().unwrap_or("unknown"),
                    error_class,
                );
            }
            if is_stream_request && error.code().as_str().starts_with("upstream_") {
                ApiError::responses_stream_terminated(request_id.clone())
            } else {
                ApiError::from_llm_proxy_for_responses(error)
            }
        })?;

    match response.body {
        LLMProxyBody::Json(body) => Ok((response.status, Json(map_chat_to_responses_envelope(body))).into_response()),
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

fn map_chat_to_responses_envelope(body: Value) -> Value {
    let request_id = body.get("request_id").cloned().unwrap_or(Value::Null);
    let pokrov = body.get("pokrov").cloned().unwrap_or(Value::Null);
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
        ("pokrov".to_string(), pokrov),
    ]);
    if let Some(model) = model {
        envelope.insert("model".to_string(), model);
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

    use super::{estimate_responses_token_units, map_chat_to_responses_envelope};

    #[test]
    fn wraps_chat_response_into_responses_output_envelope() {
        let chat = json!({
            "request_id":"req-1",
            "model":"gpt-4o-mini",
            "choices":[{"message":{"role":"assistant","content":"ok"}}],
            "pokrov":{"profile":"strict","action":"allow","sanitized_input":false,"sanitized_output":false,"rule_hits":0}
        });

        let responses = map_chat_to_responses_envelope(chat);
        assert_eq!(responses["request_id"], "req-1");
        assert_eq!(responses["output"][0]["type"], "message");
        assert_eq!(responses["output"][0]["content"][0]["text"], "ok");
        assert_eq!(responses["pokrov"]["profile"], "strict");
    }

    #[test]
    fn estimates_tokens_from_input_field_only() {
        let payload = json!({"input":["abcd","abcdefgh"]});
        assert_eq!(estimate_responses_token_units(&payload), 3);
    }
}
