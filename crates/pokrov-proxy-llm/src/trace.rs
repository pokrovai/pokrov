use std::{
    fs::{create_dir_all, File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use serde::Serialize;
use serde_json::Value;

/// Trace sink used only in local debug builds to capture exact upstream payloads.
#[derive(Debug, Clone)]
pub struct LlmPayloadTraceSink {
    writer: Arc<Mutex<BufWriter<File>>>,
}

impl LlmPayloadTraceSink {
    pub fn new(path: &str) -> Result<Self, std::io::Error> {
        let target = Path::new(path);
        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent)?;
            }
        }

        let file = OpenOptions::new().create(true).append(true).open(target)?;
        Ok(Self { writer: Arc::new(Mutex::new(BufWriter::new(file))) })
    }

    pub fn emit_request_payload(
        &self,
        request_id: &str,
        provider_id: &str,
        endpoint: &str,
        attempt: u8,
        payload: &Value,
    ) {
        self.emit_payload(
            "llm_upstream_payload",
            request_id,
            provider_id,
            endpoint,
            attempt,
            payload,
        );
    }

    pub fn emit_response_payload(
        &self,
        request_id: &str,
        provider_id: &str,
        endpoint: &str,
        payload: &Value,
    ) {
        self.emit_payload(
            "llm_final_response_payload",
            request_id,
            provider_id,
            endpoint,
            0,
            payload,
        );
    }

    fn emit_payload(
        &self,
        event: &'static str,
        request_id: &str,
        provider_id: &str,
        endpoint: &str,
        attempt: u8,
        payload: &Value,
    ) {
        let ts_unix_ms = now_unix_ms();
        let line = TraceRecord {
            event,
            ts_unix_ms,
            ts_rfc3339: format_unix_ms_rfc3339(ts_unix_ms),
            request_id,
            provider_id,
            endpoint,
            attempt,
            payload,
        };

        let mut writer = match self.writer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if serde_json::to_writer(&mut *writer, &line).is_ok() {
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
            return;
        }

        tracing::warn!(
            component = "llm_proxy",
            action = "payload_trace_write_failed",
            request_id = request_id,
            provider_id = provider_id,
            endpoint = endpoint
        );
    }
}

#[derive(Debug, Serialize)]
struct TraceRecord<'a> {
    event: &'static str,
    ts_unix_ms: u128,
    ts_rfc3339: String,
    request_id: &'a str,
    provider_id: &'a str,
    endpoint: &'a str,
    attempt: u8,
    payload: &'a Value,
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn format_unix_ms_rfc3339(unix_ms: u128) -> String {
    OffsetDateTime::from_unix_timestamp_nanos((unix_ms as i128).saturating_mul(1_000_000))
        .ok()
        .and_then(|ts| ts.format(&Rfc3339).ok())
        .unwrap_or_else(|| "invalid_unix_ms".to_string())
}
