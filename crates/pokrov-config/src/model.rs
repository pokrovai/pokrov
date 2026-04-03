use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub shutdown: ShutdownConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub policies: Option<BTreeMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub llm: Option<BTreeMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub mcp: Option<BTreeMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShutdownConfig {
    pub drain_timeout_ms: u64,
    pub grace_period_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SecurityConfig {
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

fn default_component() -> String {
    "runtime".to_string()
}
