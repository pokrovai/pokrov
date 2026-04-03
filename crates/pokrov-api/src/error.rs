use axum::{http::StatusCode, response::IntoResponse, Json};
use pokrov_proxy_llm::errors::LLMProxyError;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub request_id: String,
    pub provider_id: Option<String>,
}

impl ApiError {
    pub fn invalid_request(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
            request_id: request_id.into(),
            provider_id: None,
        }
    }

    pub fn unauthorized(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: message.into(),
            request_id: request_id.into(),
            provider_id: None,
        }
    }

    pub fn invalid_profile(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_profile",
            message: message.into(),
            request_id: request_id.into(),
            provider_id: None,
        }
    }

    pub fn internal(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: message.into(),
            request_id: request_id.into(),
            provider_id: None,
        }
    }

    pub fn from_llm_proxy(error: LLMProxyError) -> Self {
        Self {
            status: error.status_code(),
            code: error.code().as_str(),
            message: error.message(),
            request_id: error.request_id().to_string(),
            provider_id: error.provider_id().map(str::to_string),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut payload = serde_json::json!({
            "request_id": self.request_id,
            "error": {
                "code": self.code,
                "message": self.message,
            },
        });
        if let Some(provider_id) = self.provider_id {
            payload["provider_id"] = serde_json::Value::String(provider_id);
        }
        (self.status, Json(payload)).into_response()
    }
}
