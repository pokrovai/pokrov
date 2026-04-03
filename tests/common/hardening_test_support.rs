use std::{io::Write, path::PathBuf};

use tempfile::NamedTempFile;

pub fn write_hardening_runtime_config(content: &str) -> PathBuf {
    let mut file = NamedTempFile::new().expect("hardening config file should be created");
    file.write_all(content.as_bytes())
        .expect("hardening config should be written");
    file.into_temp_path()
        .keep()
        .expect("hardening config path should persist")
}

pub fn count_prometheus_series(metrics_payload: &str, metric_name: &str) -> usize {
    metrics_payload
        .lines()
        .filter(|line| line.starts_with(metric_name))
        .count()
}
