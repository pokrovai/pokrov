use std::{sync::Arc, time::Instant};

use pokrov_config::model::McpConfig;
use pokrov_core::{
    types::{EvaluateRequest, EvaluationMode, PathClass, PolicyAction},
    SanitizationEngine,
};
use pokrov_metrics::hooks::SharedRuntimeMetricsHooks;

use crate::{
    audit::McpAuditEvent,
    errors::McpProxyError,
    policy::resolve_tool_call,
    types::{
        McpPolicyReason, McpSanitizationResult, McpToolCallRequest, McpToolCallResponse,
        McpToolResultEnvelope,
    },
    upstream::McpUpstreamClient,
    validate::validate_tool_arguments,
};

#[derive(Clone)]
pub struct McpProxyHandler {
    evaluator: Option<Arc<SanitizationEngine>>,
    metrics: SharedRuntimeMetricsHooks,
    config: Arc<McpConfig>,
    upstream: McpUpstreamClient,
}

impl McpProxyHandler {
    pub fn new(
        evaluator: Option<Arc<SanitizationEngine>>,
        metrics: SharedRuntimeMetricsHooks,
        config: McpConfig,
    ) -> Result<Self, McpProxyError> {
        Ok(Self {
            evaluator,
            metrics,
            config: Arc::new(config),
            upstream: McpUpstreamClient::new()?,
        })
    }

    pub fn routes_loaded(&self) -> bool {
        self.config.servers.iter().any(|server| server.enabled)
    }

    pub fn default_profile_id(&self) -> &str {
        &self.config.defaults.profile_id
    }

    pub async fn handle_tool_call(
        &self,
        request_id: String,
        request: McpToolCallRequest,
        api_key_profile: &str,
        auth_mode: &'static str,
        upstream_credential: Option<&str>,
    ) -> Result<McpToolCallResponse, McpProxyError> {
        let started = Instant::now();
        self.metrics.on_mcp_tool_call();

        let result = self
            .handle_tool_call_inner(
                request_id.clone(),
                request.clone(),
                api_key_profile,
                upstream_credential,
            )
            .await;

        match &result {
            Ok(response) => {
                self.emit_terminal_event(
                    &request_id,
                    &request.server,
                    &request.tool,
                    &response.pokrov.profile,
                    response.pokrov.action,
                    response.pokrov.rule_hits,
                    false,
                    Some(200),
                    started.elapsed().as_millis() as u64,
                    auth_mode,
                    if upstream_credential.is_some() { "request" } else { "config" },
                );
            }
            Err(error) => {
                let blocked = matches!(
                    error,
                    McpProxyError::ToolCallBlocked { .. }
                        | McpProxyError::ArgumentValidationFailed { .. }
                );
                self.emit_terminal_event(
                    error.request_id(),
                    &request.server,
                    &request.tool,
                    resolve_profile_id(request.metadata.profile.as_deref(), api_key_profile),
                    PolicyAction::Block,
                    0,
                    blocked,
                    error.upstream_status(),
                    started.elapsed().as_millis() as u64,
                    auth_mode,
                    if upstream_credential.is_some() { "request" } else { "config" },
                );
            }
        }

        result
    }

    async fn handle_tool_call_inner(
        &self,
        request_id: String,
        request: McpToolCallRequest,
        api_key_profile: &str,
        upstream_credential: Option<&str>,
    ) -> Result<McpToolCallResponse, McpProxyError> {
        validate_request_shape(&request_id, &request)?;
        guard_pilot_subset(&request_id, &request)?;

        if let Some(requested_profile) = request.metadata.profile.as_deref() {
            if requested_profile != api_key_profile {
                return Err(McpProxyError::unauthorized(
                    request_id,
                    "metadata.profile must match API key profile binding",
                ));
            }
        }

        let profile_id = resolve_profile_id(request.metadata.profile.as_deref(), api_key_profile).to_string();
        let resolved = resolve_tool_call(&self.config, &request_id, &request, &profile_id)?;

        validate_tool_arguments(
            &request_id,
            &request.server,
            &request.tool,
            &request.arguments,
            resolved.argument_policy.as_ref(),
        )?;

        let mut upstream_context = resolved.upstream.clone();
        upstream_context.upstream_bearer_token = upstream_credential.map(str::to_string);

        let mut result = self
            .upstream
            .execute_tool_call(&upstream_context, &request.arguments)
            .await?;

        let sanitization = self.sanitize_output(
            &request_id,
            &profile_id,
            &request.server,
            &request.tool,
            resolved.output_sanitization,
            &result,
        )?;

        // Output blocking is post-hoc by design: the tool call is already executed upstream,
        // but the response payload is withheld from the caller after policy evaluation.
        if sanitization.action == PolicyAction::Block {
            return Err(McpProxyError::tool_call_blocked(
                &request_id,
                &request.server,
                &request.tool,
                McpPolicyReason::OutputBlocked.as_str(),
                sanitization.rule_hits_total.max(1),
            ));
        }

        result.content = sanitization.safe_output;

        Ok(McpToolCallResponse {
            request_id,
            allowed: true,
            sanitized: sanitization.sanitized,
            result,
            pokrov: crate::types::McpResponseMetadata {
                profile: profile_id,
                action: sanitization.action,
                rule_hits: sanitization.rule_hits_total,
                server: request.server,
                tool: request.tool,
            },
        })
    }

    fn sanitize_output(
        &self,
        request_id: &str,
        profile_id: &str,
        server: &str,
        tool: &str,
        output_sanitization: bool,
        result: &McpToolResultEnvelope,
    ) -> Result<McpSanitizationResult, McpProxyError> {
        if !output_sanitization {
            return Ok(McpSanitizationResult {
                sanitized: false,
                action: PolicyAction::Allow,
                rule_hits_total: 0,
                safe_output: result.content.clone(),
            });
        }

        let Some(evaluator) = self.evaluator.as_ref() else {
            return Ok(McpSanitizationResult {
                sanitized: false,
                action: PolicyAction::Allow,
                rule_hits_total: 0,
                safe_output: result.content.clone(),
            });
        };

        let evaluation = evaluator
            .evaluate(EvaluateRequest {
                request_id: request_id.to_string(),
                profile_id: profile_id.to_string(),
                mode: EvaluationMode::Enforce,
                payload: result.content.clone(),
                path_class: PathClass::Mcp,
            })
            .map_err(|error| {
                McpProxyError::upstream_error(
                    request_id,
                    server,
                    tool,
                    format!("failed to evaluate MCP output policy: {error}"),
                )
            })?;

        self.metrics.on_rule_hits(evaluation.decision.rule_hits_total);
        self.metrics
            .on_payload_transformed(evaluation.transform.transformed_fields_count);
        if evaluation.transform.blocked {
            self.metrics.on_evaluation_blocked();
        }

        Ok(McpSanitizationResult {
            sanitized: evaluation.transform.transformed_fields_count > 0,
            action: evaluation.decision.final_action,
            rule_hits_total: evaluation.decision.rule_hits_total,
            safe_output: evaluation
                .transform
                .sanitized_payload
                .unwrap_or_else(|| result.content.clone()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_terminal_event(
        &self,
        request_id: &str,
        server: &str,
        tool: &str,
        profile_id: &str,
        final_action: PolicyAction,
        rule_hits_total: u32,
        blocked: bool,
        upstream_status: Option<u16>,
        duration_ms: u64,
        auth_mode: &'static str,
        credential_origin: &'static str,
    ) {
        McpAuditEvent {
            request_id: request_id.to_string(),
            server_id: server.to_string(),
            tool_id: tool.to_string(),
            profile_id: profile_id.to_string(),
            final_action: action_to_str(final_action),
            rule_hits_total,
            blocked,
            upstream_status,
            duration_ms,
            auth_mode,
            credential_origin,
        }
        .emit();

        if blocked {
            self.metrics.on_mcp_tool_call_blocked();
        }
        self.metrics.on_mcp_tool_call_duration_ms(duration_ms);
    }
}

fn resolve_profile_id<'a>(requested: Option<&'a str>, fallback: &'a str) -> &'a str {
    requested.unwrap_or(fallback)
}

fn validate_request_shape(request_id: &str, request: &McpToolCallRequest) -> Result<(), McpProxyError> {
    if request.server.trim().is_empty() {
        return Err(McpProxyError::invalid_request(
            request_id,
            "server must not be empty",
        ));
    }

    if request.tool.trim().is_empty() {
        return Err(McpProxyError::invalid_request(request_id, "tool must not be empty"));
    }

    if !request.arguments.is_object() {
        return Err(McpProxyError::invalid_request(
            request_id,
            "arguments must be a JSON object",
        ));
    }

    Ok(())
}

fn guard_pilot_subset(request_id: &str, request: &McpToolCallRequest) -> Result<(), McpProxyError> {
    if let Some(transport) = request.metadata.tags.get("transport") {
        if transport != "http_json" {
            return Err(McpProxyError::unsupported_variant(
                request_id,
                "only transport=http_json is supported by v1 MCP pilot subset",
            ));
        }
    }

    if let Some(variant) = request.metadata.tags.get("variant") {
        if variant != "tool_call" {
            return Err(McpProxyError::unsupported_variant(
                request_id,
                "only variant=tool_call is supported by v1 MCP pilot subset",
            ));
        }
    }

    Ok(())
}

fn action_to_str(action: PolicyAction) -> &'static str {
    match action {
        PolicyAction::Allow => "allow",
        PolicyAction::Mask => "mask",
        PolicyAction::Replace => "replace",
        PolicyAction::Redact => "redact",
        PolicyAction::Block => "block",
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use pokrov_config::model::{
        McpConfig, McpDefaultsConfig, McpServerDefinition, McpToolPolicy, ToolArgumentConstraints,
    };
    use pokrov_core::{
        types::{
            CategoryActions, CustomRule, DetectionCategory, EvaluationMode, EvaluatorConfig,
            PolicyAction, PolicyProfile,
        },
        SanitizationEngine,
    };
    use pokrov_metrics::hooks::NoopRuntimeMetricsHooks;

    use super::McpProxyHandler;
    use crate::types::McpToolResultEnvelope;

    fn handler_for_sanitization_tests() -> McpProxyHandler {
        let config = McpConfig {
            defaults: McpDefaultsConfig {
                profile_id: "strict".to_string(),
                upstream_timeout_ms: 1000,
                output_sanitization: true,
            },
            servers: vec![McpServerDefinition {
                id: "repo-tools".to_string(),
                endpoint: "http://127.0.0.1:0".to_string(),
                enabled: true,
                allowed_tools: vec!["read_file".to_string()],
                blocked_tools: Vec::new(),
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
        };

        let evaluator = SanitizationEngine::new(EvaluatorConfig {
            default_profile: "strict".to_string(),
            profiles: BTreeMap::from([(
                "strict".to_string(),
                PolicyProfile {
                    profile_id: "strict".to_string(),
                    mode_default: EvaluationMode::Enforce,
                    category_actions: CategoryActions {
                        secrets: PolicyAction::Redact,
                        pii: PolicyAction::Allow,
                        corporate_markers: PolicyAction::Allow,
                        custom: PolicyAction::Redact,
                    },
                    mask_visible_suffix: 4,
                    custom_rules_enabled: true,
                    custom_rules: vec![CustomRule {
                        rule_id: "custom.secret_token".to_string(),
                        category: DetectionCategory::Custom,
                        pattern: "(?i)sk-test-[a-z0-9]+".to_string(),
                        action: PolicyAction::Redact,
                        priority: 100,
                        replacement_template: None,
                        enabled: true,
                    }],
                },
            )]),
        })
        .expect("evaluator should build");

        McpProxyHandler::new(
            Some(Arc::new(evaluator)),
            Arc::new(NoopRuntimeMetricsHooks),
            config,
        )
        .expect("handler should build")
    }

    #[test]
    fn output_sanitization_mutates_only_string_leaves_and_keeps_json_shape() {
        let handler = handler_for_sanitization_tests();
        let envelope = McpToolResultEnvelope {
            content: serde_json::json!({
                "summary": "token sk-test-abcdef12",
                "meta": {
                    "count": 3,
                    "active": true,
                    "items": ["ok", "sk-test-zzzz9999", {"note": "safe"}]
                }
            }),
            content_type: None,
            truncated: false,
        };

        let result = handler
            .sanitize_output(
                "req-1",
                "strict",
                "repo-tools",
                "read_file",
                true,
                &envelope,
            )
            .expect("sanitization should succeed");

        let object = result.safe_output.as_object().expect("sanitized output must be object");
        let meta = object["meta"].as_object().expect("meta must remain object");
        let items = meta["items"].as_array().expect("items must remain array");

        assert!(result.sanitized);
        assert_eq!(meta["count"], serde_json::json!(3));
        assert_eq!(meta["active"], serde_json::json!(true));
        assert_eq!(items.len(), 3);

        let serialized = serde_json::to_string(&result.safe_output).expect("payload should serialize");
        assert!(!serialized.contains("sk-test-abcdef12"));
        assert!(!serialized.contains("sk-test-zzzz9999"));
    }
}
