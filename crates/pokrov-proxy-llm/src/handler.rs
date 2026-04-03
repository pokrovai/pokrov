use std::{sync::Arc, time::Instant};

use futures_util::StreamExt;
use pokrov_core::{
    types::{EvaluateRequest, EvaluationMode, PathClass, PolicyAction},
    SanitizationEngine,
};
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;
use serde_json::Value;

use crate::{
    audit::LLMAuditEvent,
    errors::LLMProxyError,
    normalize::{normalize_request, resolve_profile_id},
    routing::ProviderRouteTable,
    stream::sanitize_sse_stream,
    types::{
        LLMProxyBody, LLMProxyResponse, LLMResponseMetadata, RouteResolution, UpstreamJsonResponse,
        UpstreamStreamResponse,
    },
    upstream::UpstreamClient,
};

#[derive(Clone)]
pub struct LLMProxyHandler {
    evaluator: Option<Arc<SanitizationEngine>>,
    metrics: SharedRuntimeMetricsHooks,
    routes: Arc<ProviderRouteTable>,
    upstream: UpstreamClient,
}

impl LLMProxyHandler {
    pub fn new(
        evaluator: Option<Arc<SanitizationEngine>>,
        metrics: SharedRuntimeMetricsHooks,
        routes: ProviderRouteTable,
    ) -> Result<Self, LLMProxyError> {
        Ok(Self {
            evaluator,
            metrics,
            routes: Arc::new(routes),
            upstream: UpstreamClient::new()?,
        })
    }

    pub fn routes_loaded(&self) -> bool {
        self.routes.routes_loaded()
    }

    pub fn default_profile_id(&self) -> &str {
        self.routes.default_profile_id()
    }

    pub async fn handle_chat_completion(
        &self,
        request_id: String,
        payload: Value,
        api_key_profile: &str,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let started = Instant::now();
        let envelope = normalize_request(&request_id, payload)?;
        let profile_id = resolve_profile_id(
            envelope.profile_hint.as_deref(),
            api_key_profile,
            self.default_profile_id(),
        );

        let mut final_action = PolicyAction::Allow;
        let mut total_hits = 0u32;
        let mut sanitized_input = false;
        let mut sanitized_payload = envelope.original_payload.clone();

        if let Some(evaluator) = self.evaluator.as_ref() {
            let input_eval = evaluator
                .evaluate(EvaluateRequest {
                    request_id: request_id.clone(),
                    profile_id: profile_id.clone(),
                    mode: EvaluationMode::Enforce,
                    payload: envelope.original_payload.clone(),
                    path_class: PathClass::Llm,
                })
                .map_err(|error| {
                    LLMProxyError::invalid_request(
                        request_id.clone(),
                        format!("failed to evaluate input policy: {error}"),
                    )
                })?;

            final_action = max_action(final_action, input_eval.decision.final_action);
            total_hits = total_hits.saturating_add(input_eval.decision.rule_hits_total);
            sanitized_input = input_eval.transform.transformed_fields_count > 0;

            self.metrics.on_rule_hits(input_eval.decision.rule_hits_total);
            self.metrics
                .on_payload_transformed(input_eval.transform.transformed_fields_count);
            if input_eval.transform.blocked {
                self.metrics.on_evaluation_blocked();
            }

            if input_eval.transform.blocked {
                let error = LLMProxyError::policy_blocked(
                    request_id.clone(),
                    "request blocked by active profile policy",
                );
                self.emit_terminal_event(TerminalEvent {
                    request_id: &request_id,
                    profile_id: &profile_id,
                    provider_id: None,
                    model: &envelope.model,
                    stream: envelope.stream,
                    final_action,
                    rule_hits_total: total_hits,
                    blocked: true,
                    upstream_status: None,
                    duration_ms: started.elapsed().as_millis() as u64,
                });
                return Err(error);
            }

            if let Some(sanitized) = input_eval.transform.sanitized_payload {
                sanitized_payload = sanitized;
            }
        }

        let route = self.routes.resolve(&request_id, &envelope.model)?;

        if envelope.stream {
            return self
                .handle_stream_response(
                    started,
                    request_id,
                    profile_id,
                    envelope.model,
                    route,
                    sanitized_payload,
                    final_action,
                    total_hits,
                    sanitized_input,
                )
                .await;
        }

        self.handle_json_response(
            started,
            request_id,
            profile_id,
            envelope.model,
            route,
            sanitized_payload,
            final_action,
            total_hits,
            sanitized_input,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_json_response(
        &self,
        started: Instant,
        request_id: String,
        profile_id: String,
        model: String,
        route: RouteResolution,
        sanitized_payload: Value,
        mut final_action: PolicyAction,
        mut total_hits: u32,
        sanitized_input: bool,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let upstream = self
            .upstream
            .execute_json(&request_id, &route, &sanitized_payload)
            .await;

        let UpstreamJsonResponse { status, mut body } = match upstream {
            Ok(response) => response,
            Err(error) => {
                self.emit_error_event(
                    started,
                    &request_id,
                    &profile_id,
                    Some(route.provider_id.clone()),
                    &model,
                    false,
                    final_action,
                    total_hits,
                    error.upstream_status(),
                    &error,
                );
                return Err(error);
            }
        };

        let mut sanitized_output = false;
        if route.output_sanitization {
            if let Some(evaluator) = self.evaluator.as_ref() {
                let output_eval = evaluator
                    .evaluate(EvaluateRequest {
                        request_id: request_id.clone(),
                        profile_id: profile_id.clone(),
                        mode: EvaluationMode::Enforce,
                        payload: body.clone(),
                        path_class: PathClass::Llm,
                    })
                    .map_err(|error| {
                        LLMProxyError::invalid_request(
                            request_id.clone(),
                            format!("failed to evaluate output policy: {error}"),
                        )
                    })?;

                final_action = max_action(final_action, output_eval.decision.final_action);
                total_hits = total_hits.saturating_add(output_eval.decision.rule_hits_total);
                sanitized_output = output_eval.transform.transformed_fields_count > 0;

                if output_eval.transform.blocked {
                    let error = LLMProxyError::policy_blocked(
                        request_id.clone(),
                        "response blocked by active profile policy",
                    );
                    self.emit_error_event(
                        started,
                        &request_id,
                        &profile_id,
                        Some(route.provider_id.clone()),
                        &model,
                        false,
                        final_action,
                        total_hits,
                        Some(status.as_u16()),
                        &error,
                    );
                    return Err(error);
                }

                if let Some(sanitized) = output_eval.transform.sanitized_payload {
                    body = sanitized;
                }
            }
        }

        attach_pokrov_metadata(
            &request_id,
            &profile_id,
            &route.provider_id,
            final_action,
            total_hits,
            sanitized_input,
            sanitized_output,
            &mut body,
        )?;

        self.emit_terminal_event(TerminalEvent {
            request_id: &request_id,
            profile_id: &profile_id,
            provider_id: Some(route.provider_id.clone()),
            model: &model,
            stream: false,
            final_action,
            rule_hits_total: total_hits,
            blocked: false,
            upstream_status: Some(status.as_u16()),
            duration_ms: started.elapsed().as_millis() as u64,
        });

        Ok(LLMProxyResponse {
            request_id,
            status,
            body: LLMProxyBody::Json(body),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_stream_response(
        &self,
        started: Instant,
        request_id: String,
        profile_id: String,
        model: String,
        route: RouteResolution,
        sanitized_payload: Value,
        mut final_action: PolicyAction,
        mut total_hits: u32,
        _sanitized_input: bool,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let upstream = self
            .upstream
            .execute_stream(&request_id, &route, &sanitized_payload)
            .await;

        let UpstreamStreamResponse {
            status,
            body: upstream_body,
        } = match upstream {
            Ok(response) => response,
            Err(error) => {
                self.emit_error_event(
                    started,
                    &request_id,
                    &profile_id,
                    Some(route.provider_id.clone()),
                    &model,
                    true,
                    final_action,
                    total_hits,
                    error.upstream_status(),
                    &error,
                );
                return Err(error);
            }
        };

        if route.output_sanitization {
            if let Some(evaluator) = self.evaluator.as_ref() {
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
                            started,
                            &request_id,
                            &profile_id,
                            Some(route.provider_id.clone()),
                            &model,
                            true,
                            final_action,
                            total_hits,
                            Some(status.as_u16()),
                            &cause,
                        );
                        return Err(cause);
                    }
                };

                let evaluator = Arc::clone(evaluator);
                let request_id_for_task = request_id.clone();
                let profile_id_for_task = profile_id.clone();
                let sanitized = match tokio::task::spawn_blocking(move || {
                    sanitize_sse_stream(
                        &request_id_for_task,
                        &profile_id_for_task,
                        &body,
                        evaluator.as_ref(),
                    )
                })
                .await
                {
                    Ok(Ok(sanitized)) => sanitized,
                    Ok(Err(error)) => {
                        self.emit_error_event(
                            started,
                            &request_id,
                            &profile_id,
                            Some(route.provider_id.clone()),
                            &model,
                            true,
                            final_action,
                            total_hits,
                            Some(status.as_u16()),
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
                            started,
                            &request_id,
                            &profile_id,
                            Some(route.provider_id.clone()),
                            &model,
                            true,
                            final_action,
                            total_hits,
                            Some(status.as_u16()),
                            &error,
                        );
                        return Err(error);
                    }
                };
                total_hits = total_hits.saturating_add(sanitized.rule_hits_total);
                final_action = max_action(final_action, sanitized.final_action);

                self.emit_terminal_event(TerminalEvent {
                    request_id: &request_id,
                    profile_id: &profile_id,
                    provider_id: Some(route.provider_id.clone()),
                    model: &model,
                    stream: true,
                    final_action,
                    rule_hits_total: total_hits,
                    blocked: false,
                    upstream_status: Some(status.as_u16()),
                    duration_ms: started.elapsed().as_millis() as u64,
                });

                return Ok(LLMProxyResponse {
                    request_id,
                    status,
                    body: LLMProxyBody::Sse(sanitized.body),
                });
            }
        }

        self.emit_terminal_event(TerminalEvent {
            request_id: &request_id,
            profile_id: &profile_id,
            provider_id: Some(route.provider_id.clone()),
            model: &model,
            stream: true,
            final_action,
            rule_hits_total: total_hits,
            blocked: false,
            upstream_status: Some(status.as_u16()),
            duration_ms: started.elapsed().as_millis() as u64,
        });

        Ok(LLMProxyResponse {
            request_id,
            status,
            body: LLMProxyBody::SseStream(Box::pin(upstream_body.bytes_stream())),
        })
    }

    fn emit_error_event(
        &self,
        started: Instant,
        request_id: &str,
        profile_id: &str,
        provider_id: Option<String>,
        model: &str,
        stream: bool,
        final_action: PolicyAction,
        total_hits: u32,
        upstream_status: Option<u16>,
        error: &LLMProxyError,
    ) {
        self.emit_terminal_event(TerminalEvent {
            request_id,
            profile_id,
            provider_id,
            model,
            stream,
            final_action,
            rule_hits_total: total_hits,
            blocked: matches!(error, LLMProxyError::PolicyBlocked { .. }),
            upstream_status: upstream_status.or_else(|| error.upstream_status()),
            duration_ms: started.elapsed().as_millis() as u64,
        });
    }

    fn emit_terminal_event(&self, event: TerminalEvent<'_>) {
        let audit = LLMAuditEvent {
            request_id: event.request_id.to_string(),
            profile_id: event.profile_id.to_string(),
            provider_id: event.provider_id,
            model: event.model.to_string(),
            stream: event.stream,
            final_action: event.final_action,
            rule_hits_total: event.rule_hits_total,
            blocked: event.blocked,
            upstream_status: event.upstream_status,
            duration_ms: event.duration_ms,
        };
        audit.emit();

        self.metrics.on_llm_final_action(event.final_action);
        if event.blocked {
            self.metrics.on_llm_blocked_request();
        }
        if let Some(status) = event.upstream_status {
            self.metrics.on_llm_upstream_status(status);
        }
        self.metrics.on_llm_request_duration_ms(event.duration_ms);
    }
}

struct TerminalEvent<'a> {
    request_id: &'a str,
    profile_id: &'a str,
    provider_id: Option<String>,
    model: &'a str,
    stream: bool,
    final_action: PolicyAction,
    rule_hits_total: u32,
    blocked: bool,
    upstream_status: Option<u16>,
    duration_ms: u64,
}

fn attach_pokrov_metadata(
    request_id: &str,
    profile_id: &str,
    provider_id: &str,
    final_action: PolicyAction,
    total_hits: u32,
    sanitized_input: bool,
    sanitized_output: bool,
    payload: &mut Value,
) -> Result<(), LLMProxyError> {
    let object = payload.as_object_mut().ok_or_else(|| {
        LLMProxyError::upstream_error(
            request_id,
            Some(provider_id.to_string()),
            "upstream JSON response must be an object",
        )
    })?;

    object.insert(
        "request_id".to_string(),
        Value::String(request_id.to_string()),
    );
    object.insert(
        "pokrov".to_string(),
        serde_json::to_value(LLMResponseMetadata {
            profile: profile_id.to_string(),
            sanitized_input,
            sanitized_output,
            action: final_action,
            rule_hits: total_hits,
            provider: Some(provider_id.to_string()),
        })
        .map_err(|error| {
            LLMProxyError::upstream_error(
                request_id,
                Some(provider_id.to_string()),
                format!("failed to serialize response metadata: {error}"),
            )
        })?,
    );
    Ok(())
}

fn max_action(left: PolicyAction, right: PolicyAction) -> PolicyAction {
    if right.strictness_rank() > left.strictness_rank() {
        right
    } else {
        left
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
