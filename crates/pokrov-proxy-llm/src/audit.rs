use pokrov_core::types::PolicyAction;
use pokrov_core::util::format_unix_ms_rfc3339;
use serde::Serialize;

use crate::types::UpstreamCredentialOrigin;

#[derive(Debug, Clone, Serialize)]
pub struct LLMAuditEvent {
    pub request_id: String,
    pub endpoint: String,
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub model: String,
    pub stream: bool,
    pub final_action: PolicyAction,
    pub rule_hits_total: u32,
    pub blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    pub duration_ms: u64,
    pub estimated_token_units: u32,
    pub auth_mode: String,
    pub credential_origin: UpstreamCredentialOrigin,
}

impl LLMAuditEvent {
    pub fn emit(&self) {
        tracing::info!(
            component = "llm_proxy",
            action = "audit",
            request_id = %self.request_id,
            endpoint = %self.endpoint,
            profile_id = %self.profile_id,
            provider_id = ?self.provider_id,
            model = %self.model,
            stream = self.stream,
            final_action = ?self.final_action,
            rule_hits_total = self.rule_hits_total,
            blocked = self.blocked,
            upstream_status = ?self.upstream_status,
            duration_ms = self.duration_ms,
            estimated_token_units = self.estimated_token_units,
            auth_mode = %self.auth_mode,
            credential_origin = ?self.credential_origin
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMAuthStageAuditEvent {
    pub request_id: String,
    pub endpoint: &'static str,
    pub auth_mode: &'static str,
    pub stage: &'static str,
    pub decision: &'static str,
}

impl LLMAuthStageAuditEvent {
    pub fn emit(&self) {
        tracing::info!(
            component = "llm_proxy",
            action = "auth_stage",
            request_id = %self.request_id,
            endpoint = self.endpoint,
            auth_mode = self.auth_mode,
            stage = self.stage,
            decision = self.decision
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMRateLimitAuditEvent {
    pub request_id: String,
    pub profile_id: String,
    pub decision: String,
    pub retry_after_ms: u64,
    pub limit: u32,
    pub remaining: u32,
    pub reset_at_unix_ms: u64,
}

impl LLMRateLimitAuditEvent {
    pub fn emit(&self) {
        let reset_at_rfc3339 = format_unix_ms_rfc3339(self.reset_at_unix_ms);
        tracing::info!(
            component = "llm_proxy",
            action = "rate_limit_decision",
            request_id = %self.request_id,
            profile_id = %self.profile_id,
            decision = %self.decision,
            retry_after_ms = self.retry_after_ms,
            limit = self.limit,
            remaining = self.remaining,
            reset_at_unix_ms = self.reset_at_unix_ms,
            reset_at_rfc3339 = %reset_at_rfc3339
        );
    }
}
