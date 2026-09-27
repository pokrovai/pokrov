use std::{sync::Arc, time::Instant};

use futures_util::StreamExt;
use pokrov_config::UpstreamAuthMode;
use pokrov_core::{
    rehydrate::{RehydrateReport, RehydrationMap, SseStreamRehydrator},
    types::PolicyAction,
};
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;
use serde_json::Value;

use crate::{
    audit::LLMAuditEvent,
    errors::LLMProxyError,
    stream::{
        convert_chat_sse_to_responses_sse, rehydrate_sse_stream, sanitize_sse_stream,
        ResponsesChunkConverter,
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
                }
            }

            if endpoint == RESPONSES_ENDPOINT {
                stream_body = convert_chat_sse_to_responses_sse(&request_id, &stream_body)?;
            }

            #[cfg(feature = "llm_payload_trace")]
            self.upstream.emit_response_trace(
                &request_id,
                &route,
                endpoint,
                &Value::String(stream_body.clone()),
            );

            // Rehydration runs strictly after output policy evaluation, endpoint
            // conversion, and payload tracing so restored originals are never
            // re-scanned or traced.
            let (restored_body, report) = rehydrate_sse_stream(&stream_body, &rehydration_map);
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
            let converted_stream =
                converting_responses_stream(restored_stream, request_id.clone());

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

    /// Builds the deferred terminal-audit context carried inside a passthrough
    /// stream; restoration happens per event during the body, so real
    /// restored/unrestored totals exist only once the stream drains or fails.
    #[allow(clippy::too_many_arguments)]
    fn stream_audit(
        &self,
        endpoint: &'static str,
        request_id: &str,
        profile_id: &str,
        model: &str,
        route: &RouteResolution,
        final_action: PolicyAction,
        total_hits: u32,
        upstream_status: u16,
        started: Instant,
        estimated_token_units: u32,
        auth_mode: UpstreamAuthMode,
        credential_origin: UpstreamCredentialOrigin,
        tokenized_spans_total: u32,
    ) -> StreamAuditContext {
        StreamAuditContext {
            request_id: request_id.to_string(),
            endpoint,
            profile_id: profile_id.to_string(),
            provider_id: Some(route.provider_id.clone()),
            model: model.to_string(),
            final_action,
            rule_hits_total: total_hits,
            upstream_status: Some(upstream_status),
            started,
            estimated_token_units,
            auth_mode: mode_as_str(auth_mode),
            credential_origin,
            tokenized_spans_total,
            metrics: self.metrics.clone(),
            last_report: RehydrateReport::default(),
            emitted: false,
        }
    }

    /// Wraps a passthrough byte stream with SSE-aware `[PKV_TOKEN]`
    /// restoration: events are buffered to their blank-line terminator so
    /// `data:` JSON stays valid under leaf-wise restore and tokens split
    /// across chunks or delta events still resolve. The terminal audit event
    /// is emitted from inside the stream once it drains, errors, or is
    /// dropped, carrying the real restored/unrestored totals (FR-010).
    fn rehydrating_byte_stream(
        &self,
        stream: SseBodyStream,
        map: RehydrationMap,
        audit: StreamAuditContext,
    ) -> SseBodyStream {
        if map.is_empty() {
            // Nothing can be restored; dropping `audit` emits the terminal
            // event synchronously with correct zero totals.
            return stream;
        }

        struct State {
            stream: SseBodyStream,
            rehydrator: SseStreamRehydrator,
            audit: StreamAuditContext,
            pending_error: Option<reqwest::Error>,
            done: bool,
            reported_restored: u32,
            reported_unrestored: u32,
        }

        impl State {
            // Totals are cumulative; only the delta since the previous chunk
            // is reported to keep metrics additive. The latest report is also
            // snapshot into the audit context for deferred emission.
            fn report(&mut self) {
                let report = self.rehydrator.report();
                if report.restored > self.reported_restored {
                    self.audit
                        .metrics
                        .on_rehydrated_tokens(report.restored - self.reported_restored);
                    self.reported_restored = report.restored;
                }
                if report.unrestored > self.reported_unrestored {
                    self.audit
                        .metrics
                        .on_unrestored_tokens(report.unrestored - self.reported_unrestored);
                    self.reported_unrestored = report.unrestored;
                }
                self.audit.last_report = report;
            }
        }

        futures_util::stream::unfold(
            State {
                stream,
                rehydrator: SseStreamRehydrator::new(map),
                audit,
                pending_error: None,
                done: false,
                reported_restored: 0,
                reported_unrestored: 0,
            },
            move |mut state| async move {
                if let Some(error) = state.pending_error.take() {
                    // Brief yield so the transport can flush the just-emitted
                    // buffered tail before the error aborts the connection.
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    state.audit.emit();
                    return Some((Err(error), state));
                }
                if state.done {
                    return None;
                }
                match state.stream.next().await {
                    Some(Ok(chunk)) => {
                        let emitted = state.rehydrator.feed(&chunk);
                        state.report();
                        Some((Ok(bytes::Bytes::from(emitted)), state))
                    }
                    Some(Err(error)) => {
                        // Buffered bytes and pending token-prefix carries must
                        // reach the client before the error propagates.
                        let tail = state.rehydrator.drain();
                        state.report();
                        if tail.is_empty() {
                            state.audit.emit();
                            Some((Err(error), state))
                        } else {
                            state.pending_error = Some(error);
                            Some((Ok(bytes::Bytes::from(tail)), state))
                        }
                    }
                    None => {
                        let tail = state.rehydrator.finish();
                        state.report();
                        state.audit.emit();
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
}

/// Terminal audit fields captured for deferred emission inside a passthrough
/// stream. `Drop` emits best-effort with the last observed totals so a
/// client-disconnected body never vanishes from the audit trail.
struct StreamAuditContext {
    request_id: String,
    endpoint: &'static str,
    profile_id: String,
    provider_id: Option<String>,
    model: String,
    final_action: PolicyAction,
    rule_hits_total: u32,
    upstream_status: Option<u16>,
    started: Instant,
    estimated_token_units: u32,
    auth_mode: &'static str,
    credential_origin: UpstreamCredentialOrigin,
    tokenized_spans_total: u32,
    metrics: SharedRuntimeMetricsHooks,
    last_report: RehydrateReport,
    emitted: bool,
}

impl StreamAuditContext {
    fn emit(&mut self) {
        if self.emitted {
            return;
        }
        self.emitted = true;
        let duration_ms = self.started.elapsed().as_millis() as u64;
        LLMAuditEvent {
            request_id: self.request_id.clone(),
            endpoint: self.endpoint.to_string(),
            profile_id: self.profile_id.clone(),
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
            stream: true,
            final_action: self.final_action,
            rule_hits_total: self.rule_hits_total,
            blocked: false,
            upstream_status: self.upstream_status,
            duration_ms,
            estimated_token_units: self.estimated_token_units,
            auth_mode: self.auth_mode.to_string(),
            credential_origin: self.credential_origin,
            tokenized_spans_total: self.tokenized_spans_total,
            rehydrated_tokens_total: self.last_report.restored,
            unrestored_tokens_total: self.last_report.unrestored,
        }
        .emit();
        self.metrics.on_llm_final_action(self.final_action);
        if let Some(status) = self.upstream_status {
            self.metrics.on_llm_upstream_status(status);
        }
        self.metrics.on_llm_request_duration_ms(duration_ms);
    }
}

impl Drop for StreamAuditContext {
    fn drop(&mut self) {
        self.emit();
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
