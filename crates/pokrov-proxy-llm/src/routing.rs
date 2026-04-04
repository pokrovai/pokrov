use std::collections::BTreeMap;

use pokrov_config::{
    model::{LlmConfig, LlmProviderConfig, LlmRouteConfig},
    normalize_model_key,
    UpstreamAuthMode,
};

use crate::{
    errors::LLMProxyError,
    types::{
        RouteResolution, SelectedUpstreamCredential, UpstreamCredentialOrigin,
        CHAT_COMPLETIONS_UPSTREAM_PATH,
    },
};

#[derive(Debug, Clone)]
struct ProviderRecord {
    id: String,
    base_url: String,
    effective_upstream_path: String,
    api_key: String,
    timeout_ms: u64,
    retry_budget: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteKeySource {
    Canonical,
    Alias,
}

#[derive(Debug, Clone)]
struct RouteRecord {
    provider_id: String,
    canonical_model: String,
    source: RouteKeySource,
    output_sanitization: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogKind {
    Canonical,
    Alias,
}

#[derive(Debug, Clone)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub canonical_model: String,
    pub provider_id: String,
    pub kind: ModelCatalogKind,
}

#[derive(Debug, Clone)]
pub struct ProviderRouteTable {
    default_profile_id: String,
    stream_sanitization_max_buffer_bytes: usize,
    providers: BTreeMap<String, ProviderRecord>,
    routes: BTreeMap<String, RouteRecord>,
    catalog: Vec<ModelCatalogEntry>,
}

impl ProviderRouteTable {
    pub fn from_config(
        config: &LlmConfig,
        resolved_provider_keys: &BTreeMap<String, String>,
    ) -> Result<Self, LLMProxyError> {
        let providers = build_provider_map(config, resolved_provider_keys);
        let (routes, catalog) = build_route_map(config, &providers)?;

        Ok(Self {
            default_profile_id: config.defaults.profile_id.clone(),
            stream_sanitization_max_buffer_bytes: config.defaults.stream_sanitization_max_buffer_bytes,
            providers,
            routes,
            catalog,
        })
    }

    pub fn default_profile_id(&self) -> &str {
        &self.default_profile_id
    }

    pub fn routes_loaded(&self) -> bool {
        !self.catalog.is_empty()
    }

    pub fn model_catalog(&self) -> &[ModelCatalogEntry] {
        &self.catalog
    }

    pub fn resolve(&self, request_id: &str, model: &str) -> Result<RouteResolution, LLMProxyError> {
        let normalized_model = normalize_model_key(model);
        let route = self
            .routes
            .get(&normalized_model)
            .ok_or_else(|| LLMProxyError::model_not_routed(request_id, model.to_string()))?;

        let provider = self.providers.get(&route.provider_id).ok_or_else(|| {
            LLMProxyError::upstream_unavailable(
                request_id,
                Some(route.provider_id.clone()),
                "provider credentials are not resolved",
            )
        })?;

        Ok(RouteResolution {
            provider_id: provider.id.clone(),
            base_url: provider.base_url.clone(),
            effective_upstream_path: provider.effective_upstream_path.clone(),
            canonical_model: route.canonical_model.clone(),
            resolved_via_alias: matches!(route.source, RouteKeySource::Alias),
            api_key: provider.api_key.clone(),
            timeout_ms: provider.timeout_ms,
            retry_budget: provider.retry_budget,
            output_sanitization: route.output_sanitization,
            stream_sanitization_max_buffer_bytes: self.stream_sanitization_max_buffer_bytes,
        })
    }
}

fn build_provider_map(
    config: &LlmConfig,
    resolved_provider_keys: &BTreeMap<String, String>,
) -> BTreeMap<String, ProviderRecord> {
    let mut providers = BTreeMap::new();

    for provider in config.providers.iter().filter(|provider| provider.enabled) {
        if let Some(api_key) = resolved_provider_keys.get(&provider.id) {
            providers.insert(
                provider.id.clone(),
                ProviderRecord {
                    id: provider.id.clone(),
                    base_url: provider.base_url.trim_end_matches('/').to_string(),
                    effective_upstream_path: provider
                        .upstream_path
                        .as_deref()
                        .unwrap_or(CHAT_COMPLETIONS_UPSTREAM_PATH)
                        .to_string(),
                    api_key: api_key.clone(),
                    timeout_ms: provider.timeout_ms,
                    retry_budget: provider.retry_budget,
                },
            );
        }
    }

    providers
}

fn build_route_map(
    config: &LlmConfig,
    providers: &BTreeMap<String, ProviderRecord>,
) -> Result<(BTreeMap<String, RouteRecord>, Vec<ModelCatalogEntry>), LLMProxyError> {
    let mut routes = BTreeMap::new();
    let mut catalog = Vec::new();

    for route in config.routes.iter().filter(|route| route.enabled) {
        if !providers.contains_key(&route.provider_id) {
            continue;
        }

        let output_sanitization = route
            .output_sanitization
            .unwrap_or(config.defaults.output_sanitization);
        let canonical_model = route.model.clone();
        let canonical_key = normalize_model_key(&canonical_model);

        insert_route_key(
            &mut routes,
            &canonical_key,
            RouteRecord {
                provider_id: route.provider_id.clone(),
                canonical_model: canonical_model.clone(),
                source: RouteKeySource::Canonical,
                output_sanitization,
            },
        )?;

        catalog.push(ModelCatalogEntry {
            id: canonical_model.clone(),
            canonical_model: canonical_model.clone(),
            provider_id: route.provider_id.clone(),
            kind: ModelCatalogKind::Canonical,
        });

        for alias in &route.aliases {
            let alias_key = normalize_model_key(alias);
            insert_route_key(
                &mut routes,
                &alias_key,
                RouteRecord {
                    provider_id: route.provider_id.clone(),
                    canonical_model: canonical_model.clone(),
                    source: RouteKeySource::Alias,
                    output_sanitization,
                },
            )?;
            catalog.push(ModelCatalogEntry {
                id: alias.clone(),
                canonical_model: canonical_model.clone(),
                provider_id: route.provider_id.clone(),
                kind: ModelCatalogKind::Alias,
            });
        }
    }

    catalog.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((routes, catalog))
}

fn insert_route_key(
    routes: &mut BTreeMap<String, RouteRecord>,
    key: &str,
    record: RouteRecord,
) -> Result<(), LLMProxyError> {
    if key.is_empty() {
        return Err(LLMProxyError::alias_conflict(
            "system",
            "route model/alias cannot be empty after normalization",
        ));
    }

    if routes.contains_key(key) {
        return Err(LLMProxyError::alias_conflict(
            "system",
            format!("normalized key '{key}' collides with another enabled route"),
        ));
    }

    routes.insert(key.to_string(), record);
    Ok(())
}

pub fn resolve_provider_keys(config: &LlmConfig) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    for provider in &config.providers {
        if let Some(secret) = resolve_secret_ref(&provider.auth.api_key) {
            keys.insert(provider.id.clone(), secret);
        }
    }
    keys
}

pub fn resolve_secret_ref(raw: &str) -> Option<String> {
    if let Some(env_name) = raw.strip_prefix("env:") {
        return std::env::var(env_name.trim()).ok();
    }

    if let Some(path) = raw.strip_prefix("file:") {
        return std::fs::read_to_string(path.trim())
            .ok()
            .map(|value| value.trim().to_string());
    }

    None
}

pub fn resolve_provider_key(
    provider: &LlmProviderConfig,
    resolved_provider_keys: &BTreeMap<String, String>,
) -> Option<String> {
    resolved_provider_keys.get(&provider.id).cloned()
}

pub fn route_provider_id(route: &LlmRouteConfig) -> &str {
    &route.provider_id
}

pub fn select_upstream_credential(
    mode: UpstreamAuthMode,
    route: &RouteResolution,
    request_credential: Option<&str>,
) -> Option<SelectedUpstreamCredential> {
    match mode {
        UpstreamAuthMode::Static => Some(SelectedUpstreamCredential {
            token: route.api_key.clone(),
            origin: UpstreamCredentialOrigin::Config,
        }),
        UpstreamAuthMode::Passthrough => request_credential
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(|token| SelectedUpstreamCredential {
                token: token.to_string(),
                origin: UpstreamCredentialOrigin::Request,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::{select_upstream_credential, ModelCatalogKind, ProviderRouteTable};
    use crate::types::{RouteResolution, UpstreamCredentialOrigin};
    use pokrov_config::model::{
        LlmConfig, LlmDefaultsConfig, LlmProviderAuthConfig, LlmProviderConfig, LlmRouteConfig,
    };
    use pokrov_config::UpstreamAuthMode;
    use std::collections::BTreeMap;

    fn llm_config() -> LlmConfig {
        LlmConfig {
            providers: vec![LlmProviderConfig {
                id: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                upstream_path: Some("/chat/completions".to_string()),
                auth: LlmProviderAuthConfig {
                    api_key: "env:OPENAI_API_KEY".to_string(),
                },
                timeout_ms: 30000,
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
                output_sanitization: false,
                stream_sanitization_max_buffer_bytes: 1024 * 1024,
            },
        }
    }

    #[test]
    fn resolves_route_deterministically() {
        let config = llm_config();
        let keys = BTreeMap::from([("openai".to_string(), "test-key".to_string())]);
        let table = ProviderRouteTable::from_config(&config, &keys)
            .expect("table should build from valid config");

        let resolution = table
            .resolve("req-1", "gpt-4o-mini")
            .expect("configured model should resolve");

        assert_eq!(resolution.provider_id, "openai");
        assert!(!resolution.resolved_via_alias);
        assert_eq!(resolution.canonical_model, "gpt-4o-mini");
        assert_eq!(resolution.effective_upstream_path, "/chat/completions");
        assert!(resolution.output_sanitization);
        assert_eq!(resolution.stream_sanitization_max_buffer_bytes, 1024 * 1024);
    }

    #[test]
    fn resolves_alias_case_insensitively_to_canonical_route() {
        let config = llm_config();
        let keys = BTreeMap::from([("openai".to_string(), "test-key".to_string())]);
        let table = ProviderRouteTable::from_config(&config, &keys)
            .expect("table should build from valid config");

        let resolution = table
            .resolve("req-1", "OPENAI/GPT-4O-MINI")
            .expect("alias should resolve");

        assert!(resolution.resolved_via_alias);
        assert_eq!(resolution.canonical_model, "gpt-4o-mini");
    }

    #[test]
    fn returns_model_not_routed_when_model_is_missing() {
        let config = llm_config();
        let keys = BTreeMap::from([("openai".to_string(), "test-key".to_string())]);
        let table = ProviderRouteTable::from_config(&config, &keys)
            .expect("table should build from valid config");

        let error = table
            .resolve("req-2", "unknown-model")
            .expect_err("unknown model should fail");

        assert_eq!(error.code().as_str(), "model_not_routed");
    }

    #[test]
    fn returns_canonical_and_alias_entries_in_model_catalog() {
        let config = llm_config();
        let keys = BTreeMap::from([("openai".to_string(), "test-key".to_string())]);
        let table = ProviderRouteTable::from_config(&config, &keys)
            .expect("table should build from valid config");

        assert_eq!(table.model_catalog().len(), 2);
        assert_eq!(table.model_catalog()[0].kind, ModelCatalogKind::Canonical);
        assert_eq!(table.model_catalog()[1].kind, ModelCatalogKind::Alias);
    }

    #[test]
    fn selects_static_credential_from_route() {
        let route = RouteResolution {
            provider_id: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            effective_upstream_path: "/chat/completions".to_string(),
            canonical_model: "gpt-4o-mini".to_string(),
            resolved_via_alias: false,
            api_key: "static-key".to_string(),
            timeout_ms: 30_000,
            retry_budget: 1,
            output_sanitization: true,
            stream_sanitization_max_buffer_bytes: 1024,
        };

        let selected = select_upstream_credential(UpstreamAuthMode::Static, &route, None)
            .expect("static mode should always resolve");
        assert_eq!(selected.token, "static-key");
        assert_eq!(selected.origin, UpstreamCredentialOrigin::Config);
    }

    #[test]
    fn selects_passthrough_credential_from_request() {
        let route = RouteResolution {
            provider_id: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            effective_upstream_path: "/chat/completions".to_string(),
            canonical_model: "gpt-4o-mini".to_string(),
            resolved_via_alias: false,
            api_key: "static-key".to_string(),
            timeout_ms: 30_000,
            retry_budget: 1,
            output_sanitization: true,
            stream_sanitization_max_buffer_bytes: 1024,
        };

        let selected = select_upstream_credential(
            UpstreamAuthMode::Passthrough,
            &route,
            Some(" provider-key "),
        )
        .expect("passthrough mode should use request credential");
        assert_eq!(selected.token, "provider-key");
        assert_eq!(selected.origin, UpstreamCredentialOrigin::Request);
    }

    #[test]
    fn passthrough_returns_none_when_request_credential_is_missing() {
        let route = RouteResolution {
            provider_id: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            effective_upstream_path: "/chat/completions".to_string(),
            canonical_model: "gpt-4o-mini".to_string(),
            resolved_via_alias: false,
            api_key: "static-key".to_string(),
            timeout_ms: 30_000,
            retry_budget: 1,
            output_sanitization: true,
            stream_sanitization_max_buffer_bytes: 1024,
        };

        assert!(
            select_upstream_credential(UpstreamAuthMode::Passthrough, &route, None).is_none()
        );
        assert!(
            select_upstream_credential(UpstreamAuthMode::Passthrough, &route, Some("  ")).is_none()
        );
    }
}
