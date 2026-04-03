use std::sync::Arc;

use axum::{routing::{get, post}, Router};
use pokrov_core::SanitizationEngine;
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;

use crate::{
    handlers::{evaluate, health, ready},
    middleware::{active_requests_middleware, request_id_middleware},
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
        self.api_key_bindings
            .iter()
            .any(|binding| binding.key == token && binding.profile == profile_id)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub lifecycle: Arc<dyn RuntimeStateReader>,
    pub metrics: SharedRuntimeMetricsHooks,
    pub sanitization: SanitizationState,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::handle_health))
        .route("/ready", get(ready::handle_ready))
        .route("/v1/sanitize/evaluate", post(evaluate::handle_evaluate))
        .layer(axum::middleware::from_fn_with_state(state.clone(), active_requests_middleware))
        .layer(axum::middleware::from_fn_with_state(state.clone(), request_id_middleware))
        .with_state(state)
}
