use http::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LLMErrorCode {
    InvalidRequest,
    Unauthorized,
    PolicyBlocked,
    ModelNotRouted,
    AliasConflict,
    UpstreamError,
    UpstreamUnavailable,
}

impl LLMErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::PolicyBlocked => "policy_blocked",
            Self::ModelNotRouted => "model_not_routed",
            Self::AliasConflict => "alias_conflict",
            Self::UpstreamError => "upstream_error",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LLMProxyError {
    #[error("invalid request: {message}")]
    InvalidRequest { request_id: String, message: String },
    #[error("unauthorized: {message}")]
    Unauthorized { request_id: String, message: String },
    #[error("request blocked by policy")]
    PolicyBlocked { request_id: String, message: String },
    #[error("model is not routed: {model}")]
    ModelNotRouted { request_id: String, model: String },
    #[error("alias conflict: {message}")]
    AliasConflict { request_id: String, message: String },
    #[error("upstream request failed: {message}")]
    UpstreamError {
        request_id: String,
        provider_id: Option<String>,
        upstream_status: Option<u16>,
        message: String,
    },
    #[error("upstream unavailable: {message}")]
    UpstreamUnavailable {
        request_id: String,
        provider_id: Option<String>,
        upstream_status: Option<u16>,
        message: String,
    },
}

impl LLMProxyError {
    pub fn invalid_request(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidRequest { request_id: request_id.into(), message: message.into() }
    }

    pub fn unauthorized(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Unauthorized { request_id: request_id.into(), message: message.into() }
    }

    pub fn policy_blocked(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::PolicyBlocked { request_id: request_id.into(), message: message.into() }
    }

    pub fn model_not_routed(request_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self::ModelNotRouted { request_id: request_id.into(), model: model.into() }
    }

    pub fn alias_conflict(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::AliasConflict { request_id: request_id.into(), message: message.into() }
    }

    pub fn upstream_error(
        request_id: impl Into<String>,
        provider_id: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamError {
            request_id: request_id.into(),
            provider_id,
            upstream_status: None,
            message: message.into(),
        }
    }

    pub fn upstream_error_with_status(
        request_id: impl Into<String>,
        provider_id: Option<String>,
        upstream_status: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamError {
            request_id: request_id.into(),
            provider_id,
            upstream_status,
            message: message.into(),
        }
    }

    pub fn upstream_unavailable(
        request_id: impl Into<String>,
        provider_id: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamUnavailable {
            request_id: request_id.into(),
            provider_id,
            upstream_status: None,
            message: message.into(),
        }
    }

    pub fn upstream_unavailable_with_status(
        request_id: impl Into<String>,
        provider_id: Option<String>,
        upstream_status: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamUnavailable {
            request_id: request_id.into(),
            provider_id,
            upstream_status,
            message: message.into(),
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            Self::PolicyBlocked { .. } => StatusCode::FORBIDDEN,
            Self::ModelNotRouted { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::AliasConflict { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::UpstreamError { .. } => StatusCode::BAD_GATEWAY,
            Self::UpstreamUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub const fn code(&self) -> LLMErrorCode {
        match self {
            Self::InvalidRequest { .. } => LLMErrorCode::InvalidRequest,
            Self::Unauthorized { .. } => LLMErrorCode::Unauthorized,
            Self::PolicyBlocked { .. } => LLMErrorCode::PolicyBlocked,
            Self::ModelNotRouted { .. } => LLMErrorCode::ModelNotRouted,
            Self::AliasConflict { .. } => LLMErrorCode::AliasConflict,
            Self::UpstreamError { .. } => LLMErrorCode::UpstreamError,
            Self::UpstreamUnavailable { .. } => LLMErrorCode::UpstreamUnavailable,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::InvalidRequest { request_id, .. }
            | Self::Unauthorized { request_id, .. }
            | Self::PolicyBlocked { request_id, .. }
            | Self::ModelNotRouted { request_id, .. }
            | Self::AliasConflict { request_id, .. }
            | Self::UpstreamError { request_id, .. }
            | Self::UpstreamUnavailable { request_id, .. } => request_id,
        }
    }

    pub fn provider_id(&self) -> Option<&str> {
        match self {
            Self::UpstreamError { provider_id, .. }
            | Self::UpstreamUnavailable { provider_id, .. } => provider_id.as_deref(),
            _ => None,
        }
    }

    pub const fn upstream_status(&self) -> Option<u16> {
        match self {
            Self::UpstreamError { upstream_status, .. }
            | Self::UpstreamUnavailable { upstream_status, .. } => *upstream_status,
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::InvalidRequest { message, .. }
            | Self::Unauthorized { message, .. }
            | Self::PolicyBlocked { message, .. }
            | Self::AliasConflict { message, .. } => message.clone(),
            Self::ModelNotRouted { .. } => {
                "requested model is not routed to a configured provider".to_string()
            }
            Self::UpstreamError { .. } => "upstream request failed".to_string(),
            Self::UpstreamUnavailable { .. } => "upstream provider is unavailable".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::LLMProxyError;

    #[test]
    fn alias_conflict_maps_to_internal_server_error() {
        let error = LLMProxyError::alias_conflict("req-1", "duplicate alias");
        assert_eq!(error.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
