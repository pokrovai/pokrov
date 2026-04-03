use std::collections::BTreeMap;

use pokrov_core::types::{
    CategoryActions, CustomRule, EvaluationMode, EvaluatorConfig, PolicyAction, PolicyProfile,
};
use serde::{Deserialize, Serialize};

use crate::rate_limit::RateLimitConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub shutdown: ShutdownConfig,
    #[serde(default)]
    pub security: SecurityConfig,
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
}

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
    pub auth: LlmProviderAuthConfig,
    #[serde(default = "default_llm_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_llm_retry_budget")]
    pub retry_budget: u8,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmProviderAuthConfig {
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmRouteConfig {
    pub model: String,
    pub provider_id: String,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_profile_id")]
    pub default_profile: String,
    #[serde(default)]
    pub profiles: SanitizationProfiles,
}

impl Default for SanitizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_profile: default_profile_id(),
            profiles: SanitizationProfiles::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizationProfiles {
    #[serde(default = "default_minimal_profile")]
    pub minimal: SanitizationProfile,
    #[serde(default = "default_strict_profile")]
    pub strict: SanitizationProfile,
    #[serde(default = "default_custom_profile")]
    pub custom: SanitizationProfile,
}

impl Default for SanitizationProfiles {
    fn default() -> Self {
        Self {
            minimal: default_minimal_profile(),
            strict: default_strict_profile(),
            custom: default_custom_profile(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizationProfile {
    #[serde(default = "default_mode")]
    pub mode_default: EvaluationMode,
    pub categories: CategoryActionsConfig,
    #[serde(default = "default_mask_visible_suffix")]
    pub mask_visible_suffix: u8,
    #[serde(default)]
    pub custom_rules: Vec<CustomRuleConfig>,
    #[serde(default)]
    pub allow_empty_matches: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CategoryActionsConfig {
    pub secrets: PolicyAction,
    pub pii: PolicyAction,
    pub corporate_markers: PolicyAction,
    #[serde(default)]
    pub custom: Option<PolicyAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomRuleConfig {
    pub id: String,
    pub category: pokrov_core::types::DetectionCategory,
    pub pattern: String,
    pub action: PolicyAction,
    #[serde(default = "default_rule_priority")]
    pub priority: u16,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl RuntimeConfig {
    pub fn evaluator_config(&self) -> EvaluatorConfig {
        EvaluatorConfig {
            default_profile: self.sanitization.default_profile.clone(),
            profiles: BTreeMap::from([
                (
                    "minimal".to_string(),
                    to_policy_profile("minimal", &self.sanitization.profiles.minimal),
                ),
                (
                    "strict".to_string(),
                    to_policy_profile("strict", &self.sanitization.profiles.strict),
                ),
                (
                    "custom".to_string(),
                    to_policy_profile("custom", &self.sanitization.profiles.custom),
                ),
            ]),
        }
    }
}

fn to_policy_profile(profile_id: &str, profile: &SanitizationProfile) -> PolicyProfile {
    PolicyProfile {
        profile_id: profile_id.to_string(),
        mode_default: profile.mode_default,
        category_actions: CategoryActions {
            secrets: profile.categories.secrets,
            pii: profile.categories.pii,
            corporate_markers: profile.categories.corporate_markers,
            custom: profile.categories.custom.unwrap_or(profile.categories.corporate_markers),
        },
        mask_visible_suffix: profile.mask_visible_suffix,
        custom_rules_enabled: true,
        custom_rules: profile
            .custom_rules
            .iter()
            .map(|rule| CustomRule {
                rule_id: rule.id.clone(),
                category: rule.category,
                pattern: rule.pattern.clone(),
                action: rule.action,
                priority: rule.priority,
                replacement_template: rule.replacement.clone(),
                enabled: rule.enabled,
            })
            .collect(),
    }
}

fn default_component() -> String {
    "runtime".to_string()
}

fn default_true() -> bool {
    true
}

fn default_profile_id() -> String {
    "strict".to_string()
}

fn default_mode() -> EvaluationMode {
    EvaluationMode::Enforce
}

fn default_mask_visible_suffix() -> u8 {
    4
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

fn default_mcp_upstream_timeout_ms() -> u64 {
    10_000
}

fn default_rule_priority() -> u16 {
    100
}

fn default_minimal_profile() -> SanitizationProfile {
    SanitizationProfile {
        mode_default: EvaluationMode::Enforce,
        categories: CategoryActionsConfig {
            secrets: PolicyAction::Mask,
            pii: PolicyAction::Allow,
            corporate_markers: PolicyAction::Allow,
            custom: None,
        },
        mask_visible_suffix: 4,
        custom_rules: Vec::new(),
        allow_empty_matches: false,
    }
}

fn default_strict_profile() -> SanitizationProfile {
    SanitizationProfile {
        mode_default: EvaluationMode::Enforce,
        categories: CategoryActionsConfig {
            secrets: PolicyAction::Block,
            pii: PolicyAction::Redact,
            corporate_markers: PolicyAction::Mask,
            custom: None,
        },
        mask_visible_suffix: 4,
        custom_rules: Vec::new(),
        allow_empty_matches: false,
    }
}

fn default_custom_profile() -> SanitizationProfile {
    SanitizationProfile {
        mode_default: EvaluationMode::DryRun,
        categories: CategoryActionsConfig {
            secrets: PolicyAction::Redact,
            pii: PolicyAction::Mask,
            corporate_markers: PolicyAction::Mask,
            custom: None,
        },
        mask_visible_suffix: 4,
        custom_rules: Vec::new(),
        allow_empty_matches: false,
    }
}

#[cfg(test)]
mod tests {
    use pokrov_core::types::PolicyAction;

    use super::RuntimeConfig;

    #[test]
    fn evaluator_config_uses_explicit_custom_action_when_present_in_yaml() {
        let raw = r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 1000
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      mode_default: enforce
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
      mask_visible_suffix: 4
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
        custom: block
      mask_visible_suffix: 4
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
"#;

        let config: RuntimeConfig =
            serde_yaml::from_str(raw).expect("runtime config with custom category must parse");
        let evaluator = config.evaluator_config();

        let strict = evaluator
            .profiles
            .get("strict")
            .expect("strict profile must exist in evaluator config");
        assert_eq!(strict.category_actions.custom, PolicyAction::Block);
    }

    #[test]
    fn evaluator_config_falls_back_to_corporate_markers_for_custom_action_when_omitted() {
        let raw = r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 1000
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      mode_default: enforce
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
      mask_visible_suffix: 4
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
"#;

        let config: RuntimeConfig =
            serde_yaml::from_str(raw).expect("runtime config without explicit custom category must parse");
        let evaluator = config.evaluator_config();

        let strict = evaluator
            .profiles
            .get("strict")
            .expect("strict profile must exist in evaluator config");
        assert_eq!(strict.category_actions.custom, strict.category_actions.corporate_markers);
    }
}
