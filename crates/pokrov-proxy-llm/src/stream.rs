use pokrov_core::{
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

    Ok(StreamSanitizationResult {
        body,
        rule_hits_total: total_hits,
        final_action,
    })
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

pub fn convert_chat_sse_chunk_to_responses_chunk(
    request_id: &str,
    pending_bytes: &mut Vec<u8>,
    incoming_chunk: &[u8],
) -> Vec<u8> {
    pending_bytes.extend_from_slice(incoming_chunk);
    let mut converted = String::new();

    while let Some(separator_index) = find_double_newline(pending_bytes.as_slice()) {
        let event_bytes: Vec<u8> = pending_bytes.drain(..separator_index).collect();
        pending_bytes.drain(..2);
        let event = String::from_utf8_lossy(event_bytes.as_slice());
        let converted_event = convert_single_chat_sse_event(request_id, &event);
        if converted_event.is_empty() {
            continue;
        }
        converted.push_str(&converted_event);
        converted.push_str("\n\n");
    }

    converted.into_bytes()
}

fn find_double_newline(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|window| window == b"\n\n")
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
        if let Some(text) = choice
            .get("delta")
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)
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

    use super::{
        convert_chat_sse_chunk_to_responses_chunk, convert_chat_sse_to_responses_sse, sanitize_sse_stream,
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
            custom_rules: Vec::new(),
            custom_rules_enabled: false,
        };

        SanitizationEngine::new(EvaluatorConfig {
            default_profile: "strict".to_string(),
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
        let mut pending = Vec::new();
        let first = convert_chat_sse_chunk_to_responses_chunk(
            "req-3",
            &mut pending,
            br#"data: {"choices":[{"delta":{"content":"he"}}]"#,
        );
        assert!(first.is_empty());

        let second = convert_chat_sse_chunk_to_responses_chunk(
            "req-3",
            &mut pending,
            b"}\n\ndata: [DONE]\n\n",
        );

        let converted = String::from_utf8(second).expect("converted chunk should be utf-8");
        assert!(converted.contains("\"type\":\"response.output_text.delta\""));
        assert!(converted.contains("\"delta\":\"he\""));
        assert!(converted.contains("\"request_id\":\"req-3\""));
        assert!(converted.contains("data: [DONE]"));
        assert!(pending.is_empty());
    }
}
