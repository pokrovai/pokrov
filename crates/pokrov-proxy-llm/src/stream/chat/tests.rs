use std::collections::BTreeMap;

use pokrov_core::{
    rehydrate::{RehydrationMap, TokenDeriver, MAX_EVENT_BUFFER_BYTES},
    types::{CategoryActions, EvaluationMode, EvaluatorConfig, PolicyAction, PolicyProfile},
    SanitizationEngine,
};

use super::{rehydrate_chat_sse_stream, sanitize_chat_sse_stream, BarrierKind};

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
    assert!(sanitized.source_barriers.is_empty());
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
    assert_eq!(sanitized.source_barriers.len(), 1);
    let (barrier, kind) = &sanitized.source_barriers[0];
    assert_eq!(*kind, BarrierKind::Ambiguous);
    let frame = &sanitized.body[barrier.clone()];
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
    assert!(sanitized.source_barriers.is_empty());
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
    assert!(sanitized.source_barriers.is_empty());
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
    assert!(sanitized.source_barriers.is_empty());
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
