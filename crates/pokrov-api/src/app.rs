use std::sync::Arc;

use axum::{routing::get, Router};
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;

use crate::{
    handlers::{health, ready},
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

#[derive(Clone)]
pub struct AppState {
    pub lifecycle: Arc<dyn RuntimeStateReader>,
    pub metrics: SharedRuntimeMetricsHooks,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::handle_health))
        .route("/ready", get(ready::handle_ready))
        .layer(axum::middleware::from_fn_with_state(state.clone(), active_requests_middleware))
        .layer(axum::middleware::from_fn_with_state(state.clone(), request_id_middleware))
        .with_state(state)
}
