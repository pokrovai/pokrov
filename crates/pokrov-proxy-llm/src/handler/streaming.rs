use std::{sync::Arc, time::Instant};

use futures_util::StreamExt;
use pokrov_config::UpstreamAuthMode;
use pokrov_core::types::PolicyAction;
use serde_json::Value;

use crate::{
    errors::LLMProxyError,
    stream::{
        convert_chat_sse_chunk_to_responses_chunk, convert_chat_sse_to_responses_sse,
        sanitize_sse_stream,
    },
    types::{
        LLMProxyBody, LLMProxyResponse, RouteResolution, UpstreamCredentialOrigin,
        UpstreamStreamResponse, RESPONSES_ENDPOINT,
    },
};

use super::{
    support::{max_action, mode_as_str, TerminalEvent},
    ErrorEventContext, LLMProxyHandler,
};

impl LLMProxyHandler {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_stream_response(
        &self,
        started: Instant,
        endpoint: &'static str,
        request_id: String,
        profile_id: String,
        model: String,
        route: RouteResolution,
        sanitized_payload: Value,
        mut final_action: PolicyAction,
        mut total_hits: u32,
        _sanitized_input: bool,
        estimated_token_units: u32,
        auth_mode: UpstreamAuthMode,
        credential_origin: UpstreamCredentialOrigin,
        upstream_credential: String,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let upstream = self
            .upstream
            .execute_stream(
                &request_id,
                &route,
                &sanitized_payload,
                Some(upstream_credential.as_str()),
            )
            .await;

        let UpstreamStreamResponse { status, body: upstream_body } = match upstream {
            Ok(response) => response,
            Err(error) => {
                self.emit_error_event(
                    ErrorEventContext {
                        started,
                        endpoint,
                        request_id: &request_id,
                        profile_id: &profile_id,
                        provider_id: Some(route.provider_id.clone()),
                        model: &model,
                        stream: true,
                        final_action,
                        total_hits,
                        upstream_status: error.upstream_status(),
                    },
                    &error,
                );
                return Err(error);
            }
        };

        if route.output_sanitization {
            let body = match read_stream_body_with_limit(
                &request_id,
                &route.provider_id,
                upstream_body,
                route.stream_sanitization_max_buffer_bytes,
            )
            .await
            {
                Ok(body) => body,
                Err(cause) => {
                    self.emit_error_event(
                        ErrorEventContext {
                            started,
                            endpoint,
                            request_id: &request_id,
                            profile_id: &profile_id,
                            provider_id: Some(route.provider_id.clone()),
                            model: &model,
                            stream: true,
                            final_action,
                            total_hits,
                            upstream_status: Some(status.as_u16()),
                        },
                        &cause,
                    );
                    return Err(cause);
                }
            };

            let mut stream_body = body;
            if route.output_sanitization {
                if let Some(evaluator) = self.evaluator.as_ref() {
                    let evaluator = Arc::clone(evaluator);
                    let request_id_for_task = request_id.clone();
                    let profile_id_for_task = profile_id.clone();
                    let body_for_task = stream_body.clone();
                    let sanitized = match tokio::task::spawn_blocking(move || {
                        sanitize_sse_stream(
                            &request_id_for_task,
                            &profile_id_for_task,
                            &body_for_task,
                            evaluator.as_ref(),
                        )
                    })
                    .await
                    {
                        Ok(Ok(sanitized)) => sanitized,
                        Ok(Err(error)) => {
                            self.emit_error_event(
                                ErrorEventContext {
                                    started,
                                    endpoint,
                                    request_id: &request_id,
                                    profile_id: &profile_id,
                                    provider_id: Some(route.provider_id.clone()),
                                    model: &model,
                                    stream: true,
                                    final_action,
                                    total_hits,
                                    upstream_status: Some(status.as_u16()),
                                },
                                &error,
                            );
                            return Err(error);
                        }
                        Err(join_error) => {
                            let error = LLMProxyError::upstream_error(
                                request_id.clone(),
                                Some(route.provider_id.clone()),
                                format!("failed to execute stream sanitization task: {join_error}"),
                            );
                            self.emit_error_event(
                                ErrorEventContext {
                                    started,
                                    endpoint,
                                    request_id: &request_id,
                                    profile_id: &profile_id,
                                    provider_id: Some(route.provider_id.clone()),
                                    model: &model,
                                    stream: true,
                                    final_action,
                                    total_hits,
                                    upstream_status: Some(status.as_u16()),
                                },
                                &error,
                            );
                            return Err(error);
                        }
                    };
                    total_hits = total_hits.saturating_add(sanitized.rule_hits_total);
                    final_action = max_action(final_action, sanitized.final_action);
                    stream_body = sanitized.body;
                }
            }

            if endpoint == RESPONSES_ENDPOINT {
                stream_body = convert_chat_sse_to_responses_sse(&request_id, &stream_body)?;
            }

            self.emit_terminal_event(TerminalEvent {
                request_id: &request_id,
                endpoint,
                profile_id: &profile_id,
                provider_id: Some(route.provider_id.clone()),
                model: &model,
                stream: true,
                final_action,
                rule_hits_total: total_hits,
                blocked: false,
                upstream_status: Some(status.as_u16()),
                duration_ms: started.elapsed().as_millis() as u64,
                estimated_token_units,
                auth_mode: mode_as_str(auth_mode),
                credential_origin,
            });

            return Ok(LLMProxyResponse {
                request_id,
                status,
                body: LLMProxyBody::Sse(stream_body),
            });
        }

        if endpoint == RESPONSES_ENDPOINT {
            let mut pending_bytes = Vec::new();
            let request_id_for_stream = request_id.clone();
            let converted_stream = upstream_body
                .bytes_stream()
                .map(move |chunk_result| match chunk_result {
                    Ok(chunk) => Ok(bytes::Bytes::from(convert_chat_sse_chunk_to_responses_chunk(
                        request_id_for_stream.as_str(),
                        &mut pending_bytes,
                        chunk.as_ref(),
                    ))),
                    Err(error) => Err(error),
                })
                .filter_map(|item| async move {
                    match item {
                        Ok(chunk) if chunk.is_empty() => None,
                        other => Some(other),
                    }
                });

            self.emit_terminal_event(TerminalEvent {
                request_id: &request_id,
                endpoint,
                profile_id: &profile_id,
                provider_id: Some(route.provider_id.clone()),
                model: &model,
                stream: true,
                final_action,
                rule_hits_total: total_hits,
                blocked: false,
                upstream_status: Some(status.as_u16()),
                duration_ms: started.elapsed().as_millis() as u64,
                estimated_token_units,
                auth_mode: mode_as_str(auth_mode),
                credential_origin,
            });

            return Ok(LLMProxyResponse {
                request_id,
                status,
                body: LLMProxyBody::SseStream(Box::pin(converted_stream)),
            });
        }

        self.emit_terminal_event(TerminalEvent {
            request_id: &request_id,
            endpoint,
            profile_id: &profile_id,
            provider_id: Some(route.provider_id.clone()),
            model: &model,
            stream: true,
            final_action,
            rule_hits_total: total_hits,
            blocked: false,
            upstream_status: Some(status.as_u16()),
            duration_ms: started.elapsed().as_millis() as u64,
            estimated_token_units,
            auth_mode: mode_as_str(auth_mode),
            credential_origin,
        });

        Ok(LLMProxyResponse {
            request_id,
            status,
            body: LLMProxyBody::SseStream(Box::pin(upstream_body.bytes_stream())),
        })
    }
}

async fn read_stream_body_with_limit(
    request_id: &str,
    provider_id: &str,
    upstream_body: reqwest::Response,
    max_buffer_bytes: usize,
) -> Result<String, LLMProxyError> {
    let mut bytes = Vec::new();
    let mut stream = upstream_body.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            LLMProxyError::upstream_error(
                request_id,
                Some(provider_id.to_string()),
                format!("failed to read stream body chunk: {error}"),
            )
        })?;
        let next_len = bytes.len().saturating_add(chunk.len());

        if next_len > max_buffer_bytes {
            return Err(LLMProxyError::upstream_error(
                request_id,
                Some(provider_id.to_string()),
                format!(
                    "sanitized stream buffer exceeded configured limit of {max_buffer_bytes} bytes"
                ),
            ));
        }

        bytes.extend_from_slice(&chunk);
    }

    String::from_utf8(bytes).map_err(|error| {
        LLMProxyError::upstream_error(
            request_id,
            Some(provider_id.to_string()),
            format!("failed to decode stream body as utf-8: {error}"),
        )
    })
}
