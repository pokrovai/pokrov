use std::path::{Path, PathBuf};

use crate::{error::ConfigError, model::RuntimeConfig, validate::validate_runtime_config};

pub fn load_runtime_config(path: impl AsRef<Path>) -> Result<RuntimeConfig, ConfigError> {
    let path = path.as_ref();
    let path_buf = PathBuf::from(path);
    let content = std::fs::read_to_string(path)
        .map_err(|source| ConfigError::Io { path: path_buf.clone(), source })?;

    let config: RuntimeConfig = serde_yaml::from_str(&content)
        .map_err(|source| ConfigError::Parse { path: path_buf.clone(), source })?;

    validate_runtime_config(&config, path)?;
    Ok(config)
}
