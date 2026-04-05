use std::path::Path;

use pokrov_core::types::{DetectionCategory, EvaluationMode, PolicyAction};

use super::validate_runtime_config;
use crate::model::{
    ApiKeyBinding, AuthConfig, CategoryActionsConfig, CustomRuleConfig, DeterministicPatternConfig,
    DeterministicRecognizerConfig, IdentityConfig, IdentitySource, LlmConfig, LlmDefaultsConfig,
    LlmProviderAuthConfig, LlmProviderConfig, LlmRouteConfig, LogFormat, LogLevel, LoggingConfig,
    McpConfig, McpDefaultsConfig, McpServerDefinition, McpToolPolicy, RuntimeConfig,
    SanitizationConfig, SanitizationProfile, SanitizationProfiles, SecurityConfig, ServerConfig,
    ShutdownConfig, TlsServerConfig, ToolArgumentConstraints, UpstreamAuthMode,
};

fn valid_config() -> RuntimeConfig {
    RuntimeConfig {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            tls: TlsServerConfig::default(),
        },
        logging: LoggingConfig {
            level: LogLevel::Info,
            format: LogFormat::Json,
            component: "runtime".to_string(),
        },
        shutdown: ShutdownConfig { drain_timeout_ms: 5000, grace_period_ms: 10000 },
        security: SecurityConfig {
            fail_on_unresolved_api_keys: false,
            fail_on_unresolved_provider_keys: false,
            api_keys: vec![ApiKeyBinding {
                key: "env:POKROV_API_KEY".to_string(),
                profile: "strict".to_string(),
            }],
        },
        auth: AuthConfig { upstream_auth_mode: UpstreamAuthMode::Static, ..AuthConfig::default() },
        identity: IdentityConfig {
            resolution_order: vec![
                IdentitySource::GatewayAuthSubject,
                IdentitySource::XPokrovClientId,
                IdentitySource::IngressIdentity,
            ],
            profile_bindings: std::collections::BTreeMap::new(),
            rate_limit_bindings: std::collections::BTreeMap::new(),
            required_for_policy: false,
            required_for_rate_limit: false,
            fallback_policy_profile: Some("strict".to_string()),
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
                    deterministic_recognizers: Vec::new(),
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
                    deterministic_recognizers: Vec::new(),
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
                    deterministic_recognizers: Vec::new(),
                    allow_empty_matches: false,
                },
            },
        },
        policies: None,
        llm: None,
        mcp: None,
        rate_limit: crate::rate_limit::RateLimitConfig::default(),
        response_envelope: crate::model::ResponseEnvelopeConfig::default(),
    }
}

fn valid_llm_config() -> LlmConfig {
    LlmConfig {
        providers: vec![LlmProviderConfig {
            id: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            upstream_path: Some("/chat/completions".to_string()),
            auth: LlmProviderAuthConfig { api_key: "env:OPENAI_API_KEY".to_string() },
            timeout_ms: 30_000,
            retry_budget: 1,
            enabled: true,
        }],
        routes: vec![LlmRouteConfig {
            model: "gpt-4o-mini".to_string(),
            provider_id: "openai".to_string(),
            aliases: vec!["openai/gpt-4o-mini".to_string()],
            output_sanitization: Some(true),
            enabled: true,
        }],
        defaults: LlmDefaultsConfig {
            profile_id: "strict".to_string(),
            output_sanitization: true,
            stream_sanitization_max_buffer_bytes: 1024 * 1024,
        },
    }
}

fn valid_mcp_config() -> McpConfig {
    McpConfig {
        defaults: McpDefaultsConfig {
            profile_id: "strict".to_string(),
            upstream_timeout_ms: 10_000,
            output_sanitization: true,
        },
        servers: vec![McpServerDefinition {
            id: "repo-tools".to_string(),
            endpoint: "http://repo-tools.internal".to_string(),
            enabled: true,
            allowed_tools: vec!["read_file".to_string()],
            blocked_tools: Vec::new(),
            tools: std::collections::BTreeMap::from([(
                "read_file".to_string(),
                McpToolPolicy {
                    enabled: true,
                    argument_schema: None,
                    argument_constraints: ToolArgumentConstraints::default(),
                    output_sanitization: Some(true),
                },
            )]),
        }],
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
        aliases: Vec::new(),
        output_sanitization: Some(false),
        enabled: true,
    });
    config.llm = Some(llm);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("duplicate enabled routes must fail validation");

    assert!(error.to_string().contains("must map to at most one enabled route"));
}

#[test]
fn rejects_invalid_stream_sanitization_buffer_size() {
    let mut config = valid_config();
    let mut llm = valid_llm_config();
    llm.defaults.stream_sanitization_max_buffer_bytes = 32;
    config.llm = Some(llm);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("invalid stream sanitization buffer size must fail validation");

    assert!(error.to_string().contains("must be in range 1024..=16777216"));
}

#[test]
fn accepts_valid_mcp_configuration() {
    let mut config = valid_config();
    config.mcp = Some(valid_mcp_config());

    validate_runtime_config(&config, Path::new("config.yaml"))
        .expect("mcp configuration should be valid");
}

#[test]
fn rejects_duplicate_mcp_server_ids() {
    let mut config = valid_config();
    let mut mcp = valid_mcp_config();
    mcp.servers.push(mcp.servers[0].clone());
    config.mcp = Some(mcp);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("duplicate mcp server ids must fail validation");

    assert!(error.to_string().contains("mcp.servers[1].id"));
    assert!(error.to_string().contains("must be unique"));
}

#[test]
fn rejects_tool_declared_in_both_allowed_and_blocked_lists() {
    let mut config = valid_config();
    let mut mcp = valid_mcp_config();
    mcp.servers[0].allowed_tools = vec!["read_file".to_string()];
    mcp.servers[0].blocked_tools = vec!["read_file".to_string()];
    config.mcp = Some(mcp);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("blocked>allowed conflicts must fail validation");

    assert!(error.to_string().contains("blocked tool cannot be listed in allowed_tools"));
}

#[test]
fn rejects_enabled_mcp_server_without_allowlisted_tools() {
    let mut config = valid_config();
    let mut mcp = valid_mcp_config();
    mcp.servers[0].allowed_tools.clear();
    mcp.servers[0].tools.clear();
    config.mcp = Some(mcp);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("enabled MCP server without allowlisted tools must fail validation");

    assert!(error.to_string().contains("mcp.servers[0].allowed_tools"));
    assert!(error.to_string().contains("must contain at least one tool id when server is enabled"));
}

#[test]
fn rejects_empty_identity_resolution_order() {
    let mut config = valid_config();
    config.identity.resolution_order.clear();

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("empty identity resolution order must fail");
    assert!(error.to_string().contains("identity.resolution_order"));
}

#[test]
fn rejects_unknown_identity_profile_binding() {
    let mut config = valid_config();
    config.identity.profile_bindings.insert("tenant-a".to_string(), "unknown".to_string());

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("unknown identity profile must fail");
    assert!(error.to_string().contains("identity.profile_bindings.tenant-a"));
}

#[test]
fn rejects_passthrough_mode_without_gateway_api_keys() {
    let mut config = valid_config();
    config.auth.upstream_auth_mode = UpstreamAuthMode::Passthrough;
    config.security.api_keys.clear();

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("passthrough mode without gateway api key bindings must fail");
    assert!(error.to_string().contains("auth.upstream_auth_mode"));
    assert!(error.to_string().contains("passthrough_requires_api_key_gateway_auth"));
}

#[test]
fn accepts_passthrough_without_gateway_api_keys_for_mesh_mtls_mode() {
    let mut config = valid_config();
    config.auth.upstream_auth_mode = UpstreamAuthMode::Passthrough;
    config.auth.gateway_auth_mode = crate::model::GatewayAuthMode::MeshMtls;
    config.security.api_keys.clear();

    validate_runtime_config(&config, Path::new("config.yaml"))
        .expect("mesh mTLS passthrough should not require security.api_keys");
}

#[test]
fn rejects_passthrough_mode_without_required_mesh_identity_header() {
    let mut config = valid_config();
    config.auth.upstream_auth_mode = UpstreamAuthMode::Passthrough;
    config.auth.gateway_auth_mode = crate::model::GatewayAuthMode::MeshMtls;
    config.auth.mesh.require_header = false;
    config.security.api_keys.clear();

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("passthrough mesh mTLS must require identity header");
    assert!(error.to_string().contains("auth.mesh.require_header"));
    assert!(error.to_string().contains("passthrough_requires_mesh_identity_header"));
}

#[test]
fn rejects_single_bearer_opt_in_outside_passthrough_mode() {
    let mut config = valid_config();
    config.auth.upstream_auth_mode = UpstreamAuthMode::Static;
    config.auth.allow_single_bearer_passthrough = true;

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("single-bearer opt-in must not apply in static mode");
    assert!(error.to_string().contains("auth.allow_single_bearer_passthrough"));
}

#[test]
fn rejects_internal_mtls_mode_without_tls_requirements() {
    let mut config = valid_config();
    config.auth.gateway_auth_mode = crate::model::GatewayAuthMode::InternalMtls;

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("internal mTLS mode must require TLS server settings");
    assert!(error.to_string().contains("server.tls.enabled"));
    assert!(error.to_string().contains("server.tls.require_client_cert"));
}

#[test]
fn rejects_invalid_llm_provider_upstream_path() {
    let mut config = valid_config();
    let mut llm = valid_llm_config();
    llm.providers[0].upstream_path = Some("chat/completions".to_string());
    config.llm = Some(llm);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("provider upstream_path without leading slash must fail validation");
    assert!(error.to_string().contains("llm.providers[0].upstream_path"));
}

#[test]
fn rejects_non_normalized_llm_provider_upstream_path() {
    for upstream_path in [
        "/chat/completions/",
        "/../../v1/chat/completions",
        "/chat//completions",
        "/chat/completions?x=1",
    ] {
        let mut config = valid_config();
        let mut llm = valid_llm_config();
        llm.providers[0].upstream_path = Some(upstream_path.to_string());
        config.llm = Some(llm);

        let error = validate_runtime_config(&config, Path::new("config.yaml"))
            .expect_err("non-normalized provider upstream_path must fail validation");
        assert!(error.to_string().contains("llm.providers[0].upstream_path"));
    }
}

#[test]
fn rejects_route_alias_longer_than_model_limit() {
    let mut config = valid_config();
    let mut llm = valid_llm_config();
    llm.routes[0].aliases = vec!["a".repeat(129)];
    config.llm = Some(llm);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("alias longer than 128 chars must fail validation");
    assert!(error.to_string().contains("llm.routes[0].aliases[0]"));
}

#[test]
fn rejects_alias_conflict_after_normalization() {
    let mut config = valid_config();
    let mut llm = valid_llm_config();
    llm.routes.push(LlmRouteConfig {
        model: "gpt-4.1-mini".to_string(),
        provider_id: "openai".to_string(),
        aliases: vec!["OPENAI/GPT-4O-MINI".to_string()],
        output_sanitization: Some(false),
        enabled: true,
    });
    config.llm = Some(llm);

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("alias collision with canonical model must fail validation");
    assert!(error.to_string().contains("alias_conflict_after_normalization"));
}

#[test]
fn rejects_duplicate_deterministic_recognizer_ids() {
    let mut config = valid_config();
    config.sanitization.profiles.strict.deterministic_recognizers = vec![
        DeterministicRecognizerConfig {
            id: "payment_card".to_string(),
            category: DetectionCategory::Secrets,
            action: PolicyAction::Block,
            family_priority: 600,
            enabled: true,
            patterns: vec![DeterministicPatternConfig {
                id: "card_pattern".to_string(),
                expression: "\\b(?:\\d[ -]*?){13,16}\\b".to_string(),
                base_score: 200,
                validator: None,
                normalization: crate::model::DeterministicNormalizationMode::Preserve,
            }],
            denylist_exact: Vec::new(),
            allowlist_exact: Vec::new(),
            context: None,
        },
        DeterministicRecognizerConfig {
            id: "payment_card".to_string(),
            category: DetectionCategory::Secrets,
            action: PolicyAction::Block,
            family_priority: 600,
            enabled: true,
            patterns: Vec::new(),
            denylist_exact: Vec::new(),
            allowlist_exact: Vec::new(),
            context: None,
        },
    ];

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("duplicate deterministic recognizer ids must fail validation");
    assert!(error.to_string().contains("deterministic_recognizers[1].id"));
}

#[test]
fn rejects_invalid_deterministic_pattern_expression() {
    let mut config = valid_config();
    config.sanitization.profiles.strict.deterministic_recognizers =
        vec![DeterministicRecognizerConfig {
            id: "invalid_pattern".to_string(),
            category: DetectionCategory::Secrets,
            action: PolicyAction::Block,
            family_priority: 600,
            enabled: true,
            patterns: vec![DeterministicPatternConfig {
                id: "broken".to_string(),
                expression: "(".to_string(),
                base_score: 100,
                validator: None,
                normalization: crate::model::DeterministicNormalizationMode::Preserve,
            }],
            denylist_exact: Vec::new(),
            allowlist_exact: Vec::new(),
            context: None,
        }];

    let error = validate_runtime_config(&config, Path::new("config.yaml"))
        .expect_err("invalid deterministic regex must fail validation");
    assert!(error.to_string().contains("deterministic_recognizers[0].patterns[0].expression"));
}
