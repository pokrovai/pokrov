//! Buffered Chat SSE output-policy pass.
//!
//! Policy evaluation rewrites every parseable `data:` line, which can hide
//! source-level duplicate members or shrink an oversized frame below the
//! streaming event budget. The sanitizer records every post-policy byte range
//! of an affected complete frame so restoration can bypass it with the
//! matching carry semantics.

use std::ops::Range;

use pokrov_core::{
    rehydrate::{event_boundary_end, has_ambiguous_chat_tool_members, MAX_EVENT_BUFFER_BYTES},
    types::{EvaluateRequest, EvaluationMode, PathClass, PolicyAction},
    SanitizationEngine,
};
use serde_json::Value;

use crate::errors::LLMProxyError;

use super::restore::BarrierKind;

/// Output policy result for one buffered Chat SSE body. `source_barriers` is
/// out-of-band metadata, never serialized into the SSE payload.
pub(crate) struct ChatPolicyBody {
    pub(crate) body: String,
    pub(crate) source_barriers: Vec<(Range<usize>, BarrierKind)>,
    pub(crate) rule_hits_total: u32,
    pub(crate) final_action: PolicyAction,
}

/// Result of evaluating one already-parsed JSON payload.
pub(crate) struct PolicyPayloadOutcome {
    pub(crate) encoded: String,
    pub(crate) rule_hits_total: u32,
    pub(crate) final_action: PolicyAction,
}

/// Evaluates one JSON `data:` payload and re-encodes the transformed value.
/// Shared by the default and Chat buffered sanitizers so their evaluation
/// request, error mapping, and result aggregation cannot drift apart.
pub(crate) fn evaluate_policy_payload(
    request_id: &str,
    profile_id: &str,
    payload: Value,
    evaluator: &SanitizationEngine,
) -> Result<PolicyPayloadOutcome, LLMProxyError> {
    let result = evaluator
        .evaluate(EvaluateRequest {
            request_id: request_id.to_string(),
            profile_id: profile_id.to_string(),
            mode: EvaluationMode::Enforce,
            payload,
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

    let sanitized = result.transform.sanitized_payload.ok_or_else(|| {
        LLMProxyError::policy_blocked(request_id, "stream output blocked by active profile policy")
    })?;

    let encoded = serde_json::to_string(&sanitized).map_err(|error| {
        LLMProxyError::upstream_error(
            request_id,
            None,
            format!("failed to serialize sanitized stream event: {error}"),
        )
    })?;

    Ok(PolicyPayloadOutcome {
        encoded,
        rule_hits_total: result.decision.rule_hits_total,
        final_action: result.decision.final_action,
    })
}

/// Applies output policy to a buffered Chat SSE body while preserving every
/// physical frame, line ending, and data-line count. Only the payload after
/// `data:` is re-encoded; a raw tail without a blank-line delimiter stays a
/// raw tail instead of gaining a synthetic terminator.
pub(crate) fn sanitize_chat_sse_stream(
    request_id: &str,
    profile_id: &str,
    raw_body: &str,
    evaluator: &SanitizationEngine,
) -> Result<ChatPolicyBody, LLMProxyError> {
    // A UTF-8 BOM is forwarded verbatim but ignored for first-line parsing
    // so a leading `data:` field is still policy-evaluated and restorable.
    let (bom, content) = split_initial_bom(raw_body);
    let bytes = content.as_bytes();
    let mut body = String::with_capacity(raw_body.len());
    body.push_str(bom);
    let mut source_barriers = Vec::new();
    let mut rule_hits_total = 0u32;
    let mut final_action = PolicyAction::Allow;
    let mut cursor = 0usize;

    while let Some(offset) = event_boundary_end(&bytes[cursor..]) {
        let end = cursor + offset;
        let frame = &content[cursor..end];
        // The first frame starts at byte zero in the emitted body: a forwarded
        // BOM belongs to that frame, not to a separate prefix.
        let frame_start = if cursor == 0 { 0 } else { body.len() };
        let outcome = sanitize_chat_frame(request_id, profile_id, frame, evaluator, &mut body)?;
        rule_hits_total = rule_hits_total.saturating_add(outcome.rule_hits_total);
        if outcome.final_action.strictness_rank() > final_action.strictness_rank() {
            final_action = outcome.final_action;
        }
        let barrier_kind = if frame_exceeds_event_budget(frame) {
            Some(BarrierKind::Oversize)
        } else if outcome.source_ambiguous {
            Some(BarrierKind::Ambiguous)
        } else {
            None
        };
        if let Some(kind) = barrier_kind {
            source_barriers.push((frame_start..body.len(), kind));
        }
        cursor = end;
    }

    if cursor < content.len() {
        // A raw tail is evaluated like any other parseable data line but is
        // never re-terminated: the stream restorer must still see it as an
        // incomplete frame and forward it verbatim last.
        let outcome =
            sanitize_chat_frame(request_id, profile_id, &content[cursor..], evaluator, &mut body)?;
        rule_hits_total = rule_hits_total.saturating_add(outcome.rule_hits_total);
        if outcome.final_action.strictness_rank() > final_action.strictness_rank() {
            final_action = outcome.final_action;
        }
    }

    Ok(ChatPolicyBody { body, source_barriers, rule_hits_total, final_action })
}

struct ChatFrameOutcome {
    source_ambiguous: bool,
    rule_hits_total: u32,
    final_action: PolicyAction,
}

fn sanitize_chat_frame(
    request_id: &str,
    profile_id: &str,
    frame: &str,
    evaluator: &SanitizationEngine,
    body: &mut String,
) -> Result<ChatFrameOutcome, LLMProxyError> {
    let mut source_ambiguous = false;
    let mut rule_hits_total = 0u32;
    let mut final_action = PolicyAction::Allow;
    let mut start = 0usize;

    while start < frame.len() {
        let (line, ending, next) = next_frame_line(frame, start);
        if let Some(data) = line.strip_prefix("data:") {
            let payload = data.trim();
            if payload != "[DONE]" {
                // Probe the source bytes first: a regular `Value` parse
                // discards duplicate members, which is exactly the ambiguity
                // a re-encode would hide from the stream restorer.
                let ambiguous = has_ambiguous_chat_tool_members(payload);
                if let Ok(event_json) = serde_json::from_str::<Value>(payload) {
                    if ambiguous {
                        source_ambiguous = true;
                    }
                    let outcome =
                        evaluate_policy_payload(request_id, profile_id, event_json, evaluator)?;
                    rule_hits_total = rule_hits_total.saturating_add(outcome.rule_hits_total);
                    if outcome.final_action.strictness_rank() > final_action.strictness_rank() {
                        final_action = outcome.final_action;
                    }
                    body.push_str("data: ");
                    body.push_str(&outcome.encoded);
                    body.push_str(ending);
                    start = next;
                    continue;
                }
            }
        }

        body.push_str(line);
        body.push_str(ending);
        start = next;
    }

    Ok(ChatFrameOutcome { source_ambiguous, rule_hits_total, final_action })
}

/// Splits one optional UTF-8 BOM from the very start of a body. The BOM is
/// forwarded byte-for-byte but must not hide the first SSE field from parsing.
fn split_initial_bom(body: &str) -> (&str, &str) {
    if let Some(rest) = body.strip_prefix('\u{feff}') {
        ("\u{feff}", rest)
    } else {
        ("", body)
    }
}

/// Splits one physical line from `start`, returning the content, its exact
/// terminator (LF, CRLF, or lone CR), and the next line offset.
fn next_frame_line(frame: &str, start: usize) -> (&str, &str, usize) {
    let bytes = frame.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        let end = match bytes[index] {
            b'\n' => index + 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index + 2,
            b'\r' => index + 1,
            _ => {
                index += 1;
                continue;
            }
        };
        return (&frame[start..index], &frame[index..end], end);
    }
    (&frame[start..], "", bytes.len())
}

/// Mirrors the streaming event budget: the delimiter EOL run is excluded so a
/// terminator split across transport chunks cannot change admissibility.
fn frame_exceeds_event_budget(frame: &str) -> bool {
    frame.trim_end_matches(['\r', '\n']).len() > MAX_EVENT_BUFFER_BYTES
}
