use axum::{
    extract::{Extension, Query},
    Json,
};
use serde::Deserialize;

use crate::handlers::HealthResponse;

const MAX_HEALTH_DELAY_MS: u64 = 5_000;

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
        let bounded_delay = delay_ms.min(MAX_HEALTH_DELAY_MS);
        tokio::time::sleep(std::time::Duration::from_millis(bounded_delay)).await;
    }

    Json(HealthResponse { status: "ok", request_id })
}
