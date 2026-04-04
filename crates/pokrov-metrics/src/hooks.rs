use std::sync::Arc;

use pokrov_core::types::PolicyAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Starting,
    Ready,
    Draining,
    Stopped,
}

pub trait RuntimeMetricsHooks: Send + Sync {
    fn on_lifecycle_event(&self, _event: LifecycleEvent) {}
    fn on_request_started(&self) {}
    fn on_request_finished(&self) {}
    fn on_rule_hits(&self, _hits: u32) {}
    fn on_payload_transformed(&self, _count: u32) {}
    fn on_evaluation_blocked(&self) {}
    fn on_llm_final_action(&self, _action: PolicyAction) {}
    fn on_llm_blocked_request(&self) {}
    fn on_llm_upstream_status(&self, _status: u16) {}
    fn on_llm_request_duration_ms(&self, _duration_ms: u64) {}
    fn on_mcp_tool_call(&self) {}
    fn on_mcp_tool_call_blocked(&self) {}
    fn on_mcp_tool_call_duration_ms(&self, _duration_ms: u64) {}
    fn on_request_outcome(
        &self,
        _route: &str,
        _path_class: &str,
        _status: u16,
        _decision: &str,
    ) {
    }
    fn on_blocked_request(&self, _route: &str, _block_reason: &str, _policy_profile: &str) {}
    fn on_rate_limit_event(
        &self,
        _route: &str,
        _limit_kind: &str,
        _decision: &str,
        _policy_profile: &str,
    ) {
    }
    fn on_auth_decision(&self, _auth_mode: &str, _stage: &str, _decision: &str) {}
    fn on_upstream_error(&self, _route: &str, _provider: &str, _error_class: &str) {}
    fn on_responses_auth_stage(&self, auth_mode: &str, stage: &str, decision: &str) {
        self.on_auth_decision(auth_mode, stage, decision);
    }
    fn on_responses_upstream_error(&self, provider: &str, error_class: &str) {
        self.on_upstream_error("/v1/responses", provider, error_class);
    }
    fn on_request_duration_seconds(&self, _route: &str, _path_class: &str, _decision: &str, _seconds: f64) {
    }
}

#[derive(Debug, Default)]
pub struct NoopRuntimeMetricsHooks;

impl RuntimeMetricsHooks for NoopRuntimeMetricsHooks {}

pub type SharedRuntimeMetricsHooks = Arc<dyn RuntimeMetricsHooks>;
