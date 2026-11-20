//! Chat Completions argument restoration tests, split by concern: token
//! restoration, escaping and marker counting, association, bounded state,
//! and opaque fallbacks.

use super::*;
use crate::rehydrate::{
    hex_encode, EventRehydrator, RehydrateReport, RehydrationMap, TokenDeriver, TOKEN_PREFIX,
    TOKEN_SUFFIX,
};
use serde_json::{json, Value};

mod association;
mod escaping;
mod fallback;
mod limits;
mod restore;

fn deriver() -> TokenDeriver {
    TokenDeriver::new(b"task-one-tool-arguments")
}
fn map_for(fragments: &[&str]) -> (RehydrationMap, Vec<String>) {
    let (deriver, mut map) = (deriver(), RehydrationMap::new());
    let tokens = fragments.iter().map(|f| map.token_for(&deriver, f)).collect();
    (map, tokens)
}
fn collision_map(fragment: &str) -> (RehydrationMap, String) {
    let (deriver, mut map) = (deriver(), RehydrationMap::new());
    let base = format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&deriver.digest_for(fragment)));
    map.token_to_fragment.insert(base.clone(), "occupant".to_string());
    map.fragment_to_token.insert("occupant".to_string(), base);
    let token = map.token_for(&deriver, fragment);
    assert_eq!(token.len(), MAX_TOKEN_BYTES);
    assert!(token.contains("_c"));
    (map, token)
}
fn event(choice: u64, tools: &[(u64, &str)], finish: bool) -> String {
    let tool_calls: Vec<Value> = tools
        .iter()
        .map(|(index, args)| json!({"index": index, "function": {"arguments": args}}))
        .collect();
    let finish_reason = if finish { Value::String("stop".to_string()) } else { Value::Null };
    format!(
        "data: {}",
        json!({
            "object": "chat.completion.chunk", "id": "chatcmpl-test", "model": "test-model", "created": 1,
            "choices": [{"index": choice, "delta": {"tool_calls": tool_calls}, "finish_reason": finish_reason}],
        })
    )
}
fn content_event(choice: u64, content: &str) -> String {
    format!("data: {}", json!({"choices": [{"index": choice, "delta": {"content": content}}]}))
}
fn finish_event(choice: u64) -> String {
    format!(
        "data: {}",
        json!({"choices": [{"index": choice, "delta": {}, "finish_reason": "stop"}]})
    )
}
fn long_envelope_event(choice: u64, tools: &[(u64, &str)]) -> String {
    let tool_calls: Vec<Value> = tools
        .iter()
        .map(|(index, args)| json!({"index": index, "function": {"arguments": args}}))
        .collect();
    format!(
        "data: {}",
        json!({"id": "i".repeat(MAX_ENVELOPE_FIELD_BYTES), "model": "m".repeat(MAX_ENVELOPE_FIELD_BYTES), "created": 1,
        "choices": [{"index": choice, "delta": {"tool_calls": tool_calls}, "finish_reason": null}]})
    )
}
fn parse_events(output: &str) -> Vec<Value> {
    output
        .split("\n\n")
        .filter(|block| !block.is_empty())
        .filter_map(|block| {
            let payload = block
                .strip_prefix("data: ")
                .unwrap_or_else(|| panic!("unexpected block: {block:?}"));
            (payload != "[DONE]").then(|| serde_json::from_str(payload).expect("valid SSE event"))
        })
        .collect()
}
fn arg_for(event: &Value, choice: u64, tool: u64) -> Option<&str> {
    for entry in event.get("choices")?.as_array()? {
        if entry.get("index").and_then(Value::as_u64) != Some(choice) {
            continue;
        }
        for call in entry.pointer("/delta/tool_calls")?.as_array()? {
            if call.get("index").and_then(Value::as_u64) == Some(tool) {
                return call.pointer("/function/arguments")?.as_str();
            }
        }
    }
    None
}
fn assembled(events: &[Value], choice: u64, tool: u64) -> String {
    events.iter().filter_map(|event| arg_for(event, choice, tool)).collect()
}
struct Harness {
    rehydrator: EventRehydrator,
}
impl Harness {
    fn new(map: RehydrationMap) -> Self {
        Self { rehydrator: EventRehydrator::new_chat_completions(map) }
    }
    fn feed(&mut self, event: &str) -> Vec<Value> {
        parse_events(&self.rehydrator.rehydrate_event(event))
    }
    fn report(&self) -> RehydrateReport {
        self.rehydrator.report()
    }
}
fn feed_all(
    map: &RehydrationMap,
    events: &[String],
    choice: u64,
    tool: u64,
) -> (String, RehydrateReport) {
    let mut harness = Harness::new(map.clone());
    let mut parsed = Vec::new();
    for event in events {
        parsed.extend(harness.feed(event));
    }
    (assembled(&parsed, choice, tool), harness.report())
}
