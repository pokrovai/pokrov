use std::{collections::HashSet, path::Path};

use regex::Regex;

use crate::{
    error::{ConfigError, ValidationIssue},
    model::{
        ApiKeyBinding, CategoryActionsConfig, CustomRuleConfig, RuntimeConfig, SanitizationProfile,
        SecretRef,
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
    let fields = [
        ("secrets", categories.secrets),
        ("pii", categories.pii),
        ("corporate_markers", categories.corporate_markers),
    ];

    for (category, action) in fields {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pokrov_core::types::{DetectionCategory, EvaluationMode, PolicyAction};

    use super::validate_runtime_config;
    use crate::model::{
        ApiKeyBinding, CategoryActionsConfig, CustomRuleConfig, LogFormat, LogLevel, LoggingConfig,
        RuntimeConfig, SanitizationConfig, SanitizationProfile, SanitizationProfiles, SecurityConfig,
        ServerConfig, ShutdownConfig,
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
}
