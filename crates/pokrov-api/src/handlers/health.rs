use axum::{
    extract::{Extension, Query},
    Json,
};
use serde::Deserialize;

use crate::handlers::HealthResponse;

#[derive(Debug, Default, Deserialize)]
pub struct HealthQuery {
    #[serde(default)]
    delay_ms: Option<u64>,
}

pub async fn handle_health(
    Query(query): Query<HealthQuery>,
    Extension(request_id): Extension<String>,
) -> Json<HealthResponse> {
    if let Some(delay_ms) = query.delay_ms {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }

    Json(HealthResponse { status: "ok", request_id })
}
