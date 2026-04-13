use std::{env, fs, time::Duration};

use http::StatusCode;
use serde_json::Value;
use tokio::time::sleep;

use crate::{
    errors::LLMProxyError,
    types::{RouteResolution, UpstreamJsonResponse, UpstreamStreamResponse},
};
#[cfg(feature = "llm_payload_trace")]
use crate::trace::LlmPayloadTraceSink;

#[derive(Debug, Clone)]
pub struct UpstreamClient {
    client: reqwest::Client,
    #[cfg(feature = "llm_payload_trace")]
    payload_trace_sink: Option<LlmPayloadTraceSink>,
}

impl UpstreamClient {
    pub fn new() -> Result<Self, LLMProxyError> {
        let builder = reqwest::Client::builder().pool_max_idle_per_host(16);
        let builder = apply_standard_tls_env(builder).map_err(|error| {
            LLMProxyError::upstream_unavailable(
                "system",
                None,
                format!("failed to initialize upstream client TLS trust: {error}"),
            )
        })?;
        let client = builder.build().map_err(|error| {
            LLMProxyError::upstream_unavailable(
                "system",
                None,
                format!("failed to initialize upstream client: {error}"),
            )
        })?;

        Ok(Self {
            client,
            #[cfg(feature = "llm_payload_trace")]
            payload_trace_sink: None,
        })
    }

    #[cfg(feature = "llm_payload_trace")]
    pub fn with_payload_trace_sink(mut self, payload_trace_sink: Option<LlmPayloadTraceSink>) -> Self {
        self.payload_trace_sink = payload_trace_sink;
        self
    }

    pub async fn execute_json(
        &self,
        request_id: &str,
        route: &RouteResolution,
        payload: &Value,
        upstream_credential: Option<&str>,
    ) -> Result<UpstreamJsonResponse, LLMProxyError> {
        let endpoint = build_endpoint(&route.base_url, &route.effective_upstream_path);
        let bearer = upstream_credential.or(route.api_key.as_deref());

        for attempt in 0..=route.retry_budget {
            #[cfg(feature = "llm_payload_trace")]
            self.emit_payload_trace(request_id, route, &endpoint, attempt, payload);

            let mut request = self.client.post(&endpoint);
            if let Some(token) = bearer {
                request = request.bearer_auth(token);
            }
            let response = request
                .timeout(Duration::from_millis(route.timeout_ms))
                .json(payload)
                .send()
                .await;

            match response {
                Ok(response) => {
                    if response.status().is_success() {
                        let status = response.status();
                        let body = response.json::<Value>().await.map_err(|error| {
                            LLMProxyError::upstream_error(
                                request_id,
                                Some(route.provider_id.clone()),
                                format!("upstream returned invalid JSON: {error}"),
                            )
                        })?;

                        return Ok(UpstreamJsonResponse { status: to_status_code(status), body });
                    }

                    let status = response.status();
                    let headers = response.headers().clone();
                    let will_retry = should_retry_status(status) && attempt < route.retry_budget;
                    let base_summary = UpstreamErrorSummary::from_headers(&headers);
                    tracing::warn!(
                        component = "llm_proxy",
                        action = "upstream_status_error",
                        request_id = request_id,
                        provider_id = %route.provider_id,
                        endpoint = endpoint,
                        attempt = attempt,
                        retry_budget = route.retry_budget,
                        timeout_ms = route.timeout_ms,
                        status = status.as_u16(),
                        will_retry = will_retry,
                        credential_present = bearer.is_some(),
                        upstream_content_type = ?base_summary.content_type,
                        upstream_request_id = ?base_summary.request_id,
                    );

                    if will_retry {
                        let delay = compute_retry_delay(status, &headers, attempt);
                        if !delay.is_zero() {
                            sleep(delay).await;
                        }
                        continue;
                    }

                    let summary = summarize_upstream_error_response(response).await;
                    tracing::warn!(
                        component = "llm_proxy",
                        action = "upstream_status_error_detail",
                        request_id = request_id,
                        provider_id = %route.provider_id,
                        endpoint = endpoint,
                        attempt = attempt,
                        retry_budget = route.retry_budget,
                        timeout_ms = route.timeout_ms,
                        status = status.as_u16(),
                        credential_present = bearer.is_some(),
                        upstream_content_type = ?summary.content_type,
                        upstream_request_id = ?summary.request_id,
                        upstream_error_type = ?summary.error_type,
                        upstream_error_code = ?summary.error_code,
                        upstream_error_param = ?summary.error_param,
                        upstream_error_message_present = summary.error_message_present,
                        upstream_error_message_len = ?summary.error_message_len,
                        upstream_top_level_keys = ?summary.top_level_keys,
                        upstream_error_object_keys = ?summary.error_object_keys,
                        upstream_body_bytes = summary.body_bytes,
                        upstream_body_format = summary.body_format,
                        response_read_error = ?summary.response_read_error,
                    );

                    return Err(map_upstream_status_error(
                        request_id,
                        route.provider_id.clone(),
                        status,
                    ));
                }
                Err(error) => {
                    let is_timeout = error.is_timeout();
                    let is_connect = error.is_connect();
                    let will_retry = attempt < route.retry_budget && (is_timeout || is_connect);
                    let transport_error_class = classify_transport_error(&error);
                    let error_sources = format_error_sources(&error);
                    tracing::warn!(
                        component = "llm_proxy",
                        action = "upstream_transport_error",
                        request_id = request_id,
                        provider_id = %route.provider_id,
                        endpoint = endpoint,
                        attempt = attempt,
                        retry_budget = route.retry_budget,
                        timeout_ms = route.timeout_ms,
                        is_timeout = is_timeout,
                        is_connect = is_connect,
                        transport_error_class = transport_error_class,
                        will_retry = will_retry,
                        credential_present = bearer.is_some(),
                        error_sources = error_sources,
                        error = %error,
                    );

                    if will_retry {
                        continue;
                    }

                    return Err(map_upstream_transport_error(
                        request_id,
                        route.provider_id.clone(),
                        error,
                    ));
                }
            }
        }

        Err(LLMProxyError::upstream_unavailable(
            request_id,
            Some(route.provider_id.clone()),
            "upstream retry budget exhausted",
        ))
    }

    pub async fn execute_stream(
        &self,
        request_id: &str,
        route: &RouteResolution,
        payload: &Value,
        upstream_credential: Option<&str>,
    ) -> Result<UpstreamStreamResponse, LLMProxyError> {
        let endpoint = build_endpoint(&route.base_url, &route.effective_upstream_path);
        let bearer = upstream_credential.or(route.api_key.as_deref());

        for attempt in 0..=route.retry_budget {
            #[cfg(feature = "llm_payload_trace")]
            self.emit_payload_trace(request_id, route, &endpoint, attempt, payload);

            let mut request = self.client.post(&endpoint);
            if let Some(token) = bearer {
                request = request.bearer_auth(token);
            }
            let response = request
                .timeout(Duration::from_millis(route.timeout_ms))
                .json(payload)
                .send()
                .await;

            match response {
                Ok(response) => {
                    if response.status().is_success() {
                        let status = to_status_code(response.status());
                        return Ok(UpstreamStreamResponse { status, body: response });
                    }

                    let status = response.status();
                    let headers = response.headers().clone();
                    let will_retry = should_retry_status(status) && attempt < route.retry_budget;
                    let base_summary = UpstreamErrorSummary::from_headers(&headers);
                    tracing::warn!(
                        component = "llm_proxy",
                        action = "upstream_status_error",
                        request_id = request_id,
                        provider_id = %route.provider_id,
                        endpoint = endpoint,
                        attempt = attempt,
                        retry_budget = route.retry_budget,
                        timeout_ms = route.timeout_ms,
                        status = status.as_u16(),
                        will_retry = will_retry,
                        credential_present = bearer.is_some(),
                        upstream_content_type = ?base_summary.content_type,
                        upstream_request_id = ?base_summary.request_id,
                    );

                    if will_retry {
                        let delay = compute_retry_delay(status, &headers, attempt);
                        if !delay.is_zero() {
                            sleep(delay).await;
                        }
                        continue;
                    }

                    let summary = summarize_upstream_error_response(response).await;
                    tracing::warn!(
                        component = "llm_proxy",
                        action = "upstream_status_error_detail",
                        request_id = request_id,
                        provider_id = %route.provider_id,
                        endpoint = endpoint,
                        attempt = attempt,
                        retry_budget = route.retry_budget,
                        timeout_ms = route.timeout_ms,
                        status = status.as_u16(),
                        credential_present = bearer.is_some(),
                        upstream_content_type = ?summary.content_type,
                        upstream_request_id = ?summary.request_id,
                        upstream_error_type = ?summary.error_type,
                        upstream_error_code = ?summary.error_code,
                        upstream_error_param = ?summary.error_param,
                        upstream_error_message_present = summary.error_message_present,
                        upstream_error_message_len = ?summary.error_message_len,
                        upstream_top_level_keys = ?summary.top_level_keys,
                        upstream_error_object_keys = ?summary.error_object_keys,
                        upstream_body_bytes = summary.body_bytes,
                        upstream_body_format = summary.body_format,
                        response_read_error = ?summary.response_read_error,
                    );

                    return Err(map_upstream_status_error(
                        request_id,
                        route.provider_id.clone(),
                        status,
                    ));
                }
                Err(error) => {
                    let is_timeout = error.is_timeout();
                    let is_connect = error.is_connect();
                    let will_retry = attempt < route.retry_budget && (is_timeout || is_connect);
                    let transport_error_class = classify_transport_error(&error);
                    let error_sources = format_error_sources(&error);
                    tracing::warn!(
                        component = "llm_proxy",
                        action = "upstream_transport_error",
                        request_id = request_id,
                        provider_id = %route.provider_id,
                        endpoint = endpoint,
                        attempt = attempt,
                        retry_budget = route.retry_budget,
                        timeout_ms = route.timeout_ms,
                        is_timeout = is_timeout,
                        is_connect = is_connect,
                        transport_error_class = transport_error_class,
                        will_retry = will_retry,
                        credential_present = bearer.is_some(),
                        error_sources = error_sources,
                        error = %error,
                    );

                    if will_retry {
                        continue;
                    }

                    return Err(map_upstream_transport_error(
                        request_id,
                        route.provider_id.clone(),
                        error,
                    ));
                }
            }
        }

        Err(LLMProxyError::upstream_unavailable(
            request_id,
            Some(route.provider_id.clone()),
            "upstream retry budget exhausted",
        ))
    }

    #[cfg(feature = "llm_payload_trace")]
    fn emit_payload_trace(
        &self,
        request_id: &str,
        route: &RouteResolution,
        endpoint: &str,
        attempt: u8,
        payload: &Value,
    ) {
        if let Some(sink) = self.payload_trace_sink.as_ref() {
            sink.emit_request_payload(request_id, &route.provider_id, endpoint, attempt, payload);
        }
    }

    #[cfg(feature = "llm_payload_trace")]
    pub fn emit_response_trace(
        &self,
        request_id: &str,
        route: &RouteResolution,
        endpoint: &str,
        payload: &Value,
    ) {
        if let Some(sink) = self.payload_trace_sink.as_ref() {
            sink.emit_response_payload(request_id, &route.provider_id, endpoint, payload);
        }
    }
}

fn apply_standard_tls_env(
    mut builder: reqwest::ClientBuilder,
) -> Result<reqwest::ClientBuilder, String> {
    let Some(path_raw) = env::var_os("SSL_CERT_FILE") else {
        return Ok(builder);
    };
    let path = path_raw.to_string_lossy().trim().to_string();
    if path.is_empty() {
        return Ok(builder);
    }

    let pem = fs::read(&path)
        .map_err(|error| format!("cannot read SSL_CERT_FILE '{}': {error}", path))?;
    let certificates = parse_pem_certificates(&pem);
    if certificates.is_empty() {
        return Err(format!(
            "SSL_CERT_FILE '{}' does not contain any PEM certificate blocks",
            path
        ));
    }

    for certificate_pem in &certificates {
        let certificate = reqwest::Certificate::from_pem(certificate_pem.as_bytes()).map_err(
            |error| format!("invalid certificate in SSL_CERT_FILE '{}': {error}", path),
        )?;
        builder = builder.add_root_certificate(certificate);
    }

    tracing::info!(
        component = "llm_proxy",
        action = "upstream_tls_custom_ca_loaded",
        source = "SSL_CERT_FILE",
        ssl_cert_file = %path,
        cert_count = certificates.len(),
    );

    Ok(builder)
}

fn parse_pem_certificates(pem: &[u8]) -> Vec<String> {
    let content = String::from_utf8_lossy(pem);
    let begin_marker = "-----BEGIN CERTIFICATE-----";
    let end_marker = "-----END CERTIFICATE-----";
    let mut certificates = Vec::new();
    let mut offset = 0usize;

    while let Some(begin) = content[offset..].find(begin_marker) {
        let start = offset + begin;
        let search_from = start + begin_marker.len();
        let Some(end_rel) = content[search_from..].find(end_marker) else {
            break;
        };
        let end = search_from + end_rel + end_marker.len();
        certificates.push(content[start..end].to_string());
        offset = end;
    }

    certificates
}

fn build_endpoint(base_url: &str, upstream_path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), upstream_path)
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    matches!(status, reqwest::StatusCode::TOO_MANY_REQUESTS) || status.is_server_error()
}

fn compute_retry_delay(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    attempt: u8,
) -> Duration {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        if let Some(delay) = parse_retry_after(headers) {
            return delay;
        }
    }

    let multiplier = 1u64 << u32::from(attempt.min(5));
    Duration::from_millis(100 * multiplier)
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let seconds = raw.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds.min(30)))
}

fn map_upstream_status_error(
    request_id: &str,
    provider_id: String,
    status: reqwest::StatusCode,
) -> LLMProxyError {
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        return LLMProxyError::upstream_unavailable_with_status(
            request_id,
            Some(provider_id),
            Some(status.as_u16()),
            "upstream provider is unavailable",
        );
    }

    LLMProxyError::upstream_error_with_status(
        request_id,
        Some(provider_id),
        Some(status.as_u16()),
        format!("upstream returned status {}", status.as_u16()),
    )
}

fn map_upstream_transport_error(
    request_id: &str,
    provider_id: String,
    error: reqwest::Error,
) -> LLMProxyError {
    if error.is_timeout() || error.is_connect() {
        return LLMProxyError::upstream_unavailable(
            request_id,
            Some(provider_id),
            "upstream request timed out or connection failed",
        );
    }

    LLMProxyError::upstream_error(
        request_id,
        Some(provider_id),
        format!("upstream request failed: {error}"),
    )
}

fn to_status_code(status: reqwest::StatusCode) -> StatusCode {
    StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY)
}

fn classify_transport_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    if error.is_connect() {
        return "connect";
    }
    if error.is_request() {
        return "request";
    }
    if error.is_body() {
        return "body";
    }
    if error.is_decode() {
        return "decode";
    }
    "unknown"
}

fn format_error_sources(error: &reqwest::Error) -> String {
    use std::error::Error as _;

    let mut chain = Vec::new();
    let mut current = error.source();
    while let Some(source) = current {
        chain.push(source.to_string());
        current = source.source();
    }

    if chain.is_empty() {
        "none".to_string()
    } else {
        chain.join(" | caused_by: ")
    }
}

#[derive(Debug, Clone, Default)]
struct UpstreamErrorSummary {
    content_type: Option<String>,
    request_id: Option<String>,
    error_type: Option<String>,
    error_code: Option<String>,
    error_param: Option<String>,
    error_message_present: bool,
    error_message_len: Option<usize>,
    top_level_keys: Vec<String>,
    error_object_keys: Vec<String>,
    body_bytes: usize,
    body_format: &'static str,
    response_read_error: Option<String>,
}

impl UpstreamErrorSummary {
    fn from_headers(headers: &reqwest::header::HeaderMap) -> Self {
        Self {
            content_type: headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            request_id: headers
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            body_format: "unknown",
            ..Self::default()
        }
    }
}

async fn summarize_upstream_error_response(response: reqwest::Response) -> UpstreamErrorSummary {
    let headers = response.headers().clone();
    let mut summary = UpstreamErrorSummary::from_headers(&headers);

    let body = match response.bytes().await {
        Ok(body) => body,
        Err(error) => {
            summary.response_read_error = Some(error.to_string());
            return summary;
        }
    };

    summary.body_bytes = body.len();
    if body.is_empty() {
        summary.body_format = "empty";
        return summary;
    }

    let parsed_json = serde_json::from_slice::<Value>(&body);
    let Ok(json) = parsed_json else {
        summary.body_format = "non_json";
        return summary;
    };
    summary.body_format = "json";

    let error_root = json.get("error").unwrap_or(&json);
    if let Value::Object(object) = &json {
        summary.top_level_keys = object.keys().cloned().collect();
    }
    if let Value::Object(object) = error_root {
        summary.error_object_keys = object.keys().cloned().collect();
    }

    summary.error_type = extract_string_field(error_root, &["type", "error_type", "reason"])
        .or_else(|| extract_string_field(&json, &["type", "error_type", "reason"]));
    summary.error_code = extract_string_or_number_field(error_root, &["code", "error_code"])
        .or_else(|| extract_string_or_number_field(&json, &["code", "error_code", "status"]));
    summary.error_param = extract_string_field(error_root, &["param", "error_param", "field"])
        .or_else(|| extract_string_field(&json, &["param", "error_param", "field"]));
    let message = extract_string_field(error_root, &["message", "error_description", "detail"])
        .or_else(|| extract_string_field(&json, &["message", "error_description", "detail"]));
    summary.error_message_present = message.is_some();
    summary.error_message_len = message.as_ref().map(|value| value.len());
    if summary.request_id.is_none() {
        summary.request_id = extract_string_field(&json, &["request_id", "id"]);
    }

    summary
}

fn extract_string_field(root: &Value, field_names: &[&str]) -> Option<String> {
    let Value::Object(object) = root else {
        return None;
    };

    for field in field_names {
        if let Some(Value::String(value)) = object.get(*field) {
            if !value.trim().is_empty() {
                return Some(value.clone());
            }
        }
    }

    None
}

fn extract_string_or_number_field(root: &Value, field_names: &[&str]) -> Option<String> {
    let Value::Object(object) = root else {
        return None;
    };

    for field in field_names {
        match object.get(*field) {
            Some(Value::String(value)) if !value.trim().is_empty() => return Some(value.clone()),
            Some(Value::Number(number)) => return Some(number.to_string()),
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    use super::compute_retry_delay;

    #[test]
    fn retry_after_header_controls_429_delay() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));

        let delay = compute_retry_delay(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, 0);
        assert_eq!(delay, std::time::Duration::from_secs(2));
    }

    #[test]
    fn backoff_is_applied_when_retry_after_is_missing() {
        let headers = HeaderMap::new();

        let delay = compute_retry_delay(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, 1);
        assert_eq!(delay, std::time::Duration::from_millis(200));
    }
}
