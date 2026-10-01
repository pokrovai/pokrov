use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use pokrov_core::prompt_injection::{
    PromptInjectionClassification, PromptInjectionDecision, PromptInjectionOutcome,
};
use pokrov_core::types::PolicyAction;
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, Opts, Registry, TextEncoder,
};

use crate::hooks::{LifecycleEvent, RuntimeMetricsHooks};

#[derive(Debug)]
pub struct RuntimeMetricsRegistry {
    starting_total: AtomicU64,
    ready_total: AtomicU64,
    draining_total: AtomicU64,
    stopped_total: AtomicU64,
    requests_started_total: AtomicU64,
    requests_finished_total: AtomicU64,
    rule_hits_total: AtomicU64,
    transformed_payloads_total: AtomicU64,
    blocked_evaluations_total: AtomicU64,
    llm_action_allow_total: AtomicU64,
    llm_action_mask_total: AtomicU64,
    llm_action_replace_total: AtomicU64,
    llm_action_redact_total: AtomicU64,
    llm_action_block_total: AtomicU64,
    llm_blocked_requests_total: AtomicU64,
    llm_upstream_2xx_total: AtomicU64,
    llm_upstream_4xx_total: AtomicU64,
    llm_upstream_5xx_total: AtomicU64,
    llm_request_duration_ms_total: AtomicU64,
    mcp_tool_calls_total: AtomicU64,
    mcp_tool_calls_blocked_total: AtomicU64,
    mcp_tool_call_duration_ms_total: AtomicU64,
    prometheus_registry: Registry,
    requests_total: IntCounterVec,
    blocked_total: IntCounterVec,
    rate_limit_events_total: IntCounterVec,
    auth_decisions_total: IntCounterVec,
    upstream_errors_total: IntCounterVec,
    request_duration_seconds: HistogramVec,
    model_resolution_total: IntCounter,
    model_resolution_failed_total: IntCounter,
    models_catalog_requests_total: IntCounter,
    tokenized_spans_total: IntCounter,
    rehydrated_tokens_total: IntCounter,
    unrestored_tokens_total: IntCounter,
    prompt_injection_evaluations_total: IntCounterVec,
    prompt_injection_detected_total: IntCounterVec,
    prompt_injection_blocked_total: IntCounterVec,
    prompt_injection_detector_errors_total: IntCounterVec,
    prompt_injection_detector_duration_seconds: HistogramVec,
    prompt_injection_chunks_total: IntCounterVec,
    force_render_failure: AtomicBool,
}

impl RuntimeMetricsRegistry {
    pub fn new() -> Result<Self, prometheus::Error> {
        let prometheus_registry = Registry::new();
        let requests_total = IntCounterVec::new(
            Opts::new("pokrov_requests_total", "Total requests by route/path/status/decision"),
            &["route", "path_class", "status", "decision"],
        )?;
        let blocked_total = IntCounterVec::new(
            Opts::new("pokrov_blocked_total", "Total blocked requests"),
            &["route", "block_reason", "policy_profile"],
        )?;
        let rate_limit_events_total = IntCounterVec::new(
            Opts::new("pokrov_rate_limit_events_total", "Total rate limit events"),
            &["route", "limit_kind", "decision", "policy_profile"],
        )?;
        let auth_decisions_total = IntCounterVec::new(
            Opts::new("pokrov_auth_decisions_total", "Total auth stage decisions by mode"),
            &["auth_mode", "stage", "decision"],
        )?;
        let upstream_errors_total = IntCounterVec::new(
            Opts::new("pokrov_upstream_errors_total", "Total upstream errors"),
            &["route", "provider", "error_class"],
        )?;
        let request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "pokrov_request_duration_seconds",
                "Request duration histogram by route/path/decision",
            )
            .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5]),
            &["route", "path_class", "decision"],
        )?;
        let model_resolution_total = IntCounter::new(
            "pokrov_model_resolution_total",
            "Total successful model resolution attempts",
        )?;
        let model_resolution_failed_total = IntCounter::new(
            "pokrov_model_resolution_failed_total",
            "Total failed model resolution attempts",
        )?;
        let models_catalog_requests_total = IntCounter::new(
            "pokrov_models_catalog_requests_total",
            "Total requests to /v1/models catalog endpoint",
        )?;
        let tokenized_spans_total = IntCounter::new(
            "pokrov_tokenized_spans_total",
            "Total sensitive spans replaced with reversible [PKV_TOKEN] pseudonyms",
        )?;
        let rehydrated_tokens_total = IntCounter::new(
            "pokrov_rehydrated_tokens_total",
            "Total pseudonym tokens restored to original fragments on trusted paths",
        )?;
        let unrestored_tokens_total = IntCounter::new(
            "pokrov_unrestored_tokens_total",
            "Total __PKV_ markers observed in responses without a matching token",
        )?;
        // Prompt-injection metrics per spec §18; label sets are constrained to
        // bounded metadata values (source/provider/model/decision/fail_mode).
        let prompt_injection_evaluations_total = IntCounterVec::new(
            Opts::new(
                "pokrov_prompt_injection_evaluations_total",
                "Total prompt-injection evaluations by source/decision",
            ),
            &["source", "decision"],
        )?;
        let prompt_injection_detected_total = IntCounterVec::new(
            Opts::new(
                "pokrov_prompt_injection_detected_total",
                "Total contents classified as injection by source/provider/model",
            ),
            &["source", "provider", "model"],
        )?;
        let prompt_injection_blocked_total = IntCounterVec::new(
            Opts::new(
                "pokrov_prompt_injection_blocked_total",
                "Total contents withheld by the prompt-injection stage",
            ),
            &["source"],
        )?;
        let prompt_injection_detector_errors_total = IntCounterVec::new(
            Opts::new(
                "pokrov_prompt_injection_detector_errors_total",
                "Total degraded prompt-injection evaluations by source/fail_mode",
            ),
            &["source", "fail_mode"],
        )?;
        let prompt_injection_detector_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "pokrov_prompt_injection_detector_duration_seconds",
                "Prompt-injection detector call duration by provider/model",
            )
            // Upper buckets cover configured detector timeouts, which can be
            // tens of seconds; without them long-running inferences would
            // collapse into +Inf.
            .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]),
            &["provider", "model"],
        )?;
        let prompt_injection_chunks_total = IntCounterVec::new(
            Opts::new(
                "pokrov_prompt_injection_chunks_total",
                "Total detector context windows evaluated by source",
            ),
            &["source"],
        )?;

        prometheus_registry.register(Box::new(requests_total.clone()))?;
        prometheus_registry.register(Box::new(blocked_total.clone()))?;
        prometheus_registry.register(Box::new(rate_limit_events_total.clone()))?;
        prometheus_registry.register(Box::new(auth_decisions_total.clone()))?;
        prometheus_registry.register(Box::new(upstream_errors_total.clone()))?;
        prometheus_registry.register(Box::new(request_duration_seconds.clone()))?;
        prometheus_registry.register(Box::new(model_resolution_total.clone()))?;
        prometheus_registry.register(Box::new(model_resolution_failed_total.clone()))?;
        prometheus_registry.register(Box::new(models_catalog_requests_total.clone()))?;
        prometheus_registry.register(Box::new(tokenized_spans_total.clone()))?;
        prometheus_registry.register(Box::new(rehydrated_tokens_total.clone()))?;
        prometheus_registry.register(Box::new(unrestored_tokens_total.clone()))?;
        prometheus_registry.register(Box::new(prompt_injection_evaluations_total.clone()))?;
        prometheus_registry.register(Box::new(prompt_injection_detected_total.clone()))?;
        prometheus_registry.register(Box::new(prompt_injection_blocked_total.clone()))?;
        prometheus_registry.register(Box::new(prompt_injection_detector_errors_total.clone()))?;
        prometheus_registry
            .register(Box::new(prompt_injection_detector_duration_seconds.clone()))?;
        prometheus_registry.register(Box::new(prompt_injection_chunks_total.clone()))?;

        requests_total.with_label_values(&["other", "runtime", "2xx", "allowed"]);
        blocked_total.with_label_values(&["other", "policy", "strict"]);
        rate_limit_events_total.with_label_values(&["other", "requests", "blocked", "strict"]);
        auth_decisions_total.with_label_values(&["static", "gateway_auth", "pass"]);
        upstream_errors_total.with_label_values(&["other", "unknown", "transport"]);
        request_duration_seconds.with_label_values(&["other", "runtime", "allowed"]);
        prompt_injection_evaluations_total.with_label_values(&["other", "allow"]);
        prompt_injection_detected_total.with_label_values(&["other", "none", "none"]);
        prompt_injection_blocked_total.with_label_values(&["other"]);
        prompt_injection_detector_errors_total.with_label_values(&["other", "fail_open"]);
        prompt_injection_detector_duration_seconds.with_label_values(&["none", "none"]);
        prompt_injection_chunks_total.with_label_values(&["other"]);

        Ok(Self {
            starting_total: AtomicU64::new(0),
            ready_total: AtomicU64::new(0),
            draining_total: AtomicU64::new(0),
            stopped_total: AtomicU64::new(0),
            requests_started_total: AtomicU64::new(0),
            requests_finished_total: AtomicU64::new(0),
            rule_hits_total: AtomicU64::new(0),
            transformed_payloads_total: AtomicU64::new(0),
            blocked_evaluations_total: AtomicU64::new(0),
            tokenized_spans_total,
            rehydrated_tokens_total,
            unrestored_tokens_total,
            llm_action_allow_total: AtomicU64::new(0),
            llm_action_mask_total: AtomicU64::new(0),
            llm_action_replace_total: AtomicU64::new(0),
            llm_action_redact_total: AtomicU64::new(0),
            llm_action_block_total: AtomicU64::new(0),
            llm_blocked_requests_total: AtomicU64::new(0),
            llm_upstream_2xx_total: AtomicU64::new(0),
            llm_upstream_4xx_total: AtomicU64::new(0),
            llm_upstream_5xx_total: AtomicU64::new(0),
            llm_request_duration_ms_total: AtomicU64::new(0),
            mcp_tool_calls_total: AtomicU64::new(0),
            mcp_tool_calls_blocked_total: AtomicU64::new(0),
            mcp_tool_call_duration_ms_total: AtomicU64::new(0),
            prometheus_registry,
            requests_total,
            blocked_total,
            rate_limit_events_total,
            auth_decisions_total,
            upstream_errors_total,
            request_duration_seconds,
            model_resolution_total,
            model_resolution_failed_total,
            models_catalog_requests_total,
            prompt_injection_evaluations_total,
            prompt_injection_detected_total,
            prompt_injection_blocked_total,
            prompt_injection_detector_errors_total,
            prompt_injection_detector_duration_seconds,
            prompt_injection_chunks_total,
            force_render_failure: AtomicBool::new(false),
        })
    }

    pub fn render_prometheus(&self) -> Result<String, String> {
        if self.force_render_failure.load(Ordering::Relaxed) {
            return Err("forced metrics rendering failure".to_string());
        }
        let mut buffer = Vec::new();
        let metric_families = self.prometheus_registry.gather();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).map_err(|error| format!("{error}"))?;
        String::from_utf8(buffer).map_err(|error| error.to_string())
    }

    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            starting_total: self.starting_total.load(Ordering::Relaxed),
            ready_total: self.ready_total.load(Ordering::Relaxed),
            draining_total: self.draining_total.load(Ordering::Relaxed),
            stopped_total: self.stopped_total.load(Ordering::Relaxed),
            requests_started_total: self.requests_started_total.load(Ordering::Relaxed),
            requests_finished_total: self.requests_finished_total.load(Ordering::Relaxed),
            rule_hits_total: self.rule_hits_total.load(Ordering::Relaxed),
            transformed_payloads_total: self.transformed_payloads_total.load(Ordering::Relaxed),
            blocked_evaluations_total: self.blocked_evaluations_total.load(Ordering::Relaxed),
            tokenized_spans_total: self.tokenized_spans_total.get() as u64,
            rehydrated_tokens_total: self.rehydrated_tokens_total.get() as u64,
            unrestored_tokens_total: self.unrestored_tokens_total.get() as u64,
            llm_action_allow_total: self.llm_action_allow_total.load(Ordering::Relaxed),
            llm_action_mask_total: self.llm_action_mask_total.load(Ordering::Relaxed),
            llm_action_replace_total: self.llm_action_replace_total.load(Ordering::Relaxed),
            llm_action_redact_total: self.llm_action_redact_total.load(Ordering::Relaxed),
            llm_action_block_total: self.llm_action_block_total.load(Ordering::Relaxed),
            llm_blocked_requests_total: self.llm_blocked_requests_total.load(Ordering::Relaxed),
            llm_upstream_2xx_total: self.llm_upstream_2xx_total.load(Ordering::Relaxed),
            llm_upstream_4xx_total: self.llm_upstream_4xx_total.load(Ordering::Relaxed),
            llm_upstream_5xx_total: self.llm_upstream_5xx_total.load(Ordering::Relaxed),
            llm_request_duration_ms_total: self
                .llm_request_duration_ms_total
                .load(Ordering::Relaxed),
            mcp_tool_calls_total: self.mcp_tool_calls_total.load(Ordering::Relaxed),
            mcp_tool_calls_blocked_total: self.mcp_tool_calls_blocked_total.load(Ordering::Relaxed),
            mcp_tool_call_duration_ms_total: self
                .mcp_tool_call_duration_ms_total
                .load(Ordering::Relaxed),
        }
    }

    pub fn set_force_render_failure(&self, forced: bool) {
        self.force_render_failure.store(forced, Ordering::Relaxed);
    }
}

impl Default for RuntimeMetricsRegistry {
    fn default() -> Self {
        Self::new().expect("prometheus metrics registry must initialize")
    }
}

impl RuntimeMetricsHooks for RuntimeMetricsRegistry {
    fn on_lifecycle_event(&self, event: LifecycleEvent) {
        match event {
            LifecycleEvent::Starting => {
                self.starting_total.fetch_add(1, Ordering::Relaxed);
            }
            LifecycleEvent::Ready => {
                self.ready_total.fetch_add(1, Ordering::Relaxed);
            }
            LifecycleEvent::Draining => {
                self.draining_total.fetch_add(1, Ordering::Relaxed);
            }
            LifecycleEvent::Stopped => {
                self.stopped_total.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn on_request_started(&self) {
        self.requests_started_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_request_finished(&self) {
        self.requests_finished_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_rule_hits(&self, hits: u32) {
        self.rule_hits_total.fetch_add(hits as u64, Ordering::Relaxed);
    }

    fn on_payload_transformed(&self, count: u32) {
        self.transformed_payloads_total.fetch_add(count as u64, Ordering::Relaxed);
    }

    fn on_evaluation_blocked(&self) {
        self.blocked_evaluations_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_tokenized_spans(&self, count: u32) {
        self.tokenized_spans_total.inc_by(count as u64);
    }

    fn on_rehydrated_tokens(&self, count: u32) {
        self.rehydrated_tokens_total.inc_by(count as u64);
    }

    fn on_unrestored_tokens(&self, count: u32) {
        self.unrestored_tokens_total.inc_by(count as u64);
    }

    fn on_llm_final_action(&self, action: PolicyAction) {
        match action {
            PolicyAction::Allow => {
                self.llm_action_allow_total.fetch_add(1, Ordering::Relaxed);
            }
            PolicyAction::Mask => {
                self.llm_action_mask_total.fetch_add(1, Ordering::Relaxed);
            }
            PolicyAction::Replace => {
                self.llm_action_replace_total.fetch_add(1, Ordering::Relaxed);
            }
            PolicyAction::Redact => {
                self.llm_action_redact_total.fetch_add(1, Ordering::Relaxed);
            }
            PolicyAction::Block => {
                self.llm_action_block_total.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn on_llm_blocked_request(&self) {
        self.llm_blocked_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_llm_upstream_status(&self, status: u16) {
        match status {
            200..=299 => {
                self.llm_upstream_2xx_total.fetch_add(1, Ordering::Relaxed);
            }
            400..=499 => {
                self.llm_upstream_4xx_total.fetch_add(1, Ordering::Relaxed);
            }
            500..=599 => {
                self.llm_upstream_5xx_total.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    fn on_llm_request_duration_ms(&self, duration_ms: u64) {
        self.llm_request_duration_ms_total.fetch_add(duration_ms, Ordering::Relaxed);
    }

    fn on_model_resolution(&self) {
        self.model_resolution_total.inc();
    }

    fn on_model_resolution_failed(&self) {
        self.model_resolution_failed_total.inc();
    }

    fn on_models_catalog_request(&self) {
        self.models_catalog_requests_total.inc();
    }

    fn on_mcp_tool_call(&self) {
        self.mcp_tool_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_mcp_tool_call_blocked(&self) {
        self.mcp_tool_calls_blocked_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_mcp_tool_call_duration_ms(&self, duration_ms: u64) {
        self.mcp_tool_call_duration_ms_total.fetch_add(duration_ms, Ordering::Relaxed);
    }

    fn on_prompt_injection_evaluation(&self, outcome: &PromptInjectionOutcome) {
        let source = constrain_pi_source(outcome.source.as_str());
        let provider = constrain_pi_provider(&outcome.provider);

        self.prompt_injection_evaluations_total
            .with_label_values(&[source, constrain_pi_decision(outcome.decision.as_str())])
            .inc();
        self.prompt_injection_chunks_total
            .with_label_values(&[source])
            .inc_by(outcome.chunks_processed as u64);
        self.prompt_injection_detector_duration_seconds
            .with_label_values(&[provider, constrain_pi_model(&outcome.model_id)])
            .observe((outcome.duration_ms as f64 / 1_000.0).max(0.0));

        if outcome.classification == Some(PromptInjectionClassification::Injection) {
            self.prompt_injection_detected_total
                .with_label_values(&[source, provider, constrain_pi_model(&outcome.model_id)])
                .inc();
        }
        if outcome.decision == PromptInjectionDecision::Block {
            self.prompt_injection_blocked_total.with_label_values(&[source]).inc();
        }
        if outcome.degraded {
            self.prompt_injection_detector_errors_total
                .with_label_values(&[source, constrain_pi_fail_mode(outcome.fail_mode.as_str())])
                .inc();
        }
    }

    fn on_request_outcome(&self, route: &str, path_class: &str, status: u16, decision: &str) {
        self.requests_total
            .with_label_values(&[
                constrain_route(route),
                constrain_path_class(path_class),
                status_class(status),
                constrain_decision(decision),
            ])
            .inc();
    }

    fn on_blocked_request(&self, route: &str, block_reason: &str, policy_profile: &str) {
        self.blocked_total
            .with_label_values(&[
                constrain_route(route),
                constrain_block_reason(block_reason),
                constrain_profile(policy_profile),
            ])
            .inc();
    }

    fn on_rate_limit_event(
        &self,
        route: &str,
        limit_kind: &str,
        decision: &str,
        policy_profile: &str,
    ) {
        self.rate_limit_events_total
            .with_label_values(&[
                constrain_route(route),
                constrain_limit_kind(limit_kind),
                constrain_rate_limit_decision(decision),
                constrain_profile(policy_profile),
            ])
            .inc();
    }

    fn on_upstream_error(&self, route: &str, provider: &str, error_class: &str) {
        self.upstream_errors_total
            .with_label_values(&[
                constrain_route(route),
                constrain_provider(provider),
                constrain_error_class(error_class),
            ])
            .inc();
    }

    fn on_auth_decision(&self, auth_mode: &str, stage: &str, decision: &str) {
        self.auth_decisions_total
            .with_label_values(&[
                constrain_auth_mode(auth_mode),
                constrain_auth_stage(stage),
                constrain_auth_decision(decision),
            ])
            .inc();
    }

    fn on_responses_auth_stage(&self, auth_mode: &str, stage: &str, decision: &str) {
        self.on_auth_decision(auth_mode, stage, decision);
    }

    fn on_responses_upstream_error(&self, provider: &str, error_class: &str) {
        self.on_upstream_error("/v1/responses", provider, error_class);
    }

    fn on_request_duration_seconds(
        &self,
        route: &str,
        path_class: &str,
        decision: &str,
        seconds: f64,
    ) {
        self.request_duration_seconds
            .with_label_values(&[
                constrain_route(route),
                constrain_path_class(path_class),
                constrain_decision(decision),
            ])
            .observe(seconds.max(0.0));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RuntimeMetricsSnapshot {
    pub starting_total: u64,
    pub ready_total: u64,
    pub draining_total: u64,
    pub stopped_total: u64,
    pub requests_started_total: u64,
    pub requests_finished_total: u64,
    pub rule_hits_total: u64,
    pub transformed_payloads_total: u64,
    pub blocked_evaluations_total: u64,
    pub tokenized_spans_total: u64,
    pub rehydrated_tokens_total: u64,
    pub unrestored_tokens_total: u64,
    pub llm_action_allow_total: u64,
    pub llm_action_mask_total: u64,
    pub llm_action_replace_total: u64,
    pub llm_action_redact_total: u64,
    pub llm_action_block_total: u64,
    pub llm_blocked_requests_total: u64,
    pub llm_upstream_2xx_total: u64,
    pub llm_upstream_4xx_total: u64,
    pub llm_upstream_5xx_total: u64,
    pub llm_request_duration_ms_total: u64,
    pub mcp_tool_calls_total: u64,
    pub mcp_tool_calls_blocked_total: u64,
    pub mcp_tool_call_duration_ms_total: u64,
}

fn constrain_route(route: &str) -> &str {
    match route {
        "/health" => "/health",
        "/ready" => "/ready",
        "/metrics" => "/metrics",
        "/v1/sanitize/evaluate" => "/v1/sanitize/evaluate",
        "/v1/chat/completions" => "/v1/chat/completions",
        "/v1/models" => "/v1/models",
        "/v1/responses" => "/v1/responses",
        "/v1/mcp/tool-call" => "/v1/mcp/tool-call",
        "/v1/mcp/tools/{toolName}/invoke" => "/v1/mcp/tools/{toolName}/invoke",
        _ => "other",
    }
}

fn constrain_path_class(path_class: &str) -> &str {
    match path_class {
        "runtime" | "sanitization" | "llm" | "mcp" => path_class,
        _ => "other",
    }
}

fn constrain_decision(decision: &str) -> &str {
    match decision {
        "allowed" | "masked" | "redacted" | "blocked" | "errored" => decision,
        _ => "errored",
    }
}

fn constrain_rate_limit_decision(decision: &str) -> &str {
    match decision {
        "blocked" | "dry_run" => decision,
        _ => "blocked",
    }
}

fn constrain_limit_kind(kind: &str) -> &str {
    match kind {
        "requests" | "token_units" => kind,
        _ => "requests",
    }
}

fn constrain_profile(profile: &str) -> &str {
    match profile {
        "minimal" | "strict" | "custom" => profile,
        _ => "custom",
    }
}

fn constrain_block_reason(reason: &str) -> &str {
    match reason {
        "policy" | "rate_limit" | "runtime_draining" | "invalid_request" => reason,
        _ => "policy",
    }
}

fn constrain_provider(provider: &str) -> &str {
    match provider {
        "openai" | "anthropic" | "mcp" | "unknown" => provider,
        _ => "other",
    }
}

fn constrain_error_class(error_class: &str) -> &str {
    match error_class {
        "upstream_4xx" | "upstream_5xx" | "transport" | "timeout" => error_class,
        _ => "transport",
    }
}

fn constrain_pi_source(source: &str) -> &str {
    match source {
        "mcp_tool_output" | "mcp_tool_description" | "mcp_resource" | "mcp_prompt" | "rag"
        | "external" => source,
        _ => "other",
    }
}

fn constrain_pi_provider(provider: &str) -> &str {
    match provider {
        "onnx" | "static" | "none" => provider,
        _ => "other",
    }
}

fn constrain_pi_decision(decision: &str) -> &str {
    match decision {
        "allow" | "block" => decision,
        _ => "allow",
    }
}

fn constrain_pi_fail_mode(fail_mode: &str) -> &str {
    match fail_mode {
        "fail_open" | "fail_closed" => fail_mode,
        _ => "fail_open",
    }
}

/// Model ids are operator-configured strings, not an enum; keep only
/// conventional model-name characters and cap the length so a malformed id
/// cannot grow the label space.
fn constrain_pi_model(model: &str) -> &str {
    let valid = !model.is_empty()
        && model.len() <= 128
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    if valid {
        model
    } else {
        "other"
    }
}

fn constrain_auth_mode(value: &str) -> &str {
    match value {
        "static" => "static",
        "passthrough" => "passthrough",
        _ => "unknown",
    }
}

fn constrain_auth_stage(value: &str) -> &str {
    match value {
        "gateway_auth" => "gateway_auth",
        "upstream_credentials" => "upstream_credentials",
        _ => "other",
    }
}

fn constrain_auth_decision(value: &str) -> &str {
    match value {
        "pass" => "pass",
        "fail" => "fail",
        _ => "other",
    }
}

fn status_class(status: u16) -> &'static str {
    match status {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use crate::hooks::RuntimeMetricsHooks;

    use super::RuntimeMetricsRegistry;

    #[test]
    fn records_mandatory_series_and_renders_prometheus_payload() {
        let registry = RuntimeMetricsRegistry::default();
        registry.on_request_outcome("/v1/chat/completions", "llm", 200, "allowed");
        registry.on_blocked_request("/v1/chat/completions", "policy", "strict");
        registry.on_rate_limit_event("/v1/chat/completions", "requests", "blocked", "strict");
        registry.on_auth_decision("passthrough", "gateway_auth", "pass");
        registry.on_upstream_error("/v1/chat/completions", "openai", "upstream_5xx");
        registry.on_request_duration_seconds("/v1/chat/completions", "llm", "allowed", 0.02);

        let rendered = registry.render_prometheus().expect("metrics should render");
        assert!(rendered.contains("pokrov_requests_total"));
        assert!(rendered.contains("pokrov_blocked_total"));
        assert!(rendered.contains("pokrov_rate_limit_events_total"));
        assert!(rendered.contains("pokrov_auth_decisions_total"));
        assert!(rendered.contains("pokrov_upstream_errors_total"));
        assert!(rendered.contains("pokrov_request_duration_seconds"));
    }

    #[test]
    fn constrains_untrusted_label_values_to_low_cardinality_buckets() {
        let registry = RuntimeMetricsRegistry::default();
        registry.on_request_outcome("/untrusted/path", "dynamic", 418, "unexpected");
        registry.on_upstream_error("/untrusted/path", "provider-123", "panic");

        let rendered = registry.render_prometheus().expect("metrics should render");
        assert!(rendered.contains("route=\"other\""));
        assert!(rendered.contains("decision=\"errored\""));
        assert!(!rendered.contains("request_id="));
        assert!(!rendered.contains("prompt="));
    }

    #[test]
    fn exposes_tokenization_counters_in_prometheus_render() {
        let registry = RuntimeMetricsRegistry::default();
        registry.on_tokenized_spans(3);
        registry.on_rehydrated_tokens(2);
        registry.on_unrestored_tokens(1);

        let rendered = registry.render_prometheus().expect("metrics should render");
        // /metrics must expose the new counters, not only the internal
        // snapshot fields the hooks also feed.
        assert!(rendered.contains("pokrov_tokenized_spans_total 3"));
        assert!(rendered.contains("pokrov_rehydrated_tokens_total 2"));
        assert!(rendered.contains("pokrov_unrestored_tokens_total 1"));
    }

    #[test]
    fn can_force_metrics_render_failure_for_degraded_readiness_checks() {
        let registry = RuntimeMetricsRegistry::default();
        registry.set_force_render_failure(true);
        let error =
            registry.render_prometheus().expect_err("forced flag should make rendering fail");
        assert!(error.contains("forced metrics rendering failure"));
    }
}
