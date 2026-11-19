//! Chat Completions driver tests: transport partitioning, opaque-gap
//! ordering, BOM handling, and terminal flush placement.
//!
//! `transport` covers chunk-split equivalence and budget boundaries,
//! `opaque` covers raw forwarding and bypass/complete-event paths, and
//! `terminal` covers flush ordering before raw tails and `[DONE]`.

use super::*;
use crate::rehydrate::{RehydrationMap, TokenDeriver, MAX_EVENT_BUFFER_BYTES};

mod opaque;
mod terminal;
mod transport;

fn map_with(fragment: &str) -> (RehydrationMap, String) {
    let deriver = TokenDeriver::new(b"task2-key");
    let mut map = RehydrationMap::new();
    let token = map.token_for(&deriver, fragment);
    (map, token)
}

fn tool_event(choice: u64, tool: u64, arguments: &str) -> String {
    let payload = serde_json::json!({
        "id": "chatcmpl-task2",
        "object": "chat.completion.chunk",
        "created": 7,
        "model": "gpt-task2",
        "choices": [{
            "index": choice,
            "delta": {"tool_calls": [{"index": tool, "function": {"arguments": arguments}}]},
            "finish_reason": null
        }]
    });
    format!("data: {payload}")
}

fn text_event(content: &str) -> String {
    format!(
        "data: {}",
        serde_json::json!({"object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":content}}]})
    )
}

fn feed_chunks(rehydrator: &mut SseStreamRehydrator, chunks: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in chunks {
        out.extend(rehydrator.feed(chunk));
    }
    out.extend(rehydrator.finish());
    out
}

fn position(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).position(|window| window == needle).expect("marker present")
}

fn padded_text_event(body_len: usize, content: &str) -> String {
    let build = |pad: usize| {
        serde_json::json!({
            "id": "chatcmpl-budget",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-budget",
            "pad": "x".repeat(pad),
            "choices": [{"index": 0, "delta": {"content": content}}]
        })
        .to_string()
    };
    let base_len = "data: ".len() + build(0).len();
    assert!(base_len <= body_len);
    let line = format!("data: {}", build(body_len - base_len));
    assert_eq!(line.len(), body_len);
    line
}
