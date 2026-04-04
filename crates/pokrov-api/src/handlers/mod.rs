pub mod chat_completions;
pub mod evaluate;
pub mod health;
pub mod mcp_tool_call;
pub mod metrics;
pub mod models;
pub mod ready;
mod request_context;
pub mod responses;
mod rate_limit;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReadyChecks {
    pub config: &'static str,
    pub auth: &'static str,
    pub policy: &'static str,
    pub llm: &'static str,
    pub mcp: &'static str,
    pub runtime: &'static str,
    pub active_requests: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub request_id: String,
    pub checks: ReadyChecks,
}
