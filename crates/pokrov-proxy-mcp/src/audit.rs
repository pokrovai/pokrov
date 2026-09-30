use pokrov_core::util::format_unix_ms_rfc3339;

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
    /// Metadata-only counters; original fragments and tokens are never logged.
    /// `unrestored_tokens_total` counts `__PKV_` markers left unrestored in
    /// the tool output (mutated or foreign tokens, fail-visible).
    pub tokenized_spans_total: u32,
    pub rehydrated_tokens_total: u32,
    pub unrestored_tokens_total: u32,
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
            tokenized_spans_total = self.tokenized_spans_total,
            rehydrated_tokens_total = self.rehydrated_tokens_total,
            unrestored_tokens_total = self.unrestored_tokens_total,
            "mcp tool call completed"
        );
    }
}

/// Metadata-only record of one prompt-injection evaluation. Scores are
/// emitted as buckets; inspected text and fragments are never logged.
#[derive(Debug, Clone)]
pub struct McpPromptInjectionAuditEvent {
    pub request_id: String,
    /// Flow identifier for audit consumers; always `mcp_tool_call` in v1.
    pub flow_type: &'static str,
    pub server_id: String,
    pub tool_id: String,
    pub source: &'static str,
    pub detector_id: String,
    pub provider: String,
    pub model_id: String,
    /// `None` when detection did not complete (degraded outcome).
    pub classification: Option<&'static str>,
    pub score_bucket: &'static str,
    pub threshold: f32,
    pub decision: &'static str,
    /// True when enforcing config would have blocked (dry_run visibility).
    pub would_block: bool,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub chunks_processed: u32,
    pub duration_ms: u64,
}

impl McpPromptInjectionAuditEvent {
    pub fn emit(&self) {
        tracing::info!(
            component = "mcp_proxy",
            action = "prompt_injection_evaluated",
            request_id = %self.request_id,
            flow_type = self.flow_type,
            server = %self.server_id,
            tool = %self.tool_id,
            source = self.source,
            detector_id = %self.detector_id,
            provider = %self.provider,
            model_id = %self.model_id,
            classification = ?self.classification,
            score_bucket = self.score_bucket,
            threshold = self.threshold,
            decision = self.decision,
            would_block = self.would_block,
            degraded = self.degraded,
            degraded_reason = ?self.degraded_reason,
            chunks_processed = self.chunks_processed,
            duration_ms = self.duration_ms,
            "prompt injection evaluation completed"
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
        let reset_at_rfc3339 = format_unix_ms_rfc3339(self.reset_at_unix_ms);
        tracing::info!(
            component = "mcp_proxy",
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
