#[derive(Debug, Clone)]
pub struct McpAuditEvent {
    pub request_id: String,
    pub server_id: String,
    pub tool_id: String,
    pub profile_id: String,
    pub final_action: &'static str,
    pub rule_hits_total: u32,
    pub blocked: bool,
    pub upstream_status: Option<u16>,
    pub duration_ms: u64,
    pub auth_mode: &'static str,
    pub credential_origin: &'static str,
}

impl McpAuditEvent {
    pub fn emit(&self) {
        tracing::info!(
            component = "mcp_proxy",
            action = "tool_call_completed",
            request_id = %self.request_id,
            server = %self.server_id,
            tool = %self.tool_id,
            profile = %self.profile_id,
            final_action = %self.final_action,
            rule_hits_total = self.rule_hits_total,
            blocked = self.blocked,
            upstream_status = ?self.upstream_status,
            duration_ms = self.duration_ms,
            auth_mode = self.auth_mode,
            credential_origin = self.credential_origin,
            "mcp tool call completed"
        );
    }
}

#[derive(Debug, Clone)]
pub struct McpAuthStageAuditEvent {
    pub request_id: String,
    pub auth_mode: &'static str,
    pub stage: &'static str,
    pub decision: &'static str,
}

impl McpAuthStageAuditEvent {
    pub fn emit(&self) {
        tracing::info!(
            component = "mcp_proxy",
            action = "auth_stage",
            request_id = %self.request_id,
            auth_mode = self.auth_mode,
            stage = self.stage,
            decision = self.decision
        );
    }
}

#[derive(Debug, Clone)]
pub struct McpRateLimitAuditEvent {
    pub request_id: String,
    pub profile_id: String,
    pub decision: String,
    pub retry_after_ms: u64,
    pub limit: u32,
    pub remaining: u32,
    pub reset_at_unix_ms: u64,
}

impl McpRateLimitAuditEvent {
    pub fn emit(&self) {
        tracing::info!(
            component = "mcp_proxy",
            action = "rate_limit_decision",
            request_id = %self.request_id,
            profile_id = %self.profile_id,
            decision = %self.decision,
            retry_after_ms = self.retry_after_ms,
            limit = self.limit,
            remaining = self.remaining,
            reset_at_unix_ms = self.reset_at_unix_ms
        );
    }
}
