use axum::{http::StatusCode, response::IntoResponse, Json};
use pokrov_proxy_llm::errors::LLMProxyError;
use pokrov_proxy_mcp::errors::McpProxyError;
use tracing::warn;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub request_id: String,
    pub allowed: Option<bool>,
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn invalid_request(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
        }
    }

    pub fn unauthorized(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
        }
    }

    pub fn payload_too_large(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "invalid_request",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
        }
    }

    pub fn invalid_profile(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_profile",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
        }
    }

    pub fn internal(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
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
            allowed: None,
            details: None,
        }
    }

    pub fn from_mcp_proxy(error: McpProxyError) -> Self {
        warn!(
            component = "api",
            action = "mcp_proxy_error",
            request_id = %error.request_id(),
            error_code = error.code().as_str(),
            status = %error.status_code(),
            upstream_status = ?error.upstream_status(),
            error = %error,
            "mcp proxy request failed"
        );

        Self {
            status: error.status_code(),
            code: error.code().as_str(),
            message: error.message(),
            request_id: error.request_id().to_string(),
            allowed: Some(false),
            details: error
                .details()
                .and_then(|details| serde_json::to_value(details).ok()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut payload = serde_json::Map::from_iter([
            (
                "request_id".to_string(),
                serde_json::Value::String(self.request_id),
            ),
        ]);

        if let Some(allowed) = self.allowed {
            payload.insert("allowed".to_string(), serde_json::Value::Bool(allowed));
        }

        let mut error = serde_json::Map::from_iter([
            (
                "code".to_string(),
                serde_json::Value::String(self.code.to_string()),
            ),
            (
                "message".to_string(),
                serde_json::Value::String(self.message),
            ),
        ]);

        if let Some(details) = self.details {
            error.insert("details".to_string(), details);
        }

        payload.insert("error".to_string(), serde_json::Value::Object(error));
        (self.status, Json(serde_json::Value::Object(payload))).into_response()
    }
}
