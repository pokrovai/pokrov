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
            "mcp tool call completed"
        );
    }
}
