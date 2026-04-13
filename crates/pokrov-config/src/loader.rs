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
    resolve_relative_observability_paths(&mut config, path);
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

fn resolve_relative_observability_paths(config: &mut RuntimeConfig, config_path: &Path) {
    let output_path = Path::new(&config.observability.llm_payload_trace.output_path);
    if output_path.is_absolute() {
        return;
    }

    let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let resolved = base_dir.join(output_path);
    config.observability.llm_payload_trace.output_path = resolved.to_string_lossy().to_string();
}

#[cfg(test)]
mod tests {
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    use super::load_runtime_config;

    #[test]
    fn resolves_relative_llm_payload_trace_output_path_against_config_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic for test id")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("pokrov-config-loader-{unique}"));
        let config_dir = temp_root.join("nested");
        fs::create_dir_all(&config_dir).expect("temp config directory must be created");
        let config_path = config_dir.join("runtime.yaml");

        let config = r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 5000
  grace_period_ms: 10000
observability:
  llm_payload_trace:
    enabled: false
    output_path: ./tmp/pokrov-llm-payload-trace.ndjson
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
sanitization:
  enabled: false
rate_limit:
  enabled: false
"#;
        fs::write(&config_path, config).expect("temp config file must be written");

        let loaded = load_runtime_config(&config_path).expect("config should load");
        let expected = config_dir.join("./tmp/pokrov-llm-payload-trace.ndjson");
        assert_eq!(
            loaded.observability.llm_payload_trace.output_path,
            expected.to_string_lossy().to_string()
        );

        let _ = fs::remove_dir_all(temp_root);
    }
}
