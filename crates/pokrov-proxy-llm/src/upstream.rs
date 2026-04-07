use std::time::Duration;

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
        let client =
            reqwest::Client::builder().pool_max_idle_per_host(16).build().map_err(|error| {
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

                    if should_retry_status(response.status()) && attempt < route.retry_budget {
                        let delay =
                            compute_retry_delay(response.status(), response.headers(), attempt);
                        if !delay.is_zero() {
                            sleep(delay).await;
                        }
                        continue;
                    }

                    return Err(map_upstream_status_error(
                        request_id,
                        route.provider_id.clone(),
                        response.status(),
                    ));
                }
                Err(error) => {
                    if attempt < route.retry_budget && (error.is_timeout() || error.is_connect()) {
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

                    if should_retry_status(response.status()) && attempt < route.retry_budget {
                        let delay =
                            compute_retry_delay(response.status(), response.headers(), attempt);
                        if !delay.is_zero() {
                            sleep(delay).await;
                        }
                        continue;
                    }

                    return Err(map_upstream_status_error(
                        request_id,
                        route.provider_id.clone(),
                        response.status(),
                    ));
                }
                Err(error) => {
                    if attempt < route.retry_budget && (error.is_timeout() || error.is_connect()) {
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
