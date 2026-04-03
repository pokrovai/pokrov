use std::{collections::HashSet, path::Path};

use crate::{
    error::{ConfigError, ValidationIssue},
    model::{RuntimeConfig, SecretRef},
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

    let mut unique_bindings = HashSet::new();
    for (idx, binding) in config.security.api_keys.iter().enumerate() {
        if SecretRef::parse(&binding.key).is_none() {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}].key"),
                "must use env:VAR or file:/path format",
            ));
        }

        if binding.profile.trim().is_empty() {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}].profile"),
                "must not be empty",
            ));
        } else if !is_valid_profile_slug(&binding.profile) {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}].profile"),
                "must match slug pattern ^[a-z0-9][a-z0-9_-]*$",
            ));
        }

        if !unique_bindings.insert((binding.key.clone(), binding.profile.clone())) {
            issues.push(ValidationIssue::new(
                format!("security.api_keys[{idx}]"),
                "duplicate binding",
            ));
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation { path: path.to_path_buf(), issues })
    }
}

fn is_valid_profile_slug(profile: &str) -> bool {
    let mut chars = profile.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }

    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::validate_runtime_config;
    use crate::model::{
        ApiKeyBinding, LogFormat, LogLevel, LoggingConfig, RuntimeConfig, SecurityConfig,
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
    fn rejects_non_slug_api_key_profile() {
        let mut config = valid_config();
        config.security.api_keys[0].profile = "Strict Profile".to_string();
        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("config should fail validation");
        assert!(
            error
                .to_string()
                .contains("must match slug pattern ^[a-z0-9][a-z0-9_-]*$")
        );
    }
}
