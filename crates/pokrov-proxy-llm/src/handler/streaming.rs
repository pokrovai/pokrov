use std::{ops::Range, sync::Arc, time::Instant};

use futures_util::StreamExt;
use pokrov_config::UpstreamAuthMode;
use pokrov_core::{
    rehydrate::{RehydrateReport, RehydrationMap},
    types::PolicyAction,
};
use serde_json::Value;

use crate::{
    errors::LLMProxyError,
    stream::{
        convert_chat_sse_to_responses_sse, rehydrate_chat_sse_stream, rehydrate_sse_stream,
        sanitize_chat_sse_stream, sanitize_sse_stream, ChatPolicyBody, ResponsesChunkConverter,
    },
    types::{
        LLMProxyBody, LLMProxyResponse, RouteResolution, SseBodyStream, UpstreamCredentialOrigin,
        UpstreamStreamResponse, RESPONSES_ENDPOINT,
    },
};

use super::{
    support::{max_action, mode_as_str, TerminalEvent},
    ErrorEventContext, LLMProxyHandler,
};

/// Endpoint that carries modern Chat Completions tool calls. The Responses
/// compatibility endpoint reuses this handler but must keep the default
/// buffered restoration shape.
const CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";

/// Post-policy buffered body plus the out-of-band source barrier that the
/// Chat restorer must forward through `bypass_complete_event`.
struct BufferedPolicyOutcome {
    body: String,
    first_source_barrier: Option<Range<usize>>,
    rule_hits_total: u32,
    final_action: PolicyAction,
}

/// Live passthrough rehydration and the deferred terminal audit it reports
/// through.
mod passthrough;

#[cfg(all(test, feature = "llm_payload_trace"))]
mod trace_tests;

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
        upstream_credential: Option<String>,
        rehydration_map: RehydrationMap,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let tokenized_spans_total = rehydration_map.spans_total();
        let upstream = self
            .upstream
            .execute_stream(&request_id, &route, &sanitized_payload, upstream_credential.as_deref())
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
                        auth_mode: mode_as_str(auth_mode),
                        credential_origin,
                        tokenized_spans_total,
                        rehydrated_tokens_total: 0,
                        unrestored_tokens_total: 0,
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
                            auth_mode: mode_as_str(auth_mode),
                            credential_origin,
                            tokenized_spans_total,
                            rehydrated_tokens_total: 0,
                            unrestored_tokens_total: 0,
                        },
                        &cause,
                    );
                    return Err(cause);
                }
            };

            let chat_buffered =
                endpoint == CHAT_COMPLETIONS_ENDPOINT && !rehydration_map.is_empty();
            let mut stream_body = body;
            let mut first_source_barrier = None;
            if let Some(evaluator) = self.evaluator.as_ref() {
                let evaluator = Arc::clone(evaluator);
                let request_id_for_task = request_id.clone();
                let profile_id_for_task = profile_id.clone();
                let body_for_task = stream_body.clone();
                let sanitized = match tokio::task::spawn_blocking(move || {
                    if chat_buffered {
                        sanitize_chat_sse_stream(
                            &request_id_for_task,
                            &profile_id_for_task,
                            &body_for_task,
                            evaluator.as_ref(),
                        )
                        .map(|sanitized| BufferedPolicyOutcome {
                            body: sanitized.body,
                            first_source_barrier: sanitized.first_source_barrier,
                            rule_hits_total: sanitized.rule_hits_total,
                            final_action: sanitized.final_action,
                        })
                    } else {
                        sanitize_sse_stream(
                            &request_id_for_task,
                            &profile_id_for_task,
                            &body_for_task,
                            evaluator.as_ref(),
                        )
                        .map(|sanitized| BufferedPolicyOutcome {
                            body: sanitized.body,
                            first_source_barrier: None,
                            rule_hits_total: sanitized.rule_hits_total,
                            final_action: sanitized.final_action,
                        })
                    }
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
                                auth_mode: mode_as_str(auth_mode),
                                credential_origin,
                                tokenized_spans_total,
                                rehydrated_tokens_total: 0,
                                unrestored_tokens_total: 0,
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
                                auth_mode: mode_as_str(auth_mode),
                                credential_origin,
                                tokenized_spans_total,
                                rehydrated_tokens_total: 0,
                                unrestored_tokens_total: 0,
                            },
                            &error,
                        );
                        return Err(error);
                    }
                };
                total_hits = total_hits.saturating_add(sanitized.rule_hits_total);
                final_action = max_action(final_action, sanitized.final_action);
                stream_body = sanitized.body;
                first_source_barrier = sanitized.first_source_barrier;
            }

            if endpoint == RESPONSES_ENDPOINT {
                stream_body = convert_chat_sse_to_responses_sse(&request_id, &stream_body)?;
            }

            // Restoration runs strictly after output policy evaluation, endpoint
            // conversion, and payload tracing so restored originals are never
            // re-scanned or traced.
            let (restored_body, report) = if chat_buffered {
                let map = rehydration_map;
                let barrier = first_source_barrier;
                self.finalize_buffered_sse(
                    &request_id,
                    &route,
                    endpoint,
                    stream_body,
                    move |body| {
                        rehydrate_chat_sse_stream(
                            ChatPolicyBody {
                                body,
                                first_source_barrier: barrier,
                                rule_hits_total: 0,
                                final_action: PolicyAction::Allow,
                            },
                            map,
                        )
                    },
                )
            } else {
                self.finalize_buffered_sse(&request_id, &route, endpoint, stream_body, |body| {
                    rehydrate_sse_stream(&body, &rehydration_map)
                })
            };
            stream_body = restored_body;
            if report.restored > 0 {
                self.metrics.on_rehydrated_tokens(report.restored);
            }
            if report.unrestored > 0 {
                self.metrics.on_unrestored_tokens(report.unrestored);
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
                tokenized_spans_total,
                rehydrated_tokens_total: report.restored,
                unrestored_tokens_total: report.unrestored,
            });

            return Ok(LLMProxyResponse {
                request_id,
                status,
                body: LLMProxyBody::Sse(stream_body),
            });
        }

        if endpoint == RESPONSES_ENDPOINT {
            let restored_stream = self.rehydrating_byte_stream(
                upstream_body.bytes_stream().boxed(),
                rehydration_map,
                self.stream_audit(
                    endpoint,
                    &request_id,
                    &profile_id,
                    &model,
                    &route,
                    final_action,
                    total_hits,
                    status.as_u16(),
                    started,
                    estimated_token_units,
                    auth_mode,
                    credential_origin,
                    tokenized_spans_total,
                ),
            );
            let converted_stream = converting_responses_stream(restored_stream, request_id.clone());

            return Ok(LLMProxyResponse {
                request_id,
                status,
                body: LLMProxyBody::SseStream(converted_stream),
            });
        }

        let audit = self.stream_audit(
            endpoint,
            &request_id,
            &profile_id,
            &model,
            &route,
            final_action,
            total_hits,
            status.as_u16(),
            started,
            estimated_token_units,
            auth_mode,
            credential_origin,
            tokenized_spans_total,
        );
        let restored_stream = self.rehydrating_byte_stream(
            upstream_body.bytes_stream().boxed(),
            rehydration_map,
            audit,
        );
        Ok(LLMProxyResponse { request_id, status, body: LLMProxyBody::SseStream(restored_stream) })
    }

    /// Emits the configured response payload trace for a post-policy buffered
    /// SSE body, then runs the endpoint's restoration pass. Trace emission
    /// stays ahead of restoration so restored originals never reach a
    /// diagnostic sink.
    fn finalize_buffered_sse<F>(
        &self,
        request_id: &str,
        route: &RouteResolution,
        endpoint: &str,
        body: String,
        restore: F,
    ) -> (String, RehydrateReport)
    where
        F: FnOnce(String) -> (String, RehydrateReport),
    {
        #[cfg(feature = "llm_payload_trace")]
        self.upstream.emit_response_trace(
            request_id,
            route,
            endpoint,
            &Value::String(body.clone()),
        );

        #[cfg(not(feature = "llm_payload_trace"))]
        let _ = (request_id, route, endpoint);

        restore(body)
    }
}

/// Wraps the rehydrated chat-SSE stream with conversion to `/v1/responses`
/// event shape. The converter's pending tail is flushed on end of stream and
/// ahead of a forwarded error so already-received bytes never vanish.
fn converting_responses_stream(stream: SseBodyStream, request_id: String) -> SseBodyStream {
    struct State {
        stream: SseBodyStream,
        converter: ResponsesChunkConverter,
        pending_error: Option<reqwest::Error>,
        done: bool,
    }

    futures_util::stream::unfold(
        State {
            stream,
            converter: ResponsesChunkConverter::new(&request_id),
            pending_error: None,
            done: false,
        },
        |mut state| async move {
            if let Some(error) = state.pending_error.take() {
                // Brief yield so the transport can flush the converter tail
                // before the error aborts the connection.
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                return Some((Err(error), state));
            }
            if state.done {
                return None;
            }
            match state.stream.next().await {
                Some(Ok(chunk)) => {
                    let converted = state.converter.feed(&chunk);
                    Some((Ok(bytes::Bytes::from(converted)), state))
                }
                Some(Err(error)) => {
                    let tail = state.converter.finish();
                    if tail.is_empty() {
                        Some((Err(error), state))
                    } else {
                        state.pending_error = Some(error);
                        Some((Ok(bytes::Bytes::from(tail)), state))
                    }
                }
                None => {
                    let tail = state.converter.finish();
                    if tail.is_empty() {
                        None
                    } else {
                        state.done = true;
                        Some((Ok(bytes::Bytes::from(tail)), state))
                    }
                }
            }
        },
    )
    .filter(|item| {
        std::future::ready(match item {
            Ok(bytes) => !bytes.is_empty(),
            Err(_) => true,
        })
    })
    .boxed()
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
