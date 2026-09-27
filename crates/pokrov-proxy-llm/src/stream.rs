use pokrov_core::{
    rehydrate::{
        event_boundary_end, EventRehydrator, RehydrateReport, RehydrationMap,
        MAX_EVENT_BUFFER_BYTES,
    },
    types::{EvaluateRequest, EvaluationMode, PathClass, PolicyAction},
    SanitizationEngine,
};
use serde_json::Value;

use crate::errors::LLMProxyError;

#[derive(Debug, Clone)]
pub struct StreamSanitizationResult {
    pub body: String,
    pub rule_hits_total: u32,
    pub final_action: PolicyAction,
}

pub fn sanitize_sse_stream(
    request_id: &str,
    profile_id: &str,
    raw_body: &str,
    evaluator: &SanitizationEngine,
) -> Result<StreamSanitizationResult, LLMProxyError> {
    let mut events = Vec::new();
    let mut total_hits = 0u32;
    let mut final_action = PolicyAction::Allow;

    for event in raw_body.split("\n\n") {
        if event.trim().is_empty() {
            continue;
        }

        let mut lines = Vec::new();
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                let payload = data.trim();
                if payload == "[DONE]" {
                    lines.push("data: [DONE]".to_string());
                    continue;
                }

                let Ok(event_json) = serde_json::from_str::<Value>(payload) else {
                    lines.push(line.to_string());
                    continue;
                };

                let result = evaluator
                    .evaluate(EvaluateRequest {
                        request_id: request_id.to_string(),
                        profile_id: profile_id.to_string(),
                        mode: EvaluationMode::Enforce,
                        payload: event_json,
                        path_class: PathClass::Llm,
                        effective_language: "en".to_string(),
                        entity_scope_filters: Vec::new(),
                        recognizer_family_filters: Vec::new(),
                        allowlist_additions: Vec::new(),
                    })
                    .map_err(|error| {
                        LLMProxyError::invalid_request(
                            request_id,
                            format!("failed to sanitize stream event: {error}"),
                        )
                    })?;

                total_hits = total_hits.saturating_add(result.decision.rule_hits_total);
                if result.decision.final_action.strictness_rank() > final_action.strictness_rank() {
                    final_action = result.decision.final_action;
                }

                let sanitized = result.transform.sanitized_payload.ok_or_else(|| {
                    LLMProxyError::policy_blocked(
                        request_id,
                        "stream output blocked by active profile policy",
                    )
                })?;

                let encoded = serde_json::to_string(&sanitized).map_err(|error| {
                    LLMProxyError::upstream_error(
                        request_id,
                        None,
                        format!("failed to serialize sanitized stream event: {error}"),
                    )
                })?;

                lines.push(format!("data: {encoded}"));
                continue;
            }

            lines.push(line.to_string());
        }

        events.push(lines.join("\n"));
    }

    let mut body = events.join("\n\n");
    if !body.is_empty() {
        body.push_str("\n\n");
    }

    Ok(StreamSanitizationResult { body, rule_hits_total: total_hits, final_action })
}

/// Restores `[PKV_TOKEN]` pseudonyms in a fully buffered SSE body. Runs after
/// output policy evaluation: `data:` payloads that parse as JSON are restored
/// leaf-wise so fragments containing JSON-significant bytes stay escaped,
/// `delta.content` values join a cross-event carry so a token split between
/// two delta events resolves, and non-JSON lines restore as plain text under
/// a newline guard that keeps SSE framing intact.
pub fn rehydrate_sse_stream(raw_body: &str, map: &RehydrationMap) -> (String, RehydrateReport) {
    if map.is_empty() {
        return (raw_body.to_string(), RehydrateReport::default());
    }

    let mut rehydrator = EventRehydrator::new(map.clone());
    let mut events = Vec::new();

    for event in raw_body.split("\n\n") {
        if event.trim().is_empty() {
            continue;
        }
        events.push(rehydrator.rehydrate_event(event));
    }
    // A stream that ended mid-token still flushes pending carries as
    // synthetic delta events rather than dropping upstream bytes.
    events.extend(rehydrator.finish());

    let mut body = events.join("\n\n");
    if !body.is_empty() {
        body.push_str("\n\n");
    }

    let report = rehydrator.report();

    (body, report)
}

pub fn convert_chat_sse_to_responses_sse(
    request_id: &str,
    raw_body: &str,
) -> Result<String, LLMProxyError> {
    let mut events = Vec::new();

    for event in raw_body.split("\n\n") {
        if event.trim().is_empty() {
            continue;
        }

        let mut lines = Vec::new();
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                let payload = data.trim();
                if payload == "[DONE]" {
                    lines.push("data: [DONE]".to_string());
                    continue;
                }

                let Ok(event_json) = serde_json::from_str::<Value>(payload) else {
                    lines.push(line.to_string());
                    continue;
                };

                let delta = extract_text_delta(&event_json).unwrap_or_default();
                let encoded = serde_json::to_string(&serde_json::json!({
                    "type": "response.output_text.delta",
                    "delta": delta,
                    "request_id": request_id,
                }))
                .map_err(|error| {
                    LLMProxyError::upstream_error(
                        request_id,
                        None,
                        format!("failed to serialize responses stream event: {error}"),
                    )
                })?;
                lines.push(format!("data: {encoded}"));
                continue;
            }

            lines.push(line.to_string());
        }

        events.push(lines.join("\n"));
    }

    let mut body = events.join("\n\n");
    if !body.is_empty() {
        body.push_str("\n\n");
    }

    Ok(body)
}

/// Incremental chat-SSE → responses-SSE converter for the passthrough path.
/// Upstream bytes accumulate until a blank-line terminator completes an
/// event (LF/CRLF/CR endings alike, shared with the rehydrator's scanner).
/// `finish` pushes the pending tail through conversion at end of stream or
/// ahead of a forwarded error so already-received bytes are never dropped.
pub struct ResponsesChunkConverter {
    request_id: String,
    pending_bytes: Vec<u8>,
    /// Bytes before this offset are known to contain no event boundary, so
    /// `feed` resumes the terminator scan here instead of re-scanning the
    /// whole pending buffer on every chunk.
    scanned: usize,
    /// After an oversized event flushed verbatim, the remainder of that same
    /// event streams through untouched until its blank-line terminator —
    /// converting a partial event already emitted would corrupt framing.
    passthrough: bool,
}

impl ResponsesChunkConverter {
    pub fn new(request_id: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            pending_bytes: Vec::new(),
            scanned: 0,
            passthrough: false,
        }
    }

    /// Converts every complete buffered event; returns bytes safe to emit.
    /// A pending event past `MAX_EVENT_BUFFER_BYTES` flushes verbatim and
    /// the rest of it streams through untouched, so the rehydrator's bound
    /// holds end-to-end on this path too.
    pub fn feed(&mut self, incoming_chunk: &[u8]) -> Vec<u8> {
        self.pending_bytes.extend_from_slice(incoming_chunk);
        let mut out = Vec::new();
        if self.passthrough {
            match event_boundary_end(&self.pending_bytes) {
                Some(end) => {
                    out.extend(self.pending_bytes.drain(..end));
                    self.passthrough = false;
                    self.scanned = 0;
                }
                // The trailing CR/LF run stays buffered: it may pair with the
                // next chunk's leading bytes to complete the awaited
                // terminator.
                None => {
                    let keep = self
                        .pending_bytes
                        .iter()
                        .rev()
                        .take_while(|b| matches!(b, b'\r' | b'\n'))
                        .count();
                    let flush_end = self.pending_bytes.len() - keep;
                    out.extend(self.pending_bytes.drain(..flush_end));
                    self.scanned = self.pending_bytes.len().saturating_sub(1);
                    return out;
                }
            }
        }

        let mut converted = String::new();
        while let Some(rel) = event_boundary_end(&self.pending_bytes[self.scanned..]) {
            let end = self.scanned + rel;
            let event_bytes: Vec<u8> = self.pending_bytes.drain(..end).collect();
            self.scanned = 0;
            let event = String::from_utf8_lossy(event_bytes.as_slice());
            let converted_event = convert_single_chat_sse_event(&self.request_id, &event);
            if converted_event.is_empty() {
                continue;
            }
            converted.push_str(&converted_event);
            converted.push_str("\n\n");
        }
        self.scanned = self.pending_bytes.len().saturating_sub(1);
        out.extend_from_slice(converted.as_bytes());
        if self.pending_bytes.len() > MAX_EVENT_BUFFER_BYTES {
            let keep = self
                .pending_bytes
                .iter()
                .rev()
                .take_while(|b| matches!(b, b'\r' | b'\n'))
                .count();
            let flush_end = self.pending_bytes.len() - keep;
            out.extend(self.pending_bytes.drain(..flush_end));
            self.scanned = self.pending_bytes.len().saturating_sub(1);
            self.passthrough = true;
        }
        out
    }

    /// Flushes the pending tail: a complete trailing event still converts,
    /// malformed or oversized leftovers pass through verbatim.
    pub fn finish(&mut self) -> Vec<u8> {
        let tail: Vec<u8> = self.pending_bytes.drain(..).collect();
        self.scanned = 0;
        if tail.is_empty() {
            return Vec::new();
        }
        if self.passthrough {
            return tail;
        }
        let event = String::from_utf8_lossy(tail.as_slice());
        let converted = convert_single_chat_sse_event(&self.request_id, &event);
        let mut out = converted.into_bytes();
        if !out.is_empty() {
            out.extend_from_slice(b"\n\n");
        }
        out
    }
}

fn convert_single_chat_sse_event(request_id: &str, event: &str) -> String {
    if event.trim().is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            let payload = data.trim();
            if payload == "[DONE]" {
                lines.push("data: [DONE]".to_string());
                continue;
            }

            let Ok(event_json) = serde_json::from_str::<Value>(payload) else {
                lines.push(line.to_string());
                continue;
            };

            let delta = extract_text_delta(&event_json).unwrap_or_default();
            let encoded = serde_json::json!({
                "type": "response.output_text.delta",
                "delta": delta,
                "request_id": request_id,
            })
            .to_string();
            lines.push(format!("data: {encoded}"));
            continue;
        }

        lines.push(line.to_string());
    }

    lines.join("\n")
}

fn extract_text_delta(value: &Value) -> Option<String> {
    let choices = value.get("choices")?.as_array()?;
    let mut merged = String::new();
    for choice in choices {
        if let Some(text) =
            choice.get("delta").and_then(|delta| delta.get("content")).and_then(Value::as_str)
        {
            merged.push_str(text);
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pokrov_core::{
        types::{
            CategoryActions, EvaluateRequest, EvaluationMode, EvaluatorConfig, PathClass,
            PolicyAction, PolicyProfile,
        },
        SanitizationEngine,
    };

    use pokrov_core::rehydrate::MAX_EVENT_BUFFER_BYTES;

    use super::{
        convert_chat_sse_to_responses_sse, sanitize_sse_stream, ResponsesChunkConverter,
    };

    fn engine() -> SanitizationEngine {
        let strict = PolicyProfile {
            profile_id: "strict".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Redact,
                pii: PolicyAction::Redact,
                corporate_markers: PolicyAction::Redact,
                custom: PolicyAction::Redact,
            },
            mask_visible_suffix: 4,
            max_hits_per_request: 4096,
            custom_rules: Vec::new(),
            custom_rules_enabled: false,
            ner_enabled: false,
        };

        SanitizationEngine::new(EvaluatorConfig {
            default_profile: "strict".to_string(),
            rehydration_key: None,
            profiles: BTreeMap::from([("strict".to_string(), strict)]),
        })
        .expect("engine should build")
    }

    #[test]
    fn preserves_done_frame_and_sanitizes_json_events() {
        let stream = "data: {\"delta\":\"token sk-test-12345678\"}\n\ndata: [DONE]\n\n";
        let result = sanitize_sse_stream("req-1", "strict", stream, &engine())
            .expect("stream should sanitize");

        assert!(result.body.contains("[DONE]"));
        assert!(result.body.contains("[REDACTED]") || result.body.contains('*'));
    }

    #[test]
    fn sanitizer_uses_llm_path_class() {
        let eval = engine()
            .evaluate(EvaluateRequest {
                request_id: "req-2".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload: serde_json::json!({"text": "hello"}),
                path_class: PathClass::Llm,
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("evaluation should succeed");

        assert_eq!(eval.audit.path_class, PathClass::Llm);
    }

    #[test]
    fn converts_chat_sse_events_into_responses_delta_events() {
        let stream =
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n";
        let converted = convert_chat_sse_to_responses_sse("req-1", stream)
            .expect("stream conversion should succeed");

        assert!(converted.contains("\"type\":\"response.output_text.delta\""));
        assert!(converted.contains("\"delta\":\"hello\""));
        assert!(converted.contains("\"request_id\":\"req-1\""));
        assert!(converted.contains("data: [DONE]"));
    }

    #[test]
    fn preserves_non_json_sse_chunks_during_conversion() {
        let stream = "data: {malformed-json}\n\ndata: [DONE]\n\n";
        let converted = convert_chat_sse_to_responses_sse("req-2", stream)
            .expect("stream conversion should preserve malformed chunks");
        assert!(converted.contains("data: {malformed-json}"));
        assert!(converted.contains("data: [DONE]"));
    }

    #[test]
    fn converts_responses_stream_chunk_by_chunk_across_boundaries() {
        let mut converter = ResponsesChunkConverter::new("req-3");
        let first =
            converter.feed(br#"data: {"choices":[{"delta":{"content":"he"}}]"#);
        assert!(first.is_empty());

        let second = converter.feed(b"}\n\ndata: [DONE]\n\n");

        let converted = String::from_utf8(second).expect("converted chunk should be utf-8");
        assert!(converted.contains("\"type\":\"response.output_text.delta\""));
        assert!(converted.contains("\"delta\":\"he\""));
        assert!(converted.contains("\"request_id\":\"req-3\""));
        assert!(converted.contains("data: [DONE]"));
        assert!(converter.finish().is_empty());
    }

    #[test]
    fn converter_passthrough_keeps_split_boundary() {
        // An oversized event whose terminator is split across the flush must
        // not swallow the following event into passthrough — the next chat
        // event still converts into responses format.
        for (first_eol, rest_eol) in [("\n", "\n"), ("\r\n", "\r\n"), ("\r", "\r"), ("\r\n\r", "\n")]
        {
            let mut converter = ResponsesChunkConverter::new("req-split");
            let mut flood = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 1];
            flood.extend_from_slice(first_eol.as_bytes());
            let out = converter.feed(&flood);
            assert!(out.len() < flood.len(), "trailing EOL prefix retained");

            converter.feed(
                format!("{rest_eol}data: {{\"choices\":[{{\"delta\":{{\"content\":\"hi\"}}}}]}}")
                    .as_bytes(),
            );
            let out = converter.feed(b"\n\n");
            let text = String::from_utf8(out).expect("converted output should be utf-8");
            assert!(
                text.contains("\"type\":\"response.output_text.delta\""),
                "terminator {first_eol:?}{rest_eol:?}: {text}"
            );
            assert!(converter.finish().is_empty());
        }
    }

    #[test]
    fn converter_flushes_pending_tail_at_stream_end() {
        let mut converter = ResponsesChunkConverter::new("req-4");
        // Upstream dies mid-event: the partial bytes must not be dropped.
        converter.feed(br#"data: {"choices":[{"delta":{"content":"trun"#);

        let tail = converter.finish();
        let text = String::from_utf8(tail).expect("tail should be utf-8");
        assert!(text.contains("trun"), "tail must preserve received bytes: {text}");
    }
}
