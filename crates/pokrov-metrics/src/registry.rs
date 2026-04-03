use std::sync::atomic::{AtomicU64, Ordering};

use pokrov_core::types::PolicyAction;

use crate::hooks::{LifecycleEvent, RuntimeMetricsHooks};

#[derive(Debug, Default)]
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
}

impl RuntimeMetricsRegistry {
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
            llm_action_allow_total: self.llm_action_allow_total.load(Ordering::Relaxed),
            llm_action_mask_total: self.llm_action_mask_total.load(Ordering::Relaxed),
            llm_action_replace_total: self.llm_action_replace_total.load(Ordering::Relaxed),
            llm_action_redact_total: self.llm_action_redact_total.load(Ordering::Relaxed),
            llm_action_block_total: self.llm_action_block_total.load(Ordering::Relaxed),
            llm_blocked_requests_total: self.llm_blocked_requests_total.load(Ordering::Relaxed),
            llm_upstream_2xx_total: self.llm_upstream_2xx_total.load(Ordering::Relaxed),
            llm_upstream_4xx_total: self.llm_upstream_4xx_total.load(Ordering::Relaxed),
            llm_upstream_5xx_total: self.llm_upstream_5xx_total.load(Ordering::Relaxed),
            llm_request_duration_ms_total: self.llm_request_duration_ms_total.load(Ordering::Relaxed),
            mcp_tool_calls_total: self.mcp_tool_calls_total.load(Ordering::Relaxed),
            mcp_tool_calls_blocked_total: self.mcp_tool_calls_blocked_total.load(Ordering::Relaxed),
            mcp_tool_call_duration_ms_total: self
                .mcp_tool_call_duration_ms_total
                .load(Ordering::Relaxed),
        }
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
        self.llm_request_duration_ms_total
            .fetch_add(duration_ms, Ordering::Relaxed);
    }

    fn on_mcp_tool_call(&self) {
        self.mcp_tool_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_mcp_tool_call_blocked(&self) {
        self.mcp_tool_calls_blocked_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn on_mcp_tool_call_duration_ms(&self, duration_ms: u64) {
        self.mcp_tool_call_duration_ms_total
            .fetch_add(duration_ms, Ordering::Relaxed);
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
