use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamAuthMode {
    #[default]
    Static,
    Passthrough,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAuthMode {
    #[default]
    ApiKey,
    InternalMtls,
    MeshMtls,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InternalMtlsAuthConfig {
    #[serde(default = "default_internal_mtls_identity_header")]
    pub identity_header: String,
    #[serde(default = "default_true")]
    pub require_header: bool,
}

impl Default for InternalMtlsAuthConfig {
    fn default() -> Self {
        Self { identity_header: default_internal_mtls_identity_header(), require_header: true }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshAuthConfig {
    #[serde(default = "default_mesh_identity_header")]
    pub identity_header: String,
    #[serde(default)]
    pub required_spiffe_trust_domain: Option<String>,
    #[serde(default = "default_true")]
    pub require_header: bool,
}

impl Default for MeshAuthConfig {
    fn default() -> Self {
        Self {
            identity_header: default_mesh_identity_header(),
            required_spiffe_trust_domain: None,
            require_header: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub upstream_auth_mode: UpstreamAuthMode,
    #[serde(default)]
    pub allow_single_bearer_passthrough: bool,
    #[serde(default)]
    pub gateway_auth_mode: GatewayAuthMode,
    #[serde(default)]
    pub internal_mtls: InternalMtlsAuthConfig,
    #[serde(default)]
    pub mesh: MeshAuthConfig,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            upstream_auth_mode: UpstreamAuthMode::Static,
            allow_single_bearer_passthrough: false,
            gateway_auth_mode: GatewayAuthMode::ApiKey,
            internal_mtls: InternalMtlsAuthConfig::default(),
            mesh: MeshAuthConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdentityConfig {
    #[serde(default = "default_identity_resolution_order")]
    pub resolution_order: Vec<IdentitySource>,
    #[serde(default)]
    pub profile_bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub rate_limit_bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub required_for_policy: bool,
    #[serde(default)]
    pub required_for_rate_limit: bool,
    #[serde(default)]
    pub fallback_policy_profile: Option<String>,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            resolution_order: default_identity_resolution_order(),
            profile_bindings: BTreeMap::new(),
            rate_limit_bindings: BTreeMap::new(),
            required_for_policy: false,
            required_for_rate_limit: false,
            fallback_policy_profile: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    GatewayAuthSubject,
    XPokrovClientId,
    IngressIdentity,
}

fn default_identity_resolution_order() -> Vec<IdentitySource> {
    vec![
        IdentitySource::GatewayAuthSubject,
        IdentitySource::XPokrovClientId,
        IdentitySource::IngressIdentity,
    ]
}

fn default_true() -> bool {
    true
}

fn default_internal_mtls_identity_header() -> String {
    "x-pokrov-client-cert-subject".to_string()
}

fn default_mesh_identity_header() -> String {
    "x-forwarded-client-cert".to_string()
}
