//! Buffered Chat Completions policy pass and tool-argument restoration.
//!
//! The buffered path must evaluate output policy before restoration and keep
//! the trace payload pre-restore. Policy evaluation rewrites every parseable
//! `data:` line, which can hide source-level duplicate members or shrink an
//! oversized frame below the streaming event budget. The sanitizer therefore
//! records the first post-policy byte range of an affected complete frame;
//! restoration later bypasses that frame, which flushes every pending
//! argument suffix and permanently closes tool admission.

use std::ops::Range;

use pokrov_core::{
    rehydrate::{
        event_boundary_end, has_ambiguous_chat_tool_members, RehydrateReport, RehydrationMap,
        SseStreamRehydrator, MAX_EVENT_BUFFER_BYTES,
    },
    types::{EvaluateRequest, EvaluationMode, PathClass, PolicyAction},
    SanitizationEngine,
};
use serde_json::Value;

use crate::errors::LLMProxyError;

/// Output policy result for one buffered Chat SSE body. `first_source_barrier`
/// is out-of-band metadata, never serialized into the SSE payload.
pub(crate) struct ChatPolicyBody {
    pub(crate) body: String,
    pub(crate) first_source_barrier: Option<Range<usize>>,
    pub(crate) rule_hits_total: u32,
    pub(crate) final_action: PolicyAction,
}

/// Result of evaluating one already-parsed JSON payload.
pub(super) struct PolicyPayloadOutcome {
    pub(super) encoded: String,
    pub(super) rule_hits_total: u32,
    pub(super) final_action: PolicyAction,
}

/// Evaluates one JSON `data:` payload and re-encodes the transformed value.
/// Shared by the default and Chat buffered sanitizers so their evaluation
/// request, error mapping, and result aggregation cannot drift apart.
pub(super) fn evaluate_policy_payload(
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
    let mut first_source_barrier = None;
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
        if first_source_barrier.is_none()
            && (outcome.source_ambiguous || frame_exceeds_event_budget(frame))
        {
            first_source_barrier = Some(frame_start..body.len());
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

    Ok(ChatPolicyBody { body, first_source_barrier, rule_hits_total, final_action })
}

/// Restores Chat tool arguments after policy evaluation. Each complete
/// post-policy frame is handed to the driver as a complete event, so policy
/// normalization growth cannot push it over the size-based `feed` budget.
/// A recorded source barrier is still bypassed raw. The driver owns
/// absolute-stream-start BOM handling; this function passes the body through
/// unchanged so a second BOM stays significant. Complete frames and their
/// synthetic carries are emitted first; an unterminated raw tail is appended
/// verbatim afterwards so a held text carry can never place a blank-line
/// terminator after it and turn it into a dispatchable event.
pub(crate) fn rehydrate_chat_sse_stream(
    body: ChatPolicyBody,
    map: RehydrationMap,
) -> (String, RehydrateReport) {
    if map.is_empty() {
        return (body.body, RehydrateReport::default());
    }

    let bytes = body.body.as_bytes();
    let barrier = body.first_source_barrier;

    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map);
    let mut output = Vec::with_capacity(body.body.len());
    let mut barrier_bypassed = barrier.is_none();
    let mut cursor = 0usize;
    while let Some(offset) = event_boundary_end(&bytes[cursor..]) {
        let end = cursor + offset;
        if let Some(source_barrier) = barrier.as_ref() {
            if cursor == source_barrier.start {
                assert_eq!(end, source_barrier.end, "source barrier must align with one frame");
                output.extend(rehydrator.bypass_complete_event(&bytes[cursor..end]));
                barrier_bypassed = true;
                cursor = end;
                continue;
            }
        }
        output.extend(rehydrator.feed_complete_event(&bytes[cursor..end]));
        cursor = end;
    }
    assert!(barrier_bypassed, "source barrier frame must be visited");

    output.extend(rehydrator.finish());
    let tail = &bytes[cursor..];
    if !tail.is_empty() {
        // Feed then drain so the driver observes every raw-tail byte and
        // counts literal markers. The tail has no complete boundary, and
        // finish() already flushed pending carries, so drain() returns only
        // those bytes with no synthetic separators.
        output.extend(rehydrator.feed(tail));
        output.extend(rehydrator.drain());
    }

    let report = rehydrator.report();
    let restored = String::from_utf8(output)
        .expect("buffered Chat restoration only rewrites valid UTF-8 string values");
    (restored, report)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pokrov_core::{
        rehydrate::{RehydrationMap, TokenDeriver, MAX_EVENT_BUFFER_BYTES},
        types::{CategoryActions, EvaluationMode, EvaluatorConfig, PolicyAction, PolicyProfile},
        SanitizationEngine,
    };

    use super::{rehydrate_chat_sse_stream, sanitize_chat_sse_stream};

    fn engine() -> SanitizationEngine {
        let profile = PolicyProfile {
            profile_id: "strict".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Allow,
                pii: PolicyAction::Allow,
                corporate_markers: PolicyAction::Allow,
                custom: PolicyAction::Allow,
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
            profiles: BTreeMap::from([("strict".to_string(), profile)]),
        })
        .expect("engine should build")
    }

    fn token_map(fragment: &str) -> (RehydrationMap, String) {
        let deriver = TokenDeriver::new(b"chat-policy-test-key");
        let mut map = RehydrationMap::new();
        let token = map.token_for(&deriver, fragment);
        (map, token)
    }

    fn event(arguments: &str, ending: &str) -> String {
        let payload = serde_json::json!({
            "id": "c1",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "m",
            "choices": [{
                "index": 0,
                "delta": {"tool_calls": [{
                    "index": 0,
                    "function": {"arguments": arguments}
                }]},
                "finish_reason": null
            }]
        });
        format!("data: {payload}{ending}{ending}")
    }

    fn content_event(content: &str, ending: &str) -> String {
        let payload = serde_json::json!({
            "id": "c1",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "m",
            "choices": [{
                "index": 0,
                "delta": {"content": content},
                "finish_reason": null
            }]
        });
        format!("data: {payload}{ending}{ending}")
    }

    fn assembled_arguments(body: &str) -> String {
        let mut assembled = String::new();
        for line in body.lines() {
            let Some(data) = line.strip_prefix("data:") else { continue };
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(data.trim()) else {
                continue;
            };
            let Some(choices) = payload.get("choices").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for choice in choices {
                let Some(calls) = choice
                    .get("delta")
                    .and_then(|delta| delta.get("tool_calls"))
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for call in calls {
                    if let Some(arguments) = call
                        .get("function")
                        .and_then(|function| function.get("arguments"))
                        .and_then(serde_json::Value::as_str)
                    {
                        assembled.push_str(arguments);
                    }
                }
            }
        }
        assembled
    }

    #[test]
    fn preserves_crlf_frames_and_restores_split_argument_tokens() {
        let (map, token) = token_map("acme\"corp");
        let split = 30;
        let body = format!(
            "{}{}\r\n\r\ndata: [DONE]\r\n\r\n",
            event(&format!("{{\"q\":\"{}", &token[..split]), "\r\n"),
            event(&format!("{}\"}}", &token[split..]), "\r\n"),
        );

        let sanitized = sanitize_chat_sse_stream("req", "strict", &body, &engine())
            .expect("chat policy should succeed");
        assert!(sanitized.first_source_barrier.is_none());
        assert!(sanitized.body.contains("\r\n\r\n"));

        let (restored, report) = rehydrate_chat_sse_stream(sanitized, map);
        assert_eq!(report.restored, 1);
        assert!(!restored.contains("__PKV_"));
        let arguments: serde_json::Value =
            serde_json::from_str(&assembled_arguments(&restored)).expect("inner JSON stays valid");
        assert_eq!(arguments["q"], "acme\"corp");
    }

    #[test]
    fn duplicate_member_frame_records_post_policy_barrier() {
        let body = concat!(
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,",
            "\"delta\":{\"tool_calls\":[{\"index\":0,\"index\":0,",
            "\"function\":{\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\r\n\r\n",
            "data: {\"id\":\"tail\"}"
        );
        let sanitized = sanitize_chat_sse_stream("req", "strict", body, &engine())
            .expect("chat policy should succeed");
        let barrier = sanitized.first_source_barrier.expect("duplicate frame must be recorded");
        let frame = &sanitized.body[barrier];
        assert!(frame.contains("\"index\":0"));
        assert!(!frame.contains("\"index\":0,\"index\":0"));
        assert!(frame.ends_with("\r\n\r\n"));
        // The unterminated tail stays unterminated after policy re-encoding.
        assert!(!sanitized.body.ends_with("\r\n\r\n"));
    }

    #[test]
    fn raw_tail_does_not_restore_arguments() {
        let (map, token) = token_map("acme\"corp");
        let tail = event(&format!("{{\"q\":\"{token}\"}}"), "");
        let sanitized = sanitize_chat_sse_stream("req", "strict", &tail, &engine())
            .expect("chat policy should succeed");
        assert!(sanitized.first_source_barrier.is_none());
        let (restored, report) = rehydrate_chat_sse_stream(sanitized, map);
        assert_eq!(report.restored, 0);
        assert_eq!(report.unrestored, 1, "the literal raw-tail marker must be counted");
        assert!(restored.contains(&token));
        assert!(!restored.ends_with("\n\n"));
    }

    #[test]
    fn policy_growth_at_event_budget_keeps_restoration() {
        let (map, token) = token_map("acme\"corp");
        let split = 30;
        let make_payload = |filler: usize| {
            serde_json::json!({
                "id": "c1",
                "object": "chat.completion.chunk",
                "created": 1,
                "model": "m",
                "choices": [{
                    "index": 0,
                    "delta": {"tool_calls": [{
                        "index": 0,
                        "function": {"arguments": format!("{{\"q\":\"{}", &token[..split])}
                    }]},
                    "finish_reason": null
                }],
                "filler": "x".repeat(filler)
            })
        };
        let base_len = make_payload(0).to_string().len();
        let filler = MAX_EVENT_BUFFER_BYTES - "data:".len() - base_len;
        let payload = make_payload(filler).to_string();
        let first = format!("data:{payload}\n\n");
        assert_eq!(first.trim_end_matches('\n').len(), MAX_EVENT_BUFFER_BYTES);
        let body = format!("{first}{}", event(&format!("{}\"}}", &token[split..]), "\n"));

        let sanitized = sanitize_chat_sse_stream("req", "strict", &body, &engine())
            .expect("chat policy should succeed");
        assert!(sanitized.first_source_barrier.is_none());
        let first_end = sanitized.body.find("\n\n").expect("first frame delimiter") + 2;
        assert_eq!(
            sanitized.body[..first_end].trim_end_matches('\n').len(),
            MAX_EVENT_BUFFER_BYTES + 1,
            "policy normalization must grow the pre-restore frame past the source cap"
        );

        let (restored, report) = rehydrate_chat_sse_stream(sanitized, map);
        assert_eq!(report.restored, 1);
        let arguments: serde_json::Value =
            serde_json::from_str(&assembled_arguments(&restored)).expect("inner JSON stays valid");
        assert_eq!(arguments["q"], "acme\"corp");
    }

    #[test]
    fn raw_tail_after_text_carry_stays_unterminated() {
        let (map, token) = token_map("acme\"corp");
        let split = 30;
        let first = content_event(&format!("use {}", &token[..split]), "\n");
        let tail = event("{\"tail\":\"plain\"}", "");
        let body = format!("{first}{tail}");

        let sanitized = sanitize_chat_sse_stream("req", "strict", &body, &engine())
            .expect("chat policy should succeed");
        assert!(sanitized.first_source_barrier.is_none());
        let (restored, _) = rehydrate_chat_sse_stream(sanitized, map);

        // The raw tail is appended after the finish() carries and keeps its
        // exact bytes: no synthetic terminator may turn it into a new event.
        assert!(restored.ends_with(&tail));
        assert!(!restored.ends_with("\n\n"));
        let carry_and_frames = &restored[..restored.len() - tail.len()];
        assert!(carry_and_frames.ends_with("\n\n"));
        assert_eq!(carry_and_frames.matches("__PKV_").count(), 1);

        let carry_frame = carry_and_frames
            .trim_end_matches('\n')
            .rsplit("\n\n")
            .next()
            .expect("synthetic carry frame");
        let payload: serde_json::Value =
            serde_json::from_str(carry_frame.strip_prefix("data: ").expect("data frame"))
                .expect("synthetic carry JSON");
        assert_eq!(payload["choices"][0]["delta"]["content"], &token[..split]);
    }
}
