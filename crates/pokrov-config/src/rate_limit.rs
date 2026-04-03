use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_rate_limit_profile")]
    pub default_profile: String,
    #[serde(default)]
    pub profiles: std::collections::BTreeMap<String, RateLimitProfile>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_profile: default_rate_limit_profile(),
            profiles: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitProfile {
    pub requests_per_minute: u32,
    pub token_units_per_minute: u32,
    #[serde(default = "default_burst_multiplier")]
    pub burst_multiplier: f32,
    #[serde(default = "default_enforcement_mode")]
    pub enforcement_mode: RateLimitEnforcementMode,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitEnforcementMode {
    Enforce,
    DryRun,
}

fn default_rate_limit_profile() -> String {
    "strict".to_string()
}

fn default_burst_multiplier() -> f32 {
    1.0
}

fn default_enforcement_mode() -> RateLimitEnforcementMode {
    RateLimitEnforcementMode::Enforce
}
