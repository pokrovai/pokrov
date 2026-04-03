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
}

#[derive(Debug, Default)]
pub struct NoopRuntimeMetricsHooks;

impl RuntimeMetricsHooks for NoopRuntimeMetricsHooks {}

pub type SharedRuntimeMetricsHooks = Arc<dyn RuntimeMetricsHooks>;
