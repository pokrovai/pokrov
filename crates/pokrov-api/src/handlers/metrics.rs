use axum::{extract::State, http::StatusCode, response::IntoResponse};

use crate::app::AppState;

pub async fn handle_metrics(State(state): State<AppState>) -> impl IntoResponse {
    match state.metrics_registry.render_prometheus() {
        Ok(body) => (
            StatusCode::OK,
            [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "text/plain; charset=utf-8")],
            format!("failed to encode metrics: {error}"),
        )
            .into_response(),
    }
}
