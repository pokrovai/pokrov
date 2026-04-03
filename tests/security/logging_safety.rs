use std::{
    io,
    sync::{Arc, Mutex},
};

use tracing_subscriber::fmt::MakeWriter;

#[test]
fn structured_logging_remains_metadata_only() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter { buf: captured.clone() };
    let secret = "plain-api-key-should-never-appear";
    let payload = r#"{"secret":"raw-payload"}"#;

    let subscriber = pokrov_runtime::observability::json_subscriber_with_writer("info", writer);

    tracing::subscriber::with_default(subscriber, || {
        let _ = (secret, payload);

        pokrov_runtime::observability::log_lifecycle_event(
            "runtime",
            "startup",
            Some("req-test-1"),
            "ready",
        );

        tracing::info!(
            component = "runtime",
            action = "request_completed",
            request_id = "req-test-1",
            method = "GET",
            path = "/health",
            status_code = 200u16
        );

        pokrov_proxy_mcp::audit::McpAuditEvent {
            request_id: "req-test-1".to_string(),
            server_id: "repo-tools".to_string(),
            tool_id: "read_file".to_string(),
            profile_id: "strict".to_string(),
            final_action: "allow",
            rule_hits_total: 1,
            blocked: false,
            upstream_status: Some(200),
            duration_ms: 3,
        }
        .emit();
    });

    let logs = String::from_utf8(captured.lock().expect("writer lock").clone())
        .expect("captured logs should be utf-8");

    assert!(logs.contains("\"request_id\":\"req-test-1\""));
    assert!(logs.contains("\"action\":\"startup\""));
    assert!(logs.contains("\"action\":\"tool_call_completed\""));
    assert!(!logs.contains(secret));
    assert!(!logs.contains(payload));
}

#[derive(Clone)]
struct SharedWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl<'a> MakeWriter<'a> for SharedWriter {
    type Writer = BufferGuard;

    fn make_writer(&'a self) -> Self::Writer {
        BufferGuard { buf: self.buf.clone() }
    }
}

struct BufferGuard {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl io::Write for BufferGuard {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.lock().expect("writer lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
