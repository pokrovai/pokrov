use std::{collections::HashSet, path::Path};

use regex::Regex;

use crate::{
    error::{ConfigError, ValidationIssue},
    model::{
        ApiKeyBinding, CategoryActionsConfig, CustomRuleConfig, LlmConfig, LlmProviderConfig,
        LlmRouteConfig, McpConfig, McpServerDefinition, RuntimeConfig, SanitizationProfile,
        SecretRef, ToolArgumentConstraints,
    },
    rate_limit::RateLimitConfig,
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
    validate_mcp(config.mcp.as_ref(), &mut issues);
    validate_rate_limit(&config.rate_limit, &mut issues);
    validate_identity(config, &mut issues);

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation { path: path.to_path_buf(), issues })
    }
}

fn validate_identity(config: &RuntimeConfig, issues: &mut Vec<ValidationIssue>) {
    if config.identity.resolution_order.is_empty() {
        issues.push(ValidationIssue::new(
            "identity.resolution_order",
            "must contain at least one identity source",
        ));
    }

    if let Some(fallback) = config.identity.fallback_policy_profile.as_ref() {
        if !matches!(fallback.as_str(), "minimal" | "strict" | "custom") {
            issues.push(ValidationIssue::new(
                "identity.fallback_policy_profile",
                "must be one of minimal|strict|custom",
            ));
        }
    }

    for (identity, profile) in &config.identity.profile_bindings {
        if identity.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "identity.profile_bindings",
                "must not contain empty identity keys",
            ));
        }

        if !matches!(profile.as_str(), "minimal" | "strict" | "custom") {
            issues.push(ValidationIssue::new(
                format!("identity.profile_bindings.{identity}"),
                "must be one of minimal|strict|custom",
            ));
        }
    }

    for (identity, profile) in &config.identity.rate_limit_bindings {
        if identity.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "identity.rate_limit_bindings",
                "must not contain empty identity keys",
            ));
        }

        if !config.rate_limit.profiles.contains_key(profile) {
            issues.push(ValidationIssue::new(
                format!("identity.rate_limit_bindings.{identity}"),
                "must reference an existing rate-limit profile",
            ));
        }
    }
}

fn validate_rate_limit(config: &RateLimitConfig, issues: &mut Vec<ValidationIssue>) {
    if !config.enabled {
        return;
    }

    if config.profiles.is_empty() {
        issues.push(ValidationIssue::new(
            "rate_limit.profiles",
            "must contain at least one profile when rate limiting is enabled",
        ));
        return;
    }

    if !config.profiles.contains_key(&config.default_profile) {
        issues.push(ValidationIssue::new(
            "rate_limit.default_profile",
            "must reference an existing rate-limit profile",
        ));
    }

    for (profile_id, profile) in &config.profiles {
        let base_path = format!("rate_limit.profiles.{profile_id}");

        if profile.requests_per_minute == 0 {
            issues.push(ValidationIssue::new(
                format!("{base_path}.requests_per_minute"),
                "must be greater than zero",
            ));
        }

        if profile.token_units_per_minute == 0 {
            issues.push(ValidationIssue::new(
                format!("{base_path}.token_units_per_minute"),
                "must be greater than zero",
            ));
        }

        if !(1.0..=5.0).contains(&profile.burst_multiplier) {
            issues.push(ValidationIssue::new(
                format!("{base_path}.burst_multiplier"),
                "must be in range 1.0..=5.0",
            ));
        }
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

    if !(1_024..=16 * 1024 * 1024).contains(&config.defaults.stream_sanitization_max_buffer_bytes) {
        issues.push(ValidationIssue::new(
            "llm.defaults.stream_sanitization_max_buffer_bytes",
            "must be in range 1024..=16777216",
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

fn validate_mcp(config: Option<&McpConfig>, issues: &mut Vec<ValidationIssue>) {
    let Some(config) = config else {
        return;
    };

    if config.servers.is_empty() {
        issues.push(ValidationIssue::new(
            "mcp.servers",
            "must contain at least one server",
        ));
    }

    if !matches!(config.defaults.profile_id.as_str(), "minimal" | "strict" | "custom") {
        issues.push(ValidationIssue::new(
            "mcp.defaults.profile_id",
            "must be one of minimal|strict|custom",
        ));
    }

    if !(100..=120_000).contains(&config.defaults.upstream_timeout_ms) {
        issues.push(ValidationIssue::new(
            "mcp.defaults.upstream_timeout_ms",
            "must be in range 100..=120000",
        ));
    }

    let mut server_ids = HashSet::new();
    let mut enabled_endpoints = HashSet::new();
    for (idx, server) in config.servers.iter().enumerate() {
        let server_path = format!("mcp.servers[{idx}]");
        validate_mcp_server(idx, server, issues);

        if !server_ids.insert(server.id.clone()) {
            issues.push(ValidationIssue::new(
                format!("{server_path}.id"),
                "must be unique",
            ));
        }

        if server.enabled && !enabled_endpoints.insert(server.endpoint.clone()) {
            issues.push(ValidationIssue::new(
                format!("{server_path}.endpoint"),
                "must be unique for enabled servers",
            ));
        }
    }
}

fn validate_mcp_server(idx: usize, server: &McpServerDefinition, issues: &mut Vec<ValidationIssue>) {
    let server_path = format!("mcp.servers[{idx}]");

    if server.id.len() < 2 || server.id.len() > 64 {
        issues.push(ValidationIssue::new(
            format!("{server_path}.id"),
            "length must be in range 2..=64",
        ));
    }

    if !server
        .id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        issues.push(ValidationIssue::new(
            format!("{server_path}.id"),
            "must match ^[a-zA-Z0-9_\\-]+$",
        ));
    }

    if !is_valid_provider_base_url(&server.endpoint) {
        issues.push(ValidationIssue::new(
            format!("{server_path}.endpoint"),
            "must be a valid http/https URL",
        ));
    }

    let mut allowed_tools = HashSet::new();
    for (tool_idx, tool) in server.allowed_tools.iter().enumerate() {
        if tool.trim().is_empty() {
            issues.push(ValidationIssue::new(
                format!("{server_path}.allowed_tools[{tool_idx}]"),
                "must not be empty",
            ));
        }
        if !allowed_tools.insert(tool.clone()) {
            issues.push(ValidationIssue::new(
                format!("{server_path}.allowed_tools[{tool_idx}]"),
                "duplicate tool id",
            ));
        }
    }

    if server.enabled && server.allowed_tools.is_empty() {
        issues.push(ValidationIssue::new(
            format!("{server_path}.allowed_tools"),
            "must contain at least one tool id when server is enabled",
        ));
    }

    let mut blocked_tools = HashSet::new();
    for (tool_idx, tool) in server.blocked_tools.iter().enumerate() {
        if tool.trim().is_empty() {
            issues.push(ValidationIssue::new(
                format!("{server_path}.blocked_tools[{tool_idx}]"),
                "must not be empty",
            ));
        }
        if !blocked_tools.insert(tool.clone()) {
            issues.push(ValidationIssue::new(
                format!("{server_path}.blocked_tools[{tool_idx}]"),
                "duplicate tool id",
            ));
        }

        if allowed_tools.contains(tool) {
            issues.push(ValidationIssue::new(
                format!("{server_path}.blocked_tools[{tool_idx}]"),
                "blocked tool cannot be listed in allowed_tools",
            ));
        }
    }

    for (tool_name, policy) in &server.tools {
        if !server.allowed_tools.iter().any(|name| name == tool_name) {
            issues.push(ValidationIssue::new(
                format!("{server_path}.tools.{tool_name}"),
                "tool policy key must reference an allowlisted tool",
            ));
        }

        if let Some(schema) = policy.argument_schema.as_ref() {
            if !schema.is_object() {
                issues.push(ValidationIssue::new(
                    format!("{server_path}.tools.{tool_name}.argument_schema"),
                    "must be a JSON object schema fragment",
                ));
            }
        }

        validate_mcp_constraints(
            &format!("{server_path}.tools.{tool_name}.argument_constraints"),
            &policy.argument_constraints,
            issues,
        );
    }
}

fn validate_mcp_constraints(
    path: &str,
    constraints: &ToolArgumentConstraints,
    issues: &mut Vec<ValidationIssue>,
) {
    if let Some(max_depth) = constraints.max_depth {
        if max_depth == 0 || max_depth > 16 {
            issues.push(ValidationIssue::new(
                format!("{path}.max_depth"),
                "must be in range 1..=16",
            ));
        }
    }

    if let Some(max_string_length) = constraints.max_string_length {
        if max_string_length == 0 || max_string_length > 16_384 {
            issues.push(ValidationIssue::new(
                format!("{path}.max_string_length"),
                "must be in range 1..=16384",
            ));
        }
    }

    let required: HashSet<&str> =
        constraints.required_keys.iter().map(String::as_str).collect();
    for (idx, key) in constraints.forbidden_keys.iter().enumerate() {
        if required.contains(key.as_str()) {
            issues.push(ValidationIssue::new(
                format!("{path}.forbidden_keys[{idx}]"),
                "must not overlap with required_keys",
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
#[path = "validate_tests.rs"]
mod tests;
