use axum::{http::StatusCode, response::IntoResponse, Json};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: message.into() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let payload = serde_json::json!({
            "error": self.message,
        });
        (self.status, Json(payload)).into_response()
    }
}
