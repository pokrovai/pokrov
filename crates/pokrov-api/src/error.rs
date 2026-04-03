use axum::{http::StatusCode, response::IntoResponse, Json};
use pokrov_proxy_llm::errors::LLMProxyError;
use tracing::warn;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub request_id: String,
}

impl ApiError {
    pub fn invalid_request(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
            request_id: request_id.into(),
        }
    }

    pub fn unauthorized(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: message.into(),
            request_id: request_id.into(),
        }
    }

    pub fn payload_too_large(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "invalid_request",
            message: message.into(),
            request_id: request_id.into(),
        }
    }

    pub fn invalid_profile(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_profile",
            message: message.into(),
            request_id: request_id.into(),
        }
    }

    pub fn internal(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: message.into(),
            request_id: request_id.into(),
        }
    }

    pub fn from_llm_proxy(error: LLMProxyError) -> Self {
        warn!(
            component = "api",
            action = "llm_proxy_error",
            request_id = %error.request_id(),
            error_code = error.code().as_str(),
            status = %error.status_code(),
            provider_id = ?error.provider_id(),
            upstream_status = ?error.upstream_status(),
            error = %error,
            "llm proxy request failed"
        );

        Self {
            status: error.status_code(),
            code: error.code().as_str(),
            message: error.message(),
            request_id: error.request_id().to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let payload = serde_json::json!({
            "request_id": self.request_id,
            "error": {
                "code": self.code,
                "message": self.message,
            },
        });
        (self.status, Json(payload)).into_response()
    }
}
