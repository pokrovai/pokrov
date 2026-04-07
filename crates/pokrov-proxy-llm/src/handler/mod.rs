use std::{sync::Arc, time::Instant};

use pokrov_config::{model::ResponseMetadataMode, normalize_model_key, UpstreamAuthMode};
use pokrov_core::{
    types::{EvaluateRequest, EvaluationMode, PathClass, PolicyAction},
    SanitizationEngine,
};
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;
use serde_json::Value;

use crate::{
    audit::LLMAuditEvent,
    errors::LLMProxyError,
    normalize::{
        estimate_token_units, normalize_request, normalize_responses_payload, resolve_profile_id,
    },
    routing::{select_upstream_credential, ProviderRouteTable},
    types::{
        LLMProxyBody, LLMProxyResponse, RouteResolution, UpstreamCredentialOrigin,
        UpstreamJsonResponse, RESPONSES_ENDPOINT,
    },
    upstream::UpstreamClient,
};
#[cfg(feature = "llm_payload_trace")]
use crate::trace::LlmPayloadTraceSink;
use support::{
    attach_pokrov_metadata, attach_request_id, max_action, mode_as_str, ResponseMetadataContext,
    TerminalEvent,
};

mod streaming;
mod support;

struct ErrorEventContext<'a> {
    started: Instant,
    endpoint: &'static str,
    request_id: &'a str,
    profile_id: &'a str,
    provider_id: Option<String>,
    model: &'a str,
    stream: bool,
    final_action: PolicyAction,
    total_hits: u32,
    upstream_status: Option<u16>,
}

#[derive(Clone)]
pub struct LLMProxyHandler {
    evaluator: Option<Arc<SanitizationEngine>>,
    metrics: SharedRuntimeMetricsHooks,
    routes: Arc<ProviderRouteTable>,
    upstream: UpstreamClient,
    response_metadata_mode: ResponseMetadataMode,
}

impl LLMProxyHandler {
    pub fn new(
        evaluator: Option<Arc<SanitizationEngine>>,
        metrics: SharedRuntimeMetricsHooks,
        routes: ProviderRouteTable,
        response_metadata_mode: ResponseMetadataMode,
        #[cfg(feature = "llm_payload_trace")] payload_trace_sink: Option<LlmPayloadTraceSink>,
    ) -> Result<Self, LLMProxyError> {
        let upstream = UpstreamClient::new()?;
        #[cfg(feature = "llm_payload_trace")]
        let upstream = upstream.with_payload_trace_sink(payload_trace_sink);

        Ok(Self {
            evaluator,
            metrics,
            routes: Arc::new(routes),
            upstream,
            response_metadata_mode,
        })
    }

    pub fn routes_loaded(&self) -> bool {
        self.routes.routes_loaded()
    }

    pub fn default_profile_id(&self) -> &str {
        self.routes.default_profile_id()
    }

    pub fn model_catalog(&self) -> &[crate::routing::ModelCatalogEntry] {
        self.routes.model_catalog()
    }

    pub async fn handle_chat_completion(
        &self,
        request_id: String,
        payload: Value,
        api_key_profile: &str,
        auth_mode: UpstreamAuthMode,
        upstream_credential: Option<&str>,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        self.handle_chat_completion_for_endpoint(
            "/v1/chat/completions",
            request_id,
            payload,
            api_key_profile,
            auth_mode,
            upstream_credential,
        )
        .await
    }

    async fn handle_chat_completion_for_endpoint(
        &self,
        endpoint: &'static str,
        request_id: String,
        payload: Value,
        api_key_profile: &str,
        auth_mode: UpstreamAuthMode,
        upstream_credential: Option<&str>,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let started = Instant::now();
        let envelope = normalize_request(&request_id, payload)?;
        let estimated_token_units = estimate_token_units(&envelope.original_payload);
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
                    effective_language: "en".to_string(),
                    entity_scope_filters: Vec::new(),
                    recognizer_family_filters: Vec::new(),
                    allowlist_additions: Vec::new(),
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
            self.metrics.on_payload_transformed(input_eval.transform.transformed_fields_count);
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
                    endpoint,
                    profile_id: &profile_id,
                    provider_id: None,
                    model: &envelope.model,
                    stream: envelope.stream,
                    final_action,
                    rule_hits_total: total_hits,
                    blocked: true,
                    upstream_status: None,
                    duration_ms: started.elapsed().as_millis() as u64,
                    estimated_token_units,
                    auth_mode: mode_as_str(auth_mode),
                    credential_origin: UpstreamCredentialOrigin::Config,
                });
                return Err(error);
            }

            if let Some(sanitized) = input_eval.transform.sanitized_payload {
                sanitized_payload = sanitized;
            }
        }

        let normalized_model_key = normalize_model_key(&envelope.model);
        let route = match self.routes.resolve(&request_id, &envelope.model) {
            Ok(route) => {
                self.metrics.on_model_resolution();
                route
            }
            Err(error) => {
                self.metrics.on_model_resolution_failed();
                tracing::info!(
                    component = "llm_proxy",
                    action = "model_resolution",
                    request_id = %request_id,
                    route = %endpoint,
                    input_model_key = %envelope.model,
                    normalized_model_key = %normalized_model_key,
                    resolution_status = %error.code().as_str(),
                );
                return Err(error);
            }
        };
        tracing::info!(
            component = "llm_proxy",
            action = "model_resolution",
            request_id = %request_id,
            route = %endpoint,
            input_model_key = %envelope.model,
            normalized_model_key = %normalized_model_key,
            canonical_model = %route.canonical_model,
            resolved_model = %route.canonical_model,
            provider_id = %route.provider_id,
            resolved_via_alias = route.resolved_via_alias,
            resolution_status = "resolved",
        );
        override_payload_model(&mut sanitized_payload, &route.canonical_model);
        let selected_credential = select_upstream_credential(auth_mode, &route, upstream_credential);
        if selected_credential.is_none() && matches!(auth_mode, UpstreamAuthMode::Passthrough) {
            return Err(LLMProxyError::invalid_request(
                request_id.clone(),
                "upstream credential is required in passthrough mode",
            ));
        }
        let credential_origin = selected_credential
            .as_ref()
            .map(|credential| credential.origin)
            .unwrap_or(UpstreamCredentialOrigin::Config);
        let upstream_credential = selected_credential.map(|credential| credential.token);

        if envelope.stream {
            return self
                .handle_stream_response(
                    started,
                    endpoint,
                    request_id,
                    profile_id,
                    envelope.model,
                    route,
                    sanitized_payload,
                    final_action,
                    total_hits,
                    sanitized_input,
                    estimated_token_units,
                    auth_mode,
                    credential_origin,
                    upstream_credential.clone(),
                )
                .await;
        }

        self.handle_json_response(
            started,
            endpoint,
            request_id,
            profile_id,
            envelope.model,
            route,
            sanitized_payload,
            final_action,
            total_hits,
            sanitized_input,
            estimated_token_units,
            auth_mode,
            credential_origin,
            upstream_credential,
        )
        .await
    }

    pub async fn handle_responses(
        &self,
        request_id: String,
        payload: Value,
        api_key_profile: &str,
        auth_mode: UpstreamAuthMode,
        upstream_credential: Option<&str>,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let normalized = normalize_responses_payload(&request_id, payload)?;
        self.handle_chat_completion_for_endpoint(
            RESPONSES_ENDPOINT,
            request_id,
            normalized,
            api_key_profile,
            auth_mode,
            upstream_credential,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_json_response(
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
        sanitized_input: bool,
        estimated_token_units: u32,
        auth_mode: UpstreamAuthMode,
        credential_origin: UpstreamCredentialOrigin,
        upstream_credential: Option<String>,
    ) -> Result<LLMProxyResponse, LLMProxyError> {
        let upstream = self
            .upstream
            .execute_json(
                &request_id,
                &route,
                &sanitized_payload,
                upstream_credential.as_deref(),
            )
            .await;

        let UpstreamJsonResponse { status, mut body } = match upstream {
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
                        stream: false,
                        final_action,
                        total_hits,
                        upstream_status: error.upstream_status(),
                    },
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
                        effective_language: "en".to_string(),
                        entity_scope_filters: Vec::new(),
                        recognizer_family_filters: Vec::new(),
                        allowlist_additions: Vec::new(),
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
                        ErrorEventContext {
                            started,
                            endpoint,
                            request_id: &request_id,
                            profile_id: &profile_id,
                            provider_id: Some(route.provider_id.clone()),
                            model: &model,
                            stream: false,
                            final_action,
                            total_hits,
                            upstream_status: Some(status.as_u16()),
                        },
                        &error,
                    );
                    return Err(error);
                }

                if let Some(sanitized) = output_eval.transform.sanitized_payload {
                    body = sanitized;
                }
            }
        }

        attach_request_id(&request_id, &route.provider_id, &mut body)?;
        if self.response_metadata_mode == ResponseMetadataMode::Enabled {
            attach_pokrov_metadata(
                ResponseMetadataContext {
                    request_id: &request_id,
                    profile_id: &profile_id,
                    provider_id: &route.provider_id,
                    final_action,
                    total_hits,
                    sanitized_input,
                    sanitized_output,
                    estimated_token_units,
                },
                &mut body,
            )?;
        }

        self.emit_terminal_event(TerminalEvent {
            request_id: &request_id,
            endpoint,
            profile_id: &profile_id,
            provider_id: Some(route.provider_id.clone()),
            model: &model,
            stream: false,
            final_action,
            rule_hits_total: total_hits,
            blocked: false,
            upstream_status: Some(status.as_u16()),
            duration_ms: started.elapsed().as_millis() as u64,
            estimated_token_units,
            auth_mode: mode_as_str(auth_mode),
            credential_origin,
        });

        Ok(LLMProxyResponse { request_id, status, body: LLMProxyBody::Json(body) })
    }

    fn emit_error_event(&self, context: ErrorEventContext<'_>, error: &LLMProxyError) {
        self.emit_terminal_event(TerminalEvent {
            request_id: context.request_id,
            endpoint: context.endpoint,
            profile_id: context.profile_id,
            provider_id: context.provider_id,
            model: context.model,
            stream: context.stream,
            final_action: context.final_action,
            rule_hits_total: context.total_hits,
            blocked: matches!(error, LLMProxyError::PolicyBlocked { .. }),
            upstream_status: context.upstream_status.or_else(|| error.upstream_status()),
            duration_ms: context.started.elapsed().as_millis() as u64,
            estimated_token_units: 0,
            auth_mode: "unknown",
            credential_origin: UpstreamCredentialOrigin::Config,
        });
    }

    fn emit_terminal_event(&self, event: TerminalEvent<'_>) {
        let audit = LLMAuditEvent {
            request_id: event.request_id.to_string(),
            endpoint: event.endpoint.to_string(),
            profile_id: event.profile_id.to_string(),
            provider_id: event.provider_id,
            model: event.model.to_string(),
            stream: event.stream,
            final_action: event.final_action,
            rule_hits_total: event.rule_hits_total,
            blocked: event.blocked,
            upstream_status: event.upstream_status,
            duration_ms: event.duration_ms,
            estimated_token_units: event.estimated_token_units,
            auth_mode: event.auth_mode.to_string(),
            credential_origin: event.credential_origin,
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

fn override_payload_model(payload: &mut Value, canonical_model: &str) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    object.insert("model".to_string(), Value::String(canonical_model.to_string()));
}
