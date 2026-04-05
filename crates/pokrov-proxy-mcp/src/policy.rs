use pokrov_config::model::{McpConfig, McpToolPolicy};
use pokrov_core::types::PolicyAction;

use crate::{
    errors::McpProxyError,
    types::{
        McpPolicyReason, McpToolCallRequest, McpToolPolicyDecision, McpUpstreamRequestContext,
    },
};

#[derive(Debug, Clone)]
pub struct ResolvedToolCall {
    pub decision: McpToolPolicyDecision,
    pub upstream: McpUpstreamRequestContext,
    pub argument_policy: Option<McpToolPolicy>,
    pub output_sanitization: bool,
}

pub fn resolve_tool_call(
    config: &McpConfig,
    request_id: &str,
    request: &McpToolCallRequest,
    profile_id: &str,
) -> Result<ResolvedToolCall, McpProxyError> {
    let server = config
        .servers
        .iter()
        .find(|server| server.enabled && server.id == request.server)
        .ok_or_else(|| {
            McpProxyError::tool_call_blocked(
                request_id,
                &request.server,
                &request.tool,
                McpPolicyReason::ServerNotAllowlisted.as_str(),
                1,
            )
        })?;

    if server.blocked_tools.iter().any(|tool| tool == &request.tool) {
        return Err(McpProxyError::tool_call_blocked(
            request_id,
            &request.server,
            &request.tool,
            McpPolicyReason::ToolBlocklisted.as_str(),
            1,
        ));
    }

    if !server.allowed_tools.iter().any(|tool| tool == &request.tool) {
        return Err(McpProxyError::tool_call_blocked(
            request_id,
            &request.server,
            &request.tool,
            McpPolicyReason::ToolNotAllowlisted.as_str(),
            1,
        ));
    }

    let tool_policy = server.tools.get(&request.tool).cloned();
    if tool_policy.as_ref().map(|policy| !policy.enabled).unwrap_or(false) {
        return Err(McpProxyError::tool_call_blocked(
            request_id,
            &request.server,
            &request.tool,
            McpPolicyReason::ToolBlocklisted.as_str(),
            1,
        ));
    }

    let output_sanitization = tool_policy
        .as_ref()
        .and_then(|policy| policy.output_sanitization)
        .unwrap_or(config.defaults.output_sanitization);

    Ok(ResolvedToolCall {
        decision: McpToolPolicyDecision {
            profile_id: profile_id.to_string(),
            allowed: true,
            final_action: PolicyAction::Allow,
            reason: McpPolicyReason::Allowed,
            rule_hits_total: 0,
        },
        upstream: McpUpstreamRequestContext {
            request_id: request_id.to_string(),
            server_id: request.server.clone(),
            tool_id: request.tool.clone(),
            endpoint: server.endpoint.clone(),
            timeout_ms: config.defaults.upstream_timeout_ms,
            upstream_bearer_token: None,
        },
        argument_policy: tool_policy,
        output_sanitization,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pokrov_config::model::{
        McpConfig, McpDefaultsConfig, McpServerDefinition, McpToolPolicy, ToolArgumentConstraints,
    };

    use super::resolve_tool_call;
    use crate::{
        errors::McpErrorCode,
        types::{McpRequestMetadata, McpToolCallRequest},
    };

    fn base_config() -> McpConfig {
        McpConfig {
            defaults: McpDefaultsConfig {
                profile_id: "strict".to_string(),
                upstream_timeout_ms: 10_000,
                output_sanitization: true,
            },
            servers: vec![McpServerDefinition {
                id: "repo-tools".to_string(),
                endpoint: "http://repo-tools.internal".to_string(),
                enabled: true,
                allowed_tools: vec!["read_file".to_string(), "write_file".to_string()],
                blocked_tools: vec!["write_file".to_string()],
                tools: BTreeMap::from([(
                    "read_file".to_string(),
                    McpToolPolicy {
                        enabled: true,
                        argument_schema: None,
                        argument_constraints: ToolArgumentConstraints::default(),
                        output_sanitization: Some(true),
                    },
                )]),
            }],
        }
    }

    #[test]
    fn blocklist_precedence_blocks_tool_even_when_allowlisted() {
        let config = base_config();
        let request = McpToolCallRequest {
            server: "repo-tools".to_string(),
            tool: "write_file".to_string(),
            arguments: serde_json::json!({"path": "src/lib.rs"}),
            metadata: McpRequestMetadata::default(),
        };

        let error = resolve_tool_call(&config, "req-1", &request, "strict")
            .expect_err("blocked tool must be rejected");

        assert_eq!(error.code(), McpErrorCode::ToolCallBlocked);
        assert_eq!(error.status_code(), http::StatusCode::FORBIDDEN);
        let details = error.details().expect("blocked response should carry details");
        assert_eq!(details.reason.as_deref(), Some("tool_blocklisted"));
    }

    #[test]
    fn allowlisted_enabled_tool_passes_policy_resolution() {
        let config = base_config();
        let request = McpToolCallRequest {
            server: "repo-tools".to_string(),
            tool: "read_file".to_string(),
            arguments: serde_json::json!({"path": "src/lib.rs"}),
            metadata: McpRequestMetadata::default(),
        };

        let resolved = resolve_tool_call(&config, "req-2", &request, "strict")
            .expect("allowlisted tool should pass policy resolution");

        assert!(resolved.decision.allowed);
        assert!(resolved.output_sanitization);
        assert_eq!(resolved.upstream.server_id, "repo-tools");
        assert_eq!(resolved.upstream.tool_id, "read_file");
    }
}
