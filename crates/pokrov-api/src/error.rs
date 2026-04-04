use axum::{
    http::{header::HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use pokrov_config::model::ResponseMetadataMode;
use pokrov_proxy_llm::errors::LLMProxyError;
use pokrov_proxy_mcp::errors::McpProxyError;
use tracing::warn;

use crate::app::RateLimitDecision;

const RETRY_AFTER: HeaderName = HeaderName::from_static("retry-after");
const X_RATE_LIMIT_LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");
const X_RATE_LIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");
const X_RATE_LIMIT_RESET: HeaderName = HeaderName::from_static("x-ratelimit-reset");

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub request_id: String,
    pub allowed: Option<bool>,
    pub details: Option<serde_json::Value>,
    pub rate_limit: Option<RateLimitDecision>,
    pub response_metadata_mode: ResponseMetadataMode,
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn unsupported_request_subset(
        request_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_request_subset",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn gateway_unauthorized(request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "gateway_unauthorized",
            message: "Client is not authorized to use Pokrov gateway".to_string(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn upstream_credential_missing(request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "upstream_credential_missing",
            message: "Upstream provider credential is required for passthrough mode".to_string(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn passthrough_requires_api_key_gateway_auth(request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "passthrough_requires_api_key_gateway_auth",
            message: "Passthrough mode requires gateway auth via X-Pokrov-Api-Key".to_string(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn upstream_credential_invalid(request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "upstream_credential_invalid",
            message: "Upstream provider credential is invalid".to_string(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn responses_stream_terminated(request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "upstream_error",
            message: "responses stream terminated due to upstream stream error".to_string(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn rate_limit_exceeded(request_id: impl Into<String>, decision: RateLimitDecision) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limit_exceeded",
            message: "Request rejected by rate-limit policy".to_string(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: Some(decision),
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn runtime_not_ready(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "runtime_not_ready",
            message: message.into(),
            request_id: request_id.into(),
            allowed: None,
            details: None,
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn from_llm_proxy_for_responses(error: LLMProxyError) -> Self {
        let mut mapped = Self::from_llm_proxy(error);
        if mapped.code == "invalid_request"
            && mapped.message.contains("minimal responses subset")
        {
            mapped.code = "unsupported_request_subset";
        }
        mapped
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
            rate_limit: None,
            response_metadata_mode: ResponseMetadataMode::Enabled,
        }
    }

    pub fn with_response_metadata_mode(mut self, mode: ResponseMetadataMode) -> Self {
        self.response_metadata_mode = mode;
        self
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

        if self.response_metadata_mode == ResponseMetadataMode::Enabled {
            payload.insert(
                "pokrov".to_string(),
                serde_json::json!({
                    "error_code": self.code,
                }),
            );
        }

        if let Some(rate_limit) = self.rate_limit {
            payload.insert(
                "retry_after_ms".to_string(),
                serde_json::Value::Number(rate_limit.retry_after_ms.into()),
            );
            payload.insert(
                "limit".to_string(),
                serde_json::Value::Number(rate_limit.limit.into()),
            );
            payload.insert(
                "remaining".to_string(),
                serde_json::Value::Number(rate_limit.remaining.into()),
            );
            payload.insert(
                "reset_at".to_string(),
                serde_json::Value::Number(rate_limit.reset_at_unix_ms.into()),
            );
        }

        let mut response = (self.status, Json(serde_json::Value::Object(payload))).into_response();
        if let Some(rate_limit) = self.rate_limit {
            let retry_after_seconds = retry_after_header_seconds(rate_limit.retry_after_ms);
            if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
                response.headers_mut().insert(RETRY_AFTER, value);
            }
            if let Ok(value) = HeaderValue::from_str(&rate_limit.limit.to_string()) {
                response.headers_mut().insert(X_RATE_LIMIT_LIMIT, value);
            }
            if let Ok(value) = HeaderValue::from_str(&rate_limit.remaining.to_string()) {
                response.headers_mut().insert(X_RATE_LIMIT_REMAINING, value);
            }
            let reset_seconds = reset_header_seconds(rate_limit.reset_at_unix_ms);
            if let Ok(value) = HeaderValue::from_str(&reset_seconds.to_string()) {
                response.headers_mut().insert(X_RATE_LIMIT_RESET, value);
            }
        }

        response
    }
}

fn retry_after_header_seconds(retry_after_ms: u64) -> u64 {
    (retry_after_ms.saturating_add(999) / 1000).max(1)
}

fn reset_header_seconds(reset_at_unix_ms: u64) -> u64 {
    reset_at_unix_ms / 1000
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;
    use pokrov_config::rate_limit::RateLimitEnforcementMode;

    use crate::app::{RateLimitDecision, RateLimitReason};

    use super::{ApiError, RETRY_AFTER, X_RATE_LIMIT_RESET, retry_after_header_seconds};

    #[test]
    fn retry_after_seconds_uses_ceiling_rounding() {
        assert_eq!(retry_after_header_seconds(0), 1);
        assert_eq!(retry_after_header_seconds(1), 1);
        assert_eq!(retry_after_header_seconds(1000), 1);
        assert_eq!(retry_after_header_seconds(1500), 2);
    }

    #[test]
    fn rate_limit_response_headers_use_ceiling_retry_after_seconds() {
        let decision = RateLimitDecision {
            allowed: false,
            reason: RateLimitReason::RequestBudgetExhausted,
            retry_after_ms: 1500,
            limit: 10,
            remaining: 0,
            reset_at_unix_ms: 1_700_000_000_000,
            enforcement_mode: RateLimitEnforcementMode::Enforce,
        };

        let response = ApiError::rate_limit_exceeded("request-1", decision).into_response();
        let header = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok());
        let reset_header = response
            .headers()
            .get(X_RATE_LIMIT_RESET)
            .and_then(|value| value.to_str().ok());

        assert_eq!(header, Some("2"));
        assert_eq!(reset_header, Some("1700000000"));
    }
}
