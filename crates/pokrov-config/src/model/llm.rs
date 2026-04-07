use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    pub providers: Vec<LlmProviderConfig>,
    pub routes: Vec<LlmRouteConfig>,
    pub defaults: LlmDefaultsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmProviderConfig {
    pub id: String,
    pub base_url: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub upstream_path: Option<String>,
    #[serde(default)]
    pub auth: LlmProviderAuthConfig,
    #[serde(default = "default_llm_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_llm_retry_budget")]
    pub retry_budget: u8,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmProviderAuthConfig {
    #[serde(default)]
    pub api_key: String,
}

impl Default for LlmProviderAuthConfig {
    fn default() -> Self {
        Self { api_key: String::new() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmRouteConfig {
    pub model: String,
    pub provider_id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub output_sanitization: Option<bool>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmDefaultsConfig {
    pub profile_id: String,
    pub output_sanitization: bool,
    #[serde(default = "default_stream_sanitization_max_buffer_bytes")]
    pub stream_sanitization_max_buffer_bytes: usize,
}

fn default_true() -> bool {
    true
}

fn default_llm_timeout_ms() -> u64 {
    30_000
}

fn default_llm_retry_budget() -> u8 {
    0
}

fn default_stream_sanitization_max_buffer_bytes() -> usize {
    1024 * 1024
}
