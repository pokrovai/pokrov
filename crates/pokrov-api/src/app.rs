use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use pokrov_config::rate_limit::RateLimitEnforcementMode;
use pokrov_core::SanitizationEngine;
use pokrov_metrics::registry::RuntimeMetricsRegistry;
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;
use pokrov_proxy_llm::handler::LLMProxyHandler;
use pokrov_proxy_mcp::handler::McpProxyHandler;

use crate::{
    handlers::{chat_completions, evaluate, health, mcp_tool_call, metrics, ready},
    middleware::{
        active_requests_middleware, rate_limit::RateLimiter, request_id_middleware,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeStateView {
    Starting,
    Ready,
    Draining,
    Stopped,
}

pub trait RuntimeStateReader: Send + Sync {
    fn state(&self) -> RuntimeStateView;
    fn config_loaded(&self) -> bool;
    fn llm_routes_loaded(&self) -> bool;
    fn mcp_routes_loaded(&self) -> bool;
    fn active_requests(&self) -> usize;
    fn on_request_started(&self);
    fn on_request_finished(&self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedApiKeyBinding {
    pub key: String,
    pub profile: String,
}

#[derive(Clone)]
pub struct SanitizationState {
    pub enabled: bool,
    pub evaluator: Option<Arc<SanitizationEngine>>,
    pub api_key_bindings: Arc<Vec<ResolvedApiKeyBinding>>,
}

impl Default for SanitizationState {
    fn default() -> Self {
        Self {
            enabled: true,
            evaluator: None,
            api_key_bindings: Arc::new(Vec::new()),
        }
    }
}

impl SanitizationState {
    pub fn is_authorized(&self, token: &str, profile_id: &str) -> bool {
        let mut matched = 0u8;

        for binding in self.api_key_bindings.iter() {
            let key_match = constant_time_eq(binding.key.as_bytes(), token.as_bytes()) as u8;
            let profile_match = (binding.profile == profile_id) as u8;
            matched |= key_match & profile_match;
        }

        matched == 1
    }

    pub fn profile_for_token(&self, token: &str) -> Option<String> {
        self.api_key_bindings
            .iter()
            .find(|binding| constant_time_eq(binding.key.as_bytes(), token.as_bytes()))
            .map(|binding| binding.profile.clone())
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    let limit = left.len().max(right.len());

    for idx in 0..limit {
        let left_byte = left.get(idx).copied().unwrap_or_default();
        let right_byte = right.get(idx).copied().unwrap_or_default();
        diff |= usize::from(left_byte ^ right_byte);
    }

    diff == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitReason {
    WithinBudget,
    RequestBudgetExhausted,
    TokenBudgetExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub reason: RateLimitReason,
    pub retry_after_ms: u64,
    pub limit: u32,
    pub remaining: u32,
    pub reset_at_unix_ms: u64,
    pub enforcement_mode: RateLimitEnforcementMode,
}

#[derive(Debug, Clone)]
pub struct RateLimitWindowState {
    pub window_started_at: Instant,
    pub consumed: u32,
}

impl RateLimitWindowState {
    pub fn new(window_started_at: Instant) -> Self {
        Self {
            window_started_at,
            consumed: 0,
        }
    }

    pub fn reset_if_stale(&mut self, now: Instant, window: Duration) {
        if now.duration_since(self.window_started_at) >= window {
            self.window_started_at = now;
            self.consumed = 0;
        }
    }
}

#[derive(Clone)]
pub struct RateLimitState {
    pub enabled: bool,
    pub limiter: Option<Arc<RateLimiter>>,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            enabled: false,
            limiter: None,
        }
    }
}

#[derive(Clone)]
pub struct LlmProxyState {
    pub enabled: bool,
    pub handler: Option<Arc<LLMProxyHandler>>,
}

impl Default for LlmProxyState {
    fn default() -> Self {
        Self {
            enabled: false,
            handler: None,
        }
    }
}

#[derive(Clone)]
pub struct McpProxyState {
    pub enabled: bool,
    pub handler: Option<Arc<McpProxyHandler>>,
}

impl Default for McpProxyState {
    fn default() -> Self {
        Self {
            enabled: false,
            handler: None,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub lifecycle: Arc<dyn RuntimeStateReader>,
    pub metrics: SharedRuntimeMetricsHooks,
    pub metrics_registry: Arc<RuntimeMetricsRegistry>,
    pub sanitization: SanitizationState,
    pub rate_limit: RateLimitState,
    pub llm: LlmProxyState,
    pub mcp: McpProxyState,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::handle_health))
        .route("/ready", get(ready::handle_ready))
        .route("/metrics", get(metrics::handle_metrics))
        .route("/v1/sanitize/evaluate", post(evaluate::handle_evaluate))
        .route("/v1/chat/completions", post(chat_completions::handle_chat_completions))
        .route("/v1/mcp/tool-call", post(mcp_tool_call::handle_mcp_tool_call))
        .route(
            "/v1/mcp/tools/{tool_name}/invoke",
            post(mcp_tool_call::handle_mcp_tool_call_with_tool_name),
        )
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(state.clone(), active_requests_middleware))
        .layer(axum::middleware::from_fn_with_state(state.clone(), request_id_middleware))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, ResolvedApiKeyBinding, SanitizationState};

    #[test]
    fn constant_time_compare_matches_only_equal_values() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
        assert!(!constant_time_eq(b"abc123", b"abc124"));
        assert!(!constant_time_eq(b"abc123", b"abc1234"));
    }

    #[test]
    fn authorization_requires_token_and_profile_match() {
        let state = SanitizationState {
            enabled: true,
            evaluator: None,
            api_key_bindings: std::sync::Arc::new(vec![ResolvedApiKeyBinding {
                key: "token-1".to_string(),
                profile: "strict".to_string(),
            }]),
        };

        assert!(state.is_authorized("token-1", "strict"));
        assert!(!state.is_authorized("token-1", "minimal"));
        assert!(!state.is_authorized("token-2", "strict"));
    }
}
