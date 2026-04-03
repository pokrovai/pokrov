use std::collections::BTreeMap;

use pokrov_core::types::PolicyAction;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct McpToolCallRequest {
    pub server: String,
    pub tool: String,
    pub arguments: Value,
    #[serde(default)]
    pub metadata: McpRequestMetadata,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct McpRequestMetadata {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, flatten)]
    pub tags: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolCallResponse {
    pub request_id: String,
    pub allowed: bool,
    pub sanitized: bool,
    pub result: McpToolResultEnvelope,
    pub pokrov: McpResponseMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolResultEnvelope {
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpResponseMetadata {
    pub profile: String,
    pub action: PolicyAction,
    pub rule_hits: u32,
    pub server: String,
    pub tool: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPolicyReason {
    ServerNotAllowlisted,
    ToolNotAllowlisted,
    ToolBlocklisted,
    ArgumentInvalid,
    Allowed,
}

impl McpPolicyReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ServerNotAllowlisted => "server_not_allowlisted",
            Self::ToolNotAllowlisted => "tool_not_allowlisted",
            Self::ToolBlocklisted => "tool_blocklisted",
            Self::ArgumentInvalid => "argument_invalid",
            Self::Allowed => "allowed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolPolicyDecision {
    pub profile_id: String,
    pub allowed: bool,
    pub final_action: PolicyAction,
    pub reason: McpPolicyReason,
    pub rule_hits_total: u32,
}

#[derive(Debug, Clone)]
pub struct McpUpstreamRequestContext {
    pub request_id: String,
    pub server_id: String,
    pub tool_id: String,
    pub endpoint: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStage {
    Schema,
    Constraints,
}

#[derive(Debug, Clone)]
pub struct ToolValidationResult {
    pub valid: bool,
    pub stage: ValidationStage,
    pub violations: Vec<ValidationViolation>,
}

#[derive(Debug, Clone)]
pub struct ValidationViolation {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct McpErrorDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_count: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct McpSanitizationResult {
    pub sanitized: bool,
    pub action: PolicyAction,
    pub rule_hits_total: u32,
    pub safe_output: Value,
}
