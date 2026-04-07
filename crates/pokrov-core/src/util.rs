use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Formats a Unix-epoch milliseconds timestamp as an RFC 3339 string.
/// Returns `"invalid_unix_ms"` for out-of-range values.
pub fn format_unix_ms_rfc3339(unix_ms: u64) -> String {
    OffsetDateTime::from_unix_timestamp_nanos((unix_ms as i128).saturating_mul(1_000_000))
        .ok()
        .and_then(|ts| ts.format(&Rfc3339).ok())
        .unwrap_or_else(|| "invalid_unix_ms".to_string())
}
