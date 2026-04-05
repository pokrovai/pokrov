use http::StatusCode;

use crate::types::McpErrorDetails;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpErrorCode {
    InvalidRequest,
    Unauthorized,
    ToolCallBlocked,
    ArgumentValidationFailed,
    UnsupportedVariant,
    UpstreamError,
    UpstreamUnavailable,
}

impl McpErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::ToolCallBlocked => "tool_call_blocked",
            Self::ArgumentValidationFailed => "argument_validation_failed",
            Self::UnsupportedVariant => "unsupported_variant",
            Self::UpstreamError => "upstream_error",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum McpProxyError {
    #[error("invalid request: {message}")]
    InvalidRequest { request_id: String, message: String },
    #[error("unauthorized: {message}")]
    Unauthorized { request_id: String, message: String },
    #[error("tool call blocked by policy")]
    ToolCallBlocked {
        request_id: String,
        server: String,
        tool: String,
        reason: String,
        violation_count: u32,
    },
    #[error("tool arguments failed validation")]
    ArgumentValidationFailed {
        request_id: String,
        server: String,
        tool: String,
        violation_count: u32,
    },
    #[error("unsupported MCP transport variant")]
    UnsupportedVariant { request_id: String, message: String },
    #[error("upstream request failed: {message}")]
    UpstreamError {
        request_id: String,
        server: String,
        tool: String,
        upstream_status: Option<u16>,
        message: String,
    },
    #[error("upstream is unavailable: {message}")]
    UpstreamUnavailable {
        request_id: String,
        server: String,
        tool: String,
        upstream_status: Option<u16>,
        message: String,
    },
}

impl McpProxyError {
    pub fn invalid_request(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidRequest { request_id: request_id.into(), message: message.into() }
    }

    pub fn unauthorized(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Unauthorized { request_id: request_id.into(), message: message.into() }
    }

    pub fn tool_call_blocked(
        request_id: impl Into<String>,
        server: impl Into<String>,
        tool: impl Into<String>,
        reason: impl Into<String>,
        violation_count: u32,
    ) -> Self {
        Self::ToolCallBlocked {
            request_id: request_id.into(),
            server: server.into(),
            tool: tool.into(),
            reason: reason.into(),
            violation_count,
        }
    }

    pub fn argument_validation_failed(
        request_id: impl Into<String>,
        server: impl Into<String>,
        tool: impl Into<String>,
        violation_count: u32,
    ) -> Self {
        Self::ArgumentValidationFailed {
            request_id: request_id.into(),
            server: server.into(),
            tool: tool.into(),
            violation_count,
        }
    }

    pub fn unsupported_variant(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::UnsupportedVariant { request_id: request_id.into(), message: message.into() }
    }

    pub fn upstream_error(
        request_id: impl Into<String>,
        server: impl Into<String>,
        tool: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamError {
            request_id: request_id.into(),
            server: server.into(),
            tool: tool.into(),
            upstream_status: None,
            message: message.into(),
        }
    }

    pub fn upstream_error_with_status(
        request_id: impl Into<String>,
        server: impl Into<String>,
        tool: impl Into<String>,
        upstream_status: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamError {
            request_id: request_id.into(),
            server: server.into(),
            tool: tool.into(),
            upstream_status,
            message: message.into(),
        }
    }

    pub fn upstream_unavailable(
        request_id: impl Into<String>,
        server: impl Into<String>,
        tool: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamUnavailable {
            request_id: request_id.into(),
            server: server.into(),
            tool: tool.into(),
            upstream_status: None,
            message: message.into(),
        }
    }

    pub fn upstream_unavailable_with_status(
        request_id: impl Into<String>,
        server: impl Into<String>,
        tool: impl Into<String>,
        upstream_status: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self::UpstreamUnavailable {
            request_id: request_id.into(),
            server: server.into(),
            tool: tool.into(),
            upstream_status,
            message: message.into(),
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            Self::ToolCallBlocked { .. } => StatusCode::FORBIDDEN,
            Self::ArgumentValidationFailed { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::UnsupportedVariant { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::UpstreamError { .. } => StatusCode::BAD_GATEWAY,
            Self::UpstreamUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub const fn code(&self) -> McpErrorCode {
        match self {
            Self::InvalidRequest { .. } => McpErrorCode::InvalidRequest,
            Self::Unauthorized { .. } => McpErrorCode::Unauthorized,
            Self::ToolCallBlocked { .. } => McpErrorCode::ToolCallBlocked,
            Self::ArgumentValidationFailed { .. } => McpErrorCode::ArgumentValidationFailed,
            Self::UnsupportedVariant { .. } => McpErrorCode::UnsupportedVariant,
            Self::UpstreamError { .. } => McpErrorCode::UpstreamError,
            Self::UpstreamUnavailable { .. } => McpErrorCode::UpstreamUnavailable,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::InvalidRequest { request_id, .. }
            | Self::Unauthorized { request_id, .. }
            | Self::ToolCallBlocked { request_id, .. }
            | Self::ArgumentValidationFailed { request_id, .. }
            | Self::UnsupportedVariant { request_id, .. }
            | Self::UpstreamError { request_id, .. }
            | Self::UpstreamUnavailable { request_id, .. } => request_id,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::InvalidRequest { message, .. }
            | Self::Unauthorized { message, .. }
            | Self::UnsupportedVariant { message, .. } => message.clone(),
            Self::ToolCallBlocked { .. } => "Tool call blocked by policy".to_string(),
            Self::ArgumentValidationFailed { .. } => "Tool arguments failed validation".to_string(),
            Self::UpstreamError { .. } => "upstream request failed".to_string(),
            Self::UpstreamUnavailable { .. } => "upstream MCP server is unavailable".to_string(),
        }
    }

    pub fn details(&self) -> Option<McpErrorDetails> {
        match self {
            Self::ToolCallBlocked { server, tool, reason, violation_count, .. } => {
                Some(McpErrorDetails {
                    server: Some(server.clone()),
                    tool: Some(tool.clone()),
                    reason: Some(reason.clone()),
                    violation_count: Some(*violation_count),
                })
            }
            Self::ArgumentValidationFailed { server, tool, violation_count, .. } => {
                Some(McpErrorDetails {
                    server: Some(server.clone()),
                    tool: Some(tool.clone()),
                    reason: Some("argument_invalid".to_string()),
                    violation_count: Some(*violation_count),
                })
            }
            Self::UpstreamError { server, tool, .. }
            | Self::UpstreamUnavailable { server, tool, .. } => Some(McpErrorDetails {
                server: Some(server.clone()),
                tool: Some(tool.clone()),
                reason: None,
                violation_count: None,
            }),
            Self::InvalidRequest { .. }
            | Self::Unauthorized { .. }
            | Self::UnsupportedVariant { .. } => None,
        }
    }

    pub const fn upstream_status(&self) -> Option<u16> {
        match self {
            Self::UpstreamError { upstream_status, .. }
            | Self::UpstreamUnavailable { upstream_status, .. } => *upstream_status,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{McpErrorCode, McpProxyError};

    #[test]
    fn blocked_error_maps_to_forbidden_and_contains_metadata_only_details() {
        let error = McpProxyError::tool_call_blocked(
            "req-1",
            "repo-tools",
            "write_file",
            "tool_blocklisted",
            1,
        );

        assert_eq!(error.code(), McpErrorCode::ToolCallBlocked);
        assert_eq!(error.status_code(), http::StatusCode::FORBIDDEN);
        assert_eq!(error.message(), "Tool call blocked by policy");

        let details = error.details().expect("blocked error should expose details");
        assert_eq!(details.server.as_deref(), Some("repo-tools"));
        assert_eq!(details.tool.as_deref(), Some("write_file"));
        assert_eq!(details.reason.as_deref(), Some("tool_blocklisted"));
        assert_eq!(details.violation_count, Some(1));
    }
}
