use std::path::{Path, PathBuf};

use crate::{error::ConfigError, model::RuntimeConfig, validate::validate_runtime_config};

const DEFAULT_LLM_UPSTREAM_PATH: &str = "/chat/completions";

pub fn load_runtime_config(path: impl AsRef<Path>) -> Result<RuntimeConfig, ConfigError> {
    let path = path.as_ref();
    let path_buf = PathBuf::from(path);
    let content = std::fs::read_to_string(path)
        .map_err(|source| ConfigError::Io { path: path_buf.clone(), source })?;

    let config: RuntimeConfig = serde_yaml::from_str(&content)
        .map_err(|source| ConfigError::Parse { path: path_buf.clone(), source })?;

    let mut config = config;
    apply_llm_defaults(&mut config);
    validate_runtime_config(&config, path)?;
    Ok(config)
}

fn apply_llm_defaults(config: &mut RuntimeConfig) {
    let Some(llm) = config.llm.as_mut() else {
        return;
    };

    for provider in &mut llm.providers {
        if provider.upstream_path.is_none() {
            provider.upstream_path = Some(DEFAULT_LLM_UPSTREAM_PATH.to_string());
        }
    }
}
