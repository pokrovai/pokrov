//! Live passthrough stream assembly for the LLM proxy.
//!
//! This module owns the deferred terminal audit and the per-chunk SSE
//! rehydration wrapper. Chat Completions streams additionally enable the
//! tool-argument mode of the shared byte driver; every other endpoint keeps
//! the default event-or-text driver.

use std::time::Instant;

use futures_util::StreamExt;
use pokrov_config::UpstreamAuthMode;
use pokrov_core::{
    rehydrate::{RehydrateReport, RehydrationMap, SseStreamRehydrator},
    types::PolicyAction,
};
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;

use crate::{
    audit::LLMAuditEvent,
    types::{RouteResolution, SseBodyStream, UpstreamCredentialOrigin},
};

use super::super::{support::mode_as_str, LLMProxyHandler};

/// Client-visible endpoint that carries modern Chat Completions tool calls.
/// Tool-argument restoration must never run for the Responses compatibility
/// endpoint, which reuses the same handler internals.
const CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";

impl LLMProxyHandler {
    /// Builds the deferred terminal-audit context carried inside a passthrough
    /// stream; restoration happens per event during the body, so real
    /// restored/unrestored totals exist only once the stream drains or fails.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn stream_audit(
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
    ///
    /// The Chat Completions driver is selected only after the empty-map fast
    /// path: without request tokens there is nothing to restore and the raw
    /// stream must pass through untouched.
    pub(super) fn rehydrating_byte_stream(
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

        let rehydrator = if audit.endpoint == CHAT_COMPLETIONS_ENDPOINT {
            SseStreamRehydrator::new_chat_completions(map)
        } else {
            SseStreamRehydrator::new(map)
        };

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
                rehydrator,
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
pub(super) struct StreamAuditContext {
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
