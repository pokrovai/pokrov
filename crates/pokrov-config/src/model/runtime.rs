use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::rate_limit::RateLimitConfig;

#[cfg(feature = "ner")]
use super::NerConfig;
use super::{AuthConfig, IdentityConfig, LlmConfig, McpConfig, SanitizationConfig};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    pub shutdown: ShutdownConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub sanitization: SanitizationConfig,
    #[serde(default)]
    pub policies: Option<BTreeMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub llm: Option<LlmConfig>,
    #[serde(default)]
    pub mcp: Option<McpConfig>,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub response_envelope: ResponseEnvelopeConfig,
    #[cfg(feature = "ner")]
    #[serde(default)]
    pub ner: Option<NerConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub tls: TlsServerConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TlsServerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cert_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    #[serde(default)]
    pub client_ca_file: Option<String>,
    #[serde(default)]
    pub require_client_cert: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    #[serde(default = "default_component")]
    pub component: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub llm_payload_trace: LlmPayloadTraceConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmPayloadTraceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_llm_payload_trace_output_path")]
    pub output_path: String,
}

impl Default for LlmPayloadTraceConfig {
    fn default() -> Self {
        Self { enabled: false, output_path: default_llm_payload_trace_output_path() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShutdownConfig {
    pub drain_timeout_ms: u64,
    pub grace_period_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub fail_on_unresolved_api_keys: bool,
    #[serde(default)]
    pub fail_on_unresolved_provider_keys: bool,
    #[serde(default)]
    pub api_keys: Vec<ApiKeyBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ApiKeyBinding {
    pub key: String,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretRef {
    Env(String),
    File(String),
}

impl SecretRef {
    pub fn parse(raw: &str) -> Option<Self> {
        if let Some(name) = raw.strip_prefix("env:") {
            let trimmed = name.trim();
            return (!trimmed.is_empty()).then(|| Self::Env(trimmed.to_string()));
        }

        if let Some(path) = raw.strip_prefix("file:") {
            let trimmed = path.trim();
            return (!trimmed.is_empty()).then(|| Self::File(trimmed.to_string()));
        }

        None
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResponseEnvelopeConfig {
    #[serde(default)]
    pub pokrov_metadata: ResponseEnvelopeMetadataConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseEnvelopeMetadataConfig {
    #[serde(default)]
    pub mode: ResponseMetadataMode,
}

impl Default for ResponseEnvelopeMetadataConfig {
    fn default() -> Self {
        Self { mode: ResponseMetadataMode::Enabled }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseMetadataMode {
    #[default]
    Enabled,
    Suppressed,
}

fn default_component() -> String {
    "runtime".to_string()
}

fn default_llm_payload_trace_output_path() -> String {
    "/tmp/pokrov-llm-payload-trace.ndjson".to_string()
}
