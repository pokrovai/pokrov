use std::time::Duration;

use serde_json::Value;

use crate::{
    errors::McpProxyError,
    types::{McpToolResultEnvelope, McpUpstreamRequestContext},
};

pub const MCP_TOOL_CALL_UPSTREAM_PATH: &str = "/tool-call";
const SYSTEM_REQUEST_ID: &str = "system";
const SYSTEM_SERVER_ID: &str = "mcp_upstream_client";
const SYSTEM_TOOL_ID: &str = "client_init";

#[derive(Debug, Clone)]
pub struct McpUpstreamClient {
    client: reqwest::Client,
}

impl McpUpstreamClient {
    pub fn new() -> Result<Self, McpProxyError> {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(16)
            .build()
            .map_err(|error| {
                McpProxyError::upstream_unavailable(
                    SYSTEM_REQUEST_ID,
                    SYSTEM_SERVER_ID,
                    SYSTEM_TOOL_ID,
                    format!("failed to initialize upstream client: {error}"),
                )
            })?;

        Ok(Self { client })
    }

    pub async fn execute_tool_call(
        &self,
        context: &McpUpstreamRequestContext,
        arguments: &Value,
    ) -> Result<McpToolResultEnvelope, McpProxyError> {
        let endpoint = build_endpoint(&context.endpoint);
        let response = self
            .client
            .post(endpoint)
            .timeout(Duration::from_millis(context.timeout_ms))
            .json(&serde_json::json!({
                "request_id": context.request_id,
                "tool": context.tool_id,
                "arguments": arguments,
            }))
            .send()
            .await
            .map_err(|error| map_transport_error(context, error))?;

        if !response.status().is_success() {
            return Err(map_status_error(context, response.status()));
        }

        let payload = response.json::<Value>().await.map_err(|error| {
            McpProxyError::upstream_error(
                &context.request_id,
                &context.server_id,
                &context.tool_id,
                format!("upstream returned invalid JSON: {error}"),
            )
        })?;

        normalize_result(payload).map_err(|message| {
            McpProxyError::upstream_error(
                &context.request_id,
                &context.server_id,
                &context.tool_id,
                message,
            )
        })
    }
}

fn build_endpoint(base_url: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), MCP_TOOL_CALL_UPSTREAM_PATH)
}

fn normalize_result(payload: Value) -> Result<McpToolResultEnvelope, String> {
    match payload {
        Value::Object(object) => {
            if let Some(result) = object.get("result") {
                if let Some(result_object) = result.as_object() {
                    let content = result_object
                        .get("content")
                        .cloned()
                        .ok_or_else(|| "upstream result.content is required".to_string())?;
                    let content_type = result_object
                        .get("content_type")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let truncated = result_object
                        .get("truncated")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);

                    return Ok(McpToolResultEnvelope {
                        content,
                        content_type,
                        truncated,
                    });
                }
            }

            let content = object
                .get("content")
                .cloned()
                .unwrap_or_else(|| Value::Object(object.clone()));
            let content_type = object
                .get("content_type")
                .and_then(Value::as_str)
                .map(str::to_string);
            let truncated = object
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            Ok(McpToolResultEnvelope {
                content,
                content_type,
                truncated,
            })
        }
        _ => Ok(McpToolResultEnvelope {
            content: payload,
            content_type: None,
            truncated: false,
        }),
    }
}

fn map_status_error(
    context: &McpUpstreamRequestContext,
    status: reqwest::StatusCode,
) -> McpProxyError {
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        return McpProxyError::upstream_unavailable_with_status(
            &context.request_id,
            &context.server_id,
            &context.tool_id,
            Some(status.as_u16()),
            "upstream MCP server is unavailable",
        );
    }

    McpProxyError::upstream_error_with_status(
        &context.request_id,
        &context.server_id,
        &context.tool_id,
        Some(status.as_u16()),
        format!("upstream returned status {}", status.as_u16()),
    )
}

fn map_transport_error(context: &McpUpstreamRequestContext, error: reqwest::Error) -> McpProxyError {
    if error.is_timeout() || error.is_connect() {
        return McpProxyError::upstream_unavailable(
            &context.request_id,
            &context.server_id,
            &context.tool_id,
            "upstream request timed out or connection failed",
        );
    }

    McpProxyError::upstream_error(
        &context.request_id,
        &context.server_id,
        &context.tool_id,
        format!("upstream request failed: {error}"),
    )
}
