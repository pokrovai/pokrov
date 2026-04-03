use std::{collections::HashSet, path::Path};

use regex::Regex;

use crate::{
    error::{ConfigError, ValidationIssue},
    model::{
        ApiKeyBinding, CategoryActionsConfig, CustomRuleConfig, LlmConfig, LlmProviderConfig,
        LlmRouteConfig, RuntimeConfig, SanitizationProfile, SecretRef,
    },
};

pub fn validate_runtime_config(config: &RuntimeConfig, path: &Path) -> Result<(), ConfigError> {
    let mut issues = Vec::new();

    if config.server.host.trim().is_empty() {
        issues.push(ValidationIssue::new("server.host", "must not be empty"));
    }

    if config.shutdown.drain_timeout_ms == 0 {
        issues.push(ValidationIssue::new("shutdown.drain_timeout_ms", "must be greater than zero"));
    }

    if config.shutdown.grace_period_ms == 0 {
        issues.push(ValidationIssue::new("shutdown.grace_period_ms", "must be greater than zero"));
    }

    if config.shutdown.grace_period_ms < config.shutdown.drain_timeout_ms {
        issues.push(ValidationIssue::new(
            "shutdown.grace_period_ms",
            "must be greater than or equal to drain_timeout_ms",
        ));
    }

    validate_api_key_bindings(&config.security.api_keys, &mut issues);
    validate_sanitization(config, &mut issues);
    validate_llm(config.llm.as_ref(), &mut issues);

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation { path: path.to_path_buf(), issues })
    }
}

fn validate_api_key_bindings(bindings: &[ApiKeyBinding], issues: &mut Vec<ValidationIssue>) {
    let mut unique_bindings = HashSet::new();
    for (idx, binding) in bindings.iter().enumerate() {
        if SecretRef::parse(&binding.key).is_none() {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}].key"),
                "must use env:VAR or file:/path format",
            ));
        }

        if !matches!(binding.profile.as_str(), "minimal" | "strict" | "custom") {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}].profile"),
                "must be one of minimal|strict|custom",
            ));
        }

        if !unique_bindings.insert((binding.key.clone(), binding.profile.clone())) {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}]"),
                "duplicate binding",
            ));
        }
    }
}

fn validate_sanitization(config: &RuntimeConfig, issues: &mut Vec<ValidationIssue>) {
    if !config.sanitization.enabled {
        return;
    }

    if !matches!(config.sanitization.default_profile.as_str(), "minimal" | "strict" | "custom") {
        issues.push(ValidationIssue::new(
            "sanitization.default_profile",
            "must be one of minimal|strict|custom",
        ));
    }

    validate_profile("minimal", &config.sanitization.profiles.minimal, issues);
    validate_profile("strict", &config.sanitization.profiles.strict, issues);
    validate_profile("custom", &config.sanitization.profiles.custom, issues);
}

fn validate_profile(profile_id: &str, profile: &SanitizationProfile, issues: &mut Vec<ValidationIssue>) {
    if profile.mask_visible_suffix > 8 {
        issues.push(ValidationIssue::new(
            format!("sanitization.profiles.{profile_id}.mask_visible_suffix"),
            "must be in range 0..=8",
        ));
    }

    validate_categories(profile_id, &profile.categories, issues);

    let mut ids = HashSet::new();
    for (idx, rule) in profile.custom_rules.iter().enumerate() {
        validate_custom_rule(profile_id, idx, profile.allow_empty_matches, rule, issues);

        if !ids.insert(rule.id.clone()) {
            issues.push(ValidationIssue::new(
                format!("sanitization.profiles.{profile_id}.custom_rules[{idx}].id"),
                "must be unique within profile",
            ));
        }
    }
}

fn validate_categories(profile_id: &str, categories: &CategoryActionsConfig, issues: &mut Vec<ValidationIssue>) {
    for (category, action) in [
        ("secrets", Some(categories.secrets)),
        ("pii", Some(categories.pii)),
        ("corporate_markers", Some(categories.corporate_markers)),
        ("custom", categories.custom),
    ] {
        let Some(action) = action else {
            continue;
        };

        if action == pokrov_core::types::PolicyAction::Replace {
            issues.push(ValidationIssue::new(
                format!("sanitization.profiles.{profile_id}.categories.{category}"),
                "replace action is allowed only for explicit custom rules",
            ));
        }
    }
}

fn validate_custom_rule(
    profile_id: &str,
    idx: usize,
    allow_empty_matches: bool,
    rule: &CustomRuleConfig,
    issues: &mut Vec<ValidationIssue>,
) {
    if rule.id.trim().is_empty() {
        issues.push(ValidationIssue::new(
            format!("sanitization.profiles.{profile_id}.custom_rules[{idx}].id"),
            "must not be empty",
        ));
    }

    if rule.pattern.trim().is_empty() {
        issues.push(ValidationIssue::new(
            format!("sanitization.profiles.{profile_id}.custom_rules[{idx}].pattern"),
            "must not be empty",
        ));
        return;
    }

    let Ok(matcher) = Regex::new(&rule.pattern) else {
        issues.push(ValidationIssue::new(
            format!("sanitization.profiles.{profile_id}.custom_rules[{idx}].pattern"),
            "must be a valid regular expression",
        ));
        return;
    };

    if !allow_empty_matches && matcher.is_match("") {
        issues.push(ValidationIssue::new(
            format!("sanitization.profiles.{profile_id}.custom_rules[{idx}].pattern"),
            "must not produce empty matches unless allow_empty_matches=true",
        ));
    }

    if rule.action == pokrov_core::types::PolicyAction::Replace && rule.replacement.is_none() {
        issues.push(ValidationIssue::new(
            format!("sanitization.profiles.{profile_id}.custom_rules[{idx}].replacement"),
            "is required when action=replace",
        ));
    }
}

fn validate_llm(config: Option<&LlmConfig>, issues: &mut Vec<ValidationIssue>) {
    let Some(config) = config else {
        return;
    };

    if config.providers.is_empty() {
        issues.push(ValidationIssue::new("llm.providers", "must contain at least one provider"));
    }

    if config.routes.is_empty() {
        issues.push(ValidationIssue::new("llm.routes", "must contain at least one route"));
    }

    if !matches!(config.defaults.profile_id.as_str(), "minimal" | "strict" | "custom") {
        issues.push(ValidationIssue::new(
            "llm.defaults.profile_id",
            "must be one of minimal|strict|custom",
        ));
    }

    let mut provider_ids = HashSet::new();
    let mut enabled_provider_ids = HashSet::new();

    for (idx, provider) in config.providers.iter().enumerate() {
        validate_llm_provider(idx, provider, issues);

        if !provider_ids.insert(provider.id.clone()) {
            issues.push(ValidationIssue::new(
                format!("llm.providers[{idx}].id"),
                "must be unique",
            ));
        }

        if provider.enabled {
            enabled_provider_ids.insert(provider.id.clone());
        }
    }

    let mut enabled_models = HashSet::new();
    for (idx, route) in config.routes.iter().enumerate() {
        validate_llm_route(idx, route, &enabled_provider_ids, issues);

        if route.enabled && !enabled_models.insert(route.model.clone()) {
            issues.push(ValidationIssue::new(
                format!("llm.routes[{idx}].model"),
                "must map to at most one enabled route",
            ));
        }
    }
}

fn validate_llm_provider(idx: usize, provider: &LlmProviderConfig, issues: &mut Vec<ValidationIssue>) {
    let provider_path = format!("llm.providers[{idx}]");

    if provider.id.len() < 2 || provider.id.len() > 64 {
        issues.push(ValidationIssue::new(
            format!("{provider_path}.id"),
            "length must be in range 2..=64",
        ));
    }

    if !provider
        .id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        issues.push(ValidationIssue::new(
            format!("{provider_path}.id"),
            "must match ^[a-zA-Z0-9_\\-]+$",
        ));
    }

    if !is_valid_provider_base_url(&provider.base_url) {
        issues.push(ValidationIssue::new(
            format!("{provider_path}.base_url"),
            "must be a valid http/https URL",
        ));
    }

    if SecretRef::parse(&provider.auth.api_key).is_none() {
        issues.push(ValidationIssue::new(
            format!("{provider_path}.auth.api_key"),
            "must use env:VAR or file:/path format",
        ));
    }

    if !(100..=120_000).contains(&provider.timeout_ms) {
        issues.push(ValidationIssue::new(
            format!("{provider_path}.timeout_ms"),
            "must be in range 100..=120000",
        ));
    }

    if provider.retry_budget > 3 {
        issues.push(ValidationIssue::new(
            format!("{provider_path}.retry_budget"),
            "must be in range 0..=3",
        ));
    }
}

fn validate_llm_route(
    idx: usize,
    route: &LlmRouteConfig,
    enabled_provider_ids: &HashSet<String>,
    issues: &mut Vec<ValidationIssue>,
) {
    let route_path = format!("llm.routes[{idx}]");

    if route.model.trim().is_empty() || route.model.len() > 128 {
        issues.push(ValidationIssue::new(
            format!("{route_path}.model"),
            "length must be in range 1..=128",
        ));
    }

    if route.provider_id.len() < 2 || route.provider_id.len() > 64 {
        issues.push(ValidationIssue::new(
            format!("{route_path}.provider_id"),
            "length must be in range 2..=64",
        ));
    }

    if route.enabled && !enabled_provider_ids.contains(&route.provider_id) {
        issues.push(ValidationIssue::new(
            format!("{route_path}.provider_id"),
            "must reference an existing enabled provider",
        ));
    }
}

fn is_valid_provider_base_url(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_http_scheme = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    if !has_http_scheme {
        return false;
    }

    if let Some(rest) = trimmed.split("://").nth(1) {
        return !rest.trim().is_empty();
    }

    false
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pokrov_core::types::{DetectionCategory, EvaluationMode, PolicyAction};

    use super::validate_runtime_config;
    use crate::model::{
        ApiKeyBinding, CategoryActionsConfig, CustomRuleConfig, LlmConfig, LlmDefaultsConfig,
        LlmProviderAuthConfig, LlmProviderConfig, LlmRouteConfig, LogFormat, LogLevel,
        LoggingConfig, RuntimeConfig, SanitizationConfig, SanitizationProfile, SanitizationProfiles,
        SecurityConfig, ServerConfig, ShutdownConfig,
    };

    fn valid_config() -> RuntimeConfig {
        RuntimeConfig {
            server: ServerConfig { host: "127.0.0.1".to_string(), port: 8080 },
            logging: LoggingConfig {
                level: LogLevel::Info,
                format: LogFormat::Json,
                component: "runtime".to_string(),
            },
            shutdown: ShutdownConfig { drain_timeout_ms: 5000, grace_period_ms: 10000 },
            security: SecurityConfig {
                fail_on_unresolved_api_keys: false,
                api_keys: vec![ApiKeyBinding {
                    key: "env:POKROV_API_KEY".to_string(),
                    profile: "strict".to_string(),
                }],
            },
            sanitization: SanitizationConfig {
                enabled: true,
                default_profile: "strict".to_string(),
                profiles: SanitizationProfiles {
                    minimal: SanitizationProfile {
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
                    },
                    strict: SanitizationProfile {
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
                    },
                    custom: SanitizationProfile {
                        mode_default: EvaluationMode::DryRun,
                        categories: CategoryActionsConfig {
                            secrets: PolicyAction::Redact,
                            pii: PolicyAction::Mask,
                            corporate_markers: PolicyAction::Mask,
                            custom: None,
                        },
                        mask_visible_suffix: 4,
                        custom_rules: vec![CustomRuleConfig {
                            id: "custom.pattern".to_string(),
                            category: DetectionCategory::Custom,
                            pattern: "(?i)project\\s+andromeda".to_string(),
                            action: PolicyAction::Redact,
                            priority: 100,
                            replacement: None,
                            enabled: true,
                        }],
                        allow_empty_matches: false,
                    },
                },
            },
            policies: None,
            llm: None,
            mcp: None,
        }
    }

    fn valid_llm_config() -> LlmConfig {
        LlmConfig {
            providers: vec![LlmProviderConfig {
                id: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                auth: LlmProviderAuthConfig {
                    api_key: "env:OPENAI_API_KEY".to_string(),
                },
                timeout_ms: 30_000,
                retry_budget: 1,
                enabled: true,
            }],
            routes: vec![LlmRouteConfig {
                model: "gpt-4o-mini".to_string(),
                provider_id: "openai".to_string(),
                output_sanitization: Some(true),
                enabled: true,
            }],
            defaults: LlmDefaultsConfig {
                profile_id: "strict".to_string(),
                output_sanitization: true,
            },
        }
    }

    #[test]
    fn accepts_valid_runtime_config() {
        let config = valid_config();
        validate_runtime_config(&config, Path::new("config.yaml")).expect("config should be valid");
    }

    #[test]
    fn rejects_plaintext_secret() {
        let mut config = valid_config();
        config.security.api_keys[0].key = "plaintext".to_string();
        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("config should fail validation");
        assert!(error.to_string().contains("must use env:VAR or file:/path format"));
    }

    #[test]
    fn rejects_invalid_shutdown_budget() {
        let mut config = valid_config();
        config.shutdown.drain_timeout_ms = 5000;
        config.shutdown.grace_period_ms = 1000;
        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("config should fail validation");
        assert!(error.to_string().contains("must be greater than or equal to drain_timeout_ms"));
    }

    #[test]
    fn rejects_duplicate_api_bindings() {
        let mut config = valid_config();
        let duplicate = config.security.api_keys[0].clone();
        config.security.api_keys.push(duplicate);
        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("config should fail validation");
        assert!(error.to_string().contains("duplicate binding"));
    }

    #[test]
    fn rejects_unknown_api_key_profile() {
        let mut config = valid_config();
        config.security.api_keys[0].profile = "unknown".to_string();
        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("config should fail validation");
        assert!(error.to_string().contains("must be one of minimal|strict|custom"));
    }

    #[test]
    fn rejects_replace_custom_rule_without_replacement_template() {
        let mut config = valid_config();
        config.sanitization.profiles.custom.custom_rules[0].action = PolicyAction::Replace;
        config.sanitization.profiles.custom.custom_rules[0].replacement = None;

        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("config should fail validation");

        assert!(error.to_string().contains("is required when action=replace"));
    }

    #[test]
    fn accepts_valid_llm_configuration() {
        let mut config = valid_config();
        config.llm = Some(valid_llm_config());

        validate_runtime_config(&config, Path::new("config.yaml"))
            .expect("llm configuration should be valid");
    }

    #[test]
    fn rejects_route_bound_to_disabled_provider() {
        let mut config = valid_config();
        let mut llm = valid_llm_config();
        llm.providers[0].enabled = false;
        config.llm = Some(llm);

        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("llm configuration should fail validation");

        assert!(error.to_string().contains("must reference an existing enabled provider"));
    }

    #[test]
    fn rejects_duplicate_enabled_model_routes() {
        let mut config = valid_config();
        let mut llm = valid_llm_config();
        llm.routes.push(LlmRouteConfig {
            model: "gpt-4o-mini".to_string(),
            provider_id: "openai".to_string(),
            output_sanitization: Some(false),
            enabled: true,
        });
        config.llm = Some(llm);

        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("duplicate enabled routes must fail validation");

        assert!(error.to_string().contains("must map to at most one enabled route"));
    }
}
