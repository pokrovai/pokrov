use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};

use crate::{
    app::{AppState, RuntimeStateView},
    handlers::{ReadyChecks, ReadyResponse},
};

pub async fn handle_ready(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
) -> (StatusCode, Json<ReadyResponse>) {
    let runtime_state = state.lifecycle.state();
    let config_ok = state.lifecycle.config_loaded();
    let auth_ok = !state.auth.identity_resolution_order.is_empty();
    let policy_ok = !state.sanitization.enabled || state.sanitization.evaluator.is_some();
    let llm_ok = !state.llm.enabled || state.lifecycle.llm_routes_loaded();
    let mcp_ok = !state.mcp.enabled || state.lifecycle.mcp_routes_loaded();
    let metrics_ok = state.metrics_registry.render_prometheus().is_ok();
    let active_requests = state.lifecycle.active_requests() as u64;

    let runtime_ready = matches!(runtime_state, RuntimeStateView::Ready);
    let all_ready = runtime_ready && config_ok && auth_ok && policy_ok && llm_ok && mcp_ok && metrics_ok;

    let (status, runtime, code) = match runtime_state {
        RuntimeStateView::Ready if all_ready => ("ready", "ok", StatusCode::OK),
        RuntimeStateView::Ready => ("degraded", "ok", StatusCode::SERVICE_UNAVAILABLE),
        RuntimeStateView::Draining => ("draining", "draining", StatusCode::SERVICE_UNAVAILABLE),
        RuntimeStateView::Starting => ("starting", "pending", StatusCode::SERVICE_UNAVAILABLE),
        RuntimeStateView::Stopped => ("draining", "draining", StatusCode::SERVICE_UNAVAILABLE),
    };

    let config = if config_ok { "ok" } else { "pending" };
    let auth = if auth_ok { "ok" } else { "pending" };
    let policy = if policy_ok { "ok" } else { "pending" };
    let llm = if llm_ok { "ok" } else { "pending" };
    let mcp = if mcp_ok { "ok" } else { "pending" };

    (
        code,
        Json(ReadyResponse {
            status,
            request_id,
            checks: ReadyChecks { config, auth, policy, llm, mcp, runtime, active_requests },
        }),
    )
}
