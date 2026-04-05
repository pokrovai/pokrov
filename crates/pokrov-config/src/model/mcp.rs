use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    pub defaults: McpDefaultsConfig,
    pub servers: Vec<McpServerDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpDefaultsConfig {
    pub profile_id: String,
    #[serde(default = "default_mcp_upstream_timeout_ms")]
    pub upstream_timeout_ms: u64,
    #[serde(default = "default_true")]
    pub output_sanitization: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerDefinition {
    pub id: String,
    pub endpoint: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub blocked_tools: Vec<String>,
    #[serde(default)]
    pub tools: BTreeMap<String, McpToolPolicy>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolPolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub argument_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub argument_constraints: ToolArgumentConstraints,
    #[serde(default)]
    pub output_sanitization: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ToolArgumentConstraints {
    #[serde(default)]
    pub max_depth: Option<u8>,
    #[serde(default)]
    pub max_string_length: Option<usize>,
    #[serde(default)]
    pub required_keys: Vec<String>,
    #[serde(default)]
    pub forbidden_keys: Vec<String>,
    #[serde(default)]
    pub allowed_path_prefixes: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_mcp_upstream_timeout_ms() -> u64 {
    10_000
}
