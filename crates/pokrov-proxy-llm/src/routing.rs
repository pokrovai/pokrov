use std::collections::BTreeMap;

use pokrov_config::model::{LlmConfig, LlmProviderConfig, LlmRouteConfig};

use crate::{errors::LLMProxyError, types::RouteResolution};

#[derive(Debug, Clone)]
struct ProviderRecord {
    id: String,
    base_url: String,
    api_key: String,
    timeout_ms: u64,
    retry_budget: u8,
}

#[derive(Debug, Clone)]
struct RouteRecord {
    provider_id: String,
    output_sanitization: bool,
}

#[derive(Debug, Clone)]
pub struct ProviderRouteTable {
    default_profile_id: String,
    stream_sanitization_max_buffer_bytes: usize,
    providers: BTreeMap<String, ProviderRecord>,
    routes: BTreeMap<String, RouteRecord>,
}

impl ProviderRouteTable {
    pub fn from_config(
        config: &LlmConfig,
        resolved_provider_keys: &BTreeMap<String, String>,
    ) -> Result<Self, LLMProxyError> {
        let providers = build_provider_map(config, resolved_provider_keys);
        let routes = build_route_map(config, &providers);

        Ok(Self {
            default_profile_id: config.defaults.profile_id.clone(),
            stream_sanitization_max_buffer_bytes: config.defaults.stream_sanitization_max_buffer_bytes,
            providers,
            routes,
        })
    }

    pub fn default_profile_id(&self) -> &str {
        &self.default_profile_id
    }

    pub fn routes_loaded(&self) -> bool {
        !self.routes.is_empty()
    }

    pub fn resolve(&self, request_id: &str, model: &str) -> Result<RouteResolution, LLMProxyError> {
        let route = self
            .routes
            .get(model)
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
) -> BTreeMap<String, RouteRecord> {
    let mut routes = BTreeMap::new();

    for route in config.routes.iter().filter(|route| route.enabled) {
        if providers.contains_key(&route.provider_id) {
            routes.insert(
                route.model.clone(),
                RouteRecord {
                    provider_id: route.provider_id.clone(),
                    output_sanitization: route
                        .output_sanitization
                        .unwrap_or(config.defaults.output_sanitization),
                },
            );
        }
    }

    routes
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

#[cfg(test)]
mod tests {
    use super::ProviderRouteTable;
    use pokrov_config::model::{
        LlmConfig, LlmDefaultsConfig, LlmProviderAuthConfig, LlmProviderConfig, LlmRouteConfig,
    };
    use std::collections::BTreeMap;

    fn llm_config() -> LlmConfig {
        LlmConfig {
            providers: vec![LlmProviderConfig {
                id: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
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
        assert!(resolution.output_sanitization);
        assert_eq!(resolution.stream_sanitization_max_buffer_bytes, 1024 * 1024);
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
}
