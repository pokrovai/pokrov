use std::collections::BTreeMap;

use bytes::Bytes;
use futures_util::stream::BoxStream;
use http::StatusCode;
use pokrov_config::UpstreamAuthMode;
use pokrov_core::types::PolicyAction;
use serde::Serialize;
use serde_json::Value;

pub const CHAT_COMPLETIONS_UPSTREAM_PATH: &str = "/chat/completions";
pub const ALLOWED_ROLES: [&str; 4] = ["system", "user", "assistant", "tool"];
pub const RESPONSES_ENDPOINT: &str = "/v1/responses";

#[derive(Debug, Clone)]
pub struct LLMRequestEnvelope {
    pub request_id: String,
    pub model: String,
    pub messages: Vec<LLMMessage>,
    pub stream: bool,
    pub profile_hint: Option<String>,
    pub metadata_tags: BTreeMap<String, String>,
    pub original_payload: Value,
}

#[derive(Debug, Clone)]
pub struct ResponsesCompatibilityRequest {
    pub model: String,
    pub input: Value,
    pub stream: bool,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResponsesCompatibilityOutputItem {
    pub output_type: &'static str,
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct LLMMessage {
    pub role: String,
    pub content: MessageContent,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone)]
pub struct ContentBlock {
    pub block_type: String,
    pub text: Option<String>,
    pub json: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct RouteResolution {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub timeout_ms: u64,
    pub retry_budget: u8,
    pub output_sanitization: bool,
    pub stream_sanitization_max_buffer_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct UpstreamRequestContext {
    pub request_id: String,
    pub provider_id: String,
    pub endpoint: String,
    pub timeout_ms: u64,
    pub stream: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamCredentialOrigin {
    Config,
    Request,
}

#[derive(Debug, Clone)]
pub struct SelectedUpstreamCredential {
    pub token: String,
    pub origin: UpstreamCredentialOrigin,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthAuditMetadata {
    pub auth_mode: UpstreamAuthMode,
    pub credential_origin: UpstreamCredentialOrigin,
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMResponseMetadata {
    pub profile: String,
    pub sanitized_input: bool,
    pub sanitized_output: bool,
    pub action: PolicyAction,
    pub rule_hits: u32,
    pub estimated_token_units: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_token_units: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

pub enum LLMProxyBody {
    Json(Value),
    Sse(String),
    SseStream(SseBodyStream),
}

pub struct LLMProxyResponse {
    pub request_id: String,
    pub status: StatusCode,
    pub body: LLMProxyBody,
}

#[derive(Debug, Clone)]
pub struct UpstreamJsonResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub type SseBodyStream = BoxStream<'static, Result<Bytes, reqwest::Error>>;

#[derive(Debug)]
pub struct UpstreamStreamResponse {
    pub status: StatusCode,
    pub body: reqwest::Response,
}
