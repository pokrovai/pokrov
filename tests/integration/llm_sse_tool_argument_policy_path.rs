//! Buffered Chat Completions tool-argument restoration after output policy.
//!
//! These tests exercise the buffered `/v1/chat/completions` route with and
//! without output sanitization: source-level duplicate members must not
//! become trusted association, both policy modes must reconstruct the same
//! arguments when policy leaves fragments intact, and a policy transform or
//! block must win over restoration.

use pokrov_core::rehydrate::MAX_EVENT_BUFFER_BYTES;
use reqwest::{header::CONTENT_TYPE, StatusCode};
use serde_json::{json, Value};

use super::{
    llm_proxy_test_support::MockProviderMode,
    sse_tool_argument_test_support::{
        counter_value, token_for, TOOL_CALL_ID, TOOL_EVENT_ID, TOOL_FRAGMENT, TOOL_MODEL, TOOL_NAME,
    },
};

#[path = "llm_sse_tool_argument_policy_support.rs"]
mod support;

use support::{
    argument_event, assembled_arguments, assembled_arguments_for, assert_event_budget_parity,
    assert_policy_parity, done_event, first_frame_at_event_budget, json_payloads, plain_stream,
    split_complete_frames, sse_event, token_stream, BufferedHarness, PLAIN_ROUTE, POLICY_ROUTE,
};

#[tokio::test]
async fn buffered_duplicate_member_frame_cannot_complete_pending_argument() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let rest = format!("{}\"}}", &token[split..]);
    let rest_json = serde_json::to_string(&rest).expect("rest should serialize");
    let duplicate = format!(
        concat!(
            "{{\"id\":\"chatcmpl-dup\",\"object\":\"chat.completion.chunk\",\"created\":1,",
            "\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[",
            "{{\"index\":0,\"index\":0,\"id\":\"call_dup\",\"type\":\"function\",",
            "\"function\":{{\"name\":\"lookup\",\"arguments\":{rest}}}}}",
            "]}},\"finish_reason\":null}}],\"policy_marker\":\"rewrite-me\"}}"
        ),
        model = TOOL_MODEL,
        rest = rest_json,
    );
    let tail = json!({
        "id": "chatcmpl-tail",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": TOOL_MODEL,
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": 0,
                "function": {"name": "lookup", "arguments": format!("{{\"later\":\"{token}\"}}")}
            }]},
            "finish_reason": null
        }]
    })
    .to_string();

    let mut body = String::new();
    body.push_str(&argument_event(&format!("{{\"q\":\"{}", &token[..split]), 0, 0, "\r\n"));
    body.push_str(&sse_event(&duplicate, "\r\n"));
    body.push_str(&format!("data: {tail}"));

    let harness = BufferedHarness::start(body).await;
    let response = harness.post(POLICY_ROUTE, "strict").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");

    let (frames, raw_tail) = split_complete_frames(&body);
    assert_eq!(frames.len(), 3, "first event, flushed suffix, duplicate frame: {frames:?}");
    assert_eq!(raw_tail, format!("data: {tail}"), "tail bytes must stay verbatim last");
    assert!(frames[0].contains(TOOL_EVENT_ID));
    assert!(frames[2].contains("POLICY-RAN"), "policy must evaluate the ambiguous frame");
    assert!(
        !frames[2].contains("\"index\":0,\"index\":0"),
        "policy must normalize the duplicate member"
    );

    let complete = frames.concat();
    assert_eq!(
        assembled_arguments_for(frames[0], 0, 0),
        "{\"q\":\"",
        "the safe prefix precedes the held token prefix"
    );
    assert_eq!(
        assembled_arguments_for(frames[1], 0, 0),
        &token[..split],
        "the synthetic suffix must carry the held prefix"
    );
    assert_eq!(
        assembled_arguments_for(frames[2], 0, 0),
        rest,
        "the bypassed frame keeps its normalized arguments"
    );
    assert_eq!(assembled_arguments_for(&complete, 0, 0), format!("{{\"q\":\"{token}\"}}"));
    assert!(!complete.contains("acme"), "no frame may complete the pending token");

    let metrics = harness.metrics().await;
    assert_eq!(
        counter_value(&metrics, "pokrov_rehydrated_tokens_total"),
        Some(0),
        "the barrier must not report a restoration"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn oversized_source_frame_closes_argument_admissions() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let rest = format!("{}\"}}", &token[split..]);
    let rest_json = serde_json::to_string(&rest).expect("rest should serialize");
    let compact = format!(
        concat!(
            "{{\"id\":\"chatcmpl-oversize\",\"object\":\"chat.completion.chunk\",\"created\":1,",
            "\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[",
            "{{\"index\":0,\"function\":{{\"arguments\":{rest}}}}}]}},",
            "\"finish_reason\":null}}]}}"
        ),
        model = TOOL_MODEL,
        rest = rest_json,
    );
    let oversized = format!(
        "data: {padding}{compact}\r\n\r\n",
        padding = " ".repeat(MAX_EVENT_BUFFER_BYTES + 1024),
    );

    let mut body = String::new();
    body.push_str(&argument_event(&format!("{{\"q\":\"{}", &token[..split]), 0, 0, "\r\n"));
    body.push_str(&oversized);
    body.push_str(&argument_event(&format!("{{\"later\":\"{token}\"}}"), 1, 0, "\r\n"));

    let harness = BufferedHarness::start(body).await;
    let response = harness.post(POLICY_ROUTE, "strict").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");

    let (frames, tail) = split_complete_frames(&body);
    assert!(tail.is_empty(), "all fixture frames are terminated");
    let complete = frames.concat();
    let compact_frame = frames
        .iter()
        .find(|frame| frame.contains("chatcmpl-oversize"))
        .expect("oversized source frame must be forwarded");
    assert!(
        compact_frame.len() < MAX_EVENT_BUFFER_BYTES,
        "policy must shrink the frame below the event budget"
    );
    assert_eq!(
        assembled_arguments_for(&complete, 0, 0),
        format!("{{\"q\":\"{token}\"}}"),
        "the held prefix and oversized-frame arguments stay literal"
    );
    assert_eq!(
        assembled_arguments_for(&complete, 1, 0),
        format!("{{\"later\":\"{token}\"}}"),
        "global bypass keeps later whole tokens literal"
    );
    assert!(!complete.contains("acme"), "no restored original may appear");

    let metrics = harness.metrics().await;
    assert_eq!(
        counter_value(&metrics, "pokrov_rehydrated_tokens_total"),
        Some(0),
        "the source barrier must not report a restoration"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn text_carry_across_second_oversized_frame_stays_literal() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let text_event = |content: &str| {
        sse_event(
            &json!({
                "id": "chatcmpl-text-overflow",
                "object": "chat.completion.chunk",
                "created": 1,
                "model": TOOL_MODEL,
                "choices": [{
                    "index": 0,
                    "delta": {"content": content},
                    "finish_reason": null
                }]
            })
            .to_string(),
            "\n",
        )
    };
    let keepalive = format!(": keepalive {}\n\n", "x".repeat(MAX_EVENT_BUFFER_BYTES));
    let oversized_restore = sse_event(
        &json!({
            "id": "chatcmpl-oversized-text",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": TOOL_MODEL,
            "choices": [{
                "index": 0,
                "delta": {"content": token},
                "finish_reason": null
            }],
            "filler": "x".repeat(MAX_EVENT_BUFFER_BYTES)
        })
        .to_string(),
        "\n",
    );

    let mut body = String::new();
    body.push_str(&text_event(&format!("start {}", &token[..split])));
    body.push_str(&keepalive);
    body.push_str(&text_event("middle"));
    body.push_str(&oversized_restore);
    body.push_str(&text_event("end"));
    body.push_str(&done_event("\n"));

    let harness = BufferedHarness::start(body).await;
    let policy = harness.post(POLICY_ROUTE, "strict").await;
    let plain = harness.post(PLAIN_ROUTE, "strict").await;
    assert_eq!(policy.status(), StatusCode::OK, "policy route must succeed");
    assert_eq!(plain.status(), StatusCode::OK, "plain route must succeed");
    let policy_body = policy.text().await.expect("policy body expected");
    let plain_body = plain.text().await.expect("plain body expected");

    for body in [&policy_body, &plain_body] {
        assert!(!body.contains("acme"), "no text or raw oversize frame may restore");
        assert!(body.contains(&token), "the later oversized frame stays literal");
        assert_eq!(
            body.matches("__PKV_").count(),
            2,
            "one flushed split prefix plus one literal token in the raw frame"
        );
    }
    assert_eq!(json_payloads(&policy_body), json_payloads(&plain_body));

    harness.shutdown().await;
}

#[tokio::test]
async fn terminal_errors_prevent_restoration() {
    let token = token_for(TOOL_FRAGMENT);
    let restorable = argument_event(&format!("{{\"q\":\"{token}\"}}"), 0, 0, "\n");

    let mut block_body = restorable.clone();
    block_body.push_str(&argument_event("{\"text\":\"block-me\"}", 0, 1, "\n"));
    let block_harness = BufferedHarness::start(block_body).await;
    let blocked = block_harness.post(POLICY_ROUTE, "minimal").await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        blocked.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok()),
        Some("application/json"),
        "a blocked buffered stream must not expose an SSE body"
    );
    let error: Value = blocked.json().await.expect("policy error body expected");
    assert_eq!(error["error"]["code"], "policy_blocked");
    let metrics = block_harness.metrics().await;
    assert_eq!(
        counter_value(&metrics, "pokrov_rehydrated_tokens_total"),
        Some(0),
        "blocked output must not report a restoration"
    );
    block_harness.shutdown().await;

    let abort_harness = BufferedHarness::start_with_mode(MockProviderMode::SseAbort {
        status: 200,
        chunks: vec![restorable],
        delay_ms: 10,
    })
    .await;
    let aborted = abort_harness.post(POLICY_ROUTE, "strict").await;
    assert_eq!(aborted.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        aborted.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok()),
        Some("application/json"),
        "an aborted buffered stream must not expose an SSE body"
    );
    let error: Value = aborted.json().await.expect("upstream error body expected");
    assert_eq!(error["error"]["code"], "upstream_error");
    let metrics = abort_harness.metrics().await;
    assert_eq!(
        counter_value(&metrics, "pokrov_rehydrated_tokens_total"),
        Some(0),
        "an aborted body must not report a restoration"
    );
    abort_harness.shutdown().await;
}

#[tokio::test]
async fn buffered_chat_policy_preserves_control_lines_and_separators() {
    let spaced = format!(
        concat!(
            "{{ \"id\" : \"chatcmpl-framing\", \"object\" : \"chat.completion.chunk\",",
            " \"created\" : 1, \"model\" : \"{model}\", \"choices\" : [ {{ \"index\" : 0,",
            " \"delta\" : {{ \"content\" : \"hello\" }}, \"finish_reason\" : null }} ] }}"
        ),
        model = TOOL_MODEL,
    );
    let compact: Value = serde_json::from_str(&spaced).expect("fixture payload is JSON");
    let compact = compact.to_string();

    for ending in ["\n", "\r\n", "\r"] {
        let body = format!(
            "event: message{ending}id: 7{ending}data: {spaced}{ending}: keepalive{ending}{ending}\
             data: [DONE]{ending}{ending}"
        );
        let expected = format!(
            "event: message{ending}id: 7{ending}data: {compact}{ending}: keepalive{ending}{ending}\
             data: [DONE]{ending}{ending}"
        );
        let harness = BufferedHarness::start(body).await;
        let response = harness.post(POLICY_ROUTE, "strict").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.expect("stream body expected");
        assert_eq!(body, expected, "every control line and separator must survive");
        harness.shutdown().await;
    }
}

#[tokio::test]
async fn buffered_chat_policy_handles_leading_bom() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let first_payload = json!({
        "id": TOOL_EVENT_ID,
        "object": "chat.completion.chunk",
        "created": 1_700_000_000,
        "model": TOOL_MODEL,
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": 0,
                "id": TOOL_CALL_ID,
                "type": "function",
                "function": {"name": TOOL_NAME, "arguments": format!("{{\"q\":\"{}", &token[..split])}
            }]},
            "finish_reason": null
        }],
        "policy_marker": "rewrite-me"
    });
    let body = format!(
        "\u{feff}{}{}",
        sse_event(&first_payload.to_string(), "\n"),
        argument_event(&format!("{}\"}}", &token[split..]), 0, 0, "\n"),
    );

    let harness = BufferedHarness::start(body).await;
    let response = harness.post(POLICY_ROUTE, "strict").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");

    assert!(body.starts_with('\u{feff}'), "the BOM must be forwarded first");
    assert_eq!(body.matches('\u{feff}').count(), 1, "exactly one BOM is forwarded");
    assert!(body.contains("POLICY-RAN"), "policy must evaluate the BOM-prefixed first event");
    assert!(!body.contains("__PKV_"), "the first event must restore through the BOM");
    let parse_body = body.strip_prefix('\u{feff}').expect("forwarded BOM");
    let inner: Value = serde_json::from_str(&assembled_arguments_for(parse_body, 0, 0))
        .expect("restored inner arguments must stay valid JSON");
    assert_eq!(inner["q"], TOOL_FRAGMENT);

    harness.shutdown().await;
}

#[tokio::test]
async fn policy_modes_agree_at_event_budget_boundary() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let body = format!(
        "{}{}",
        first_frame_at_event_budget(&format!("{{\"q\":\"{}", &token[..split])),
        argument_event(&format!("{}\"}}", &token[split..]), 0, 0, "\n"),
    );
    assert_event_budget_parity(body, false).await;
}

#[tokio::test]
async fn policy_modes_agree_at_bom_event_budget_boundary() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let body = format!(
        "\u{feff}{}{}",
        first_frame_at_event_budget(&format!("{{\"q\":\"{}", &token[..split])),
        argument_event(&format!("{}\"}}", &token[split..]), 0, 0, "\n"),
    );
    assert_event_budget_parity(body, true).await;
}

#[tokio::test]
async fn double_bom_before_done_keeps_later_token_restorable() {
    let token = token_for(TOOL_FRAGMENT);
    let split = 30;
    let body = format!(
        "\u{feff}\u{feff}{}{}{}",
        done_event("\n"),
        argument_event(&format!("{{\"q\":\"{}", &token[..split]), 0, 0, "\n"),
        argument_event(&format!("{}\"}}", &token[split..]), 0, 0, "\n"),
    );

    let harness = BufferedHarness::start(body).await;
    let policy = harness.post(POLICY_ROUTE, "strict").await;
    let plain = harness.post(PLAIN_ROUTE, "strict").await;
    assert_eq!(policy.status(), StatusCode::OK, "policy route must succeed");
    assert_eq!(plain.status(), StatusCode::OK, "plain route must succeed");
    let policy_body = policy.text().await.expect("policy body expected");
    let plain_body = plain.text().await.expect("plain body expected");

    for body in [&policy_body, &plain_body] {
        assert!(body.starts_with("\u{feff}\u{feff}"), "both BOMs must be forwarded");
        assert_eq!(body.matches('\u{feff}').count(), 2);
        assert!(body.contains("acme"), "the second BOM must not make [DONE] terminal");
        assert!(!body.contains("__PKV_"));
        let view = body.strip_prefix('\u{feff}').expect("first BOM");
        let inner: Value = serde_json::from_str(&assembled_arguments_for(view, 0, 0))
            .expect("held token must restore after the nonterminal marker");
        assert_eq!(inner["q"], TOOL_FRAGMENT);
    }
    let policy_view = policy_body.strip_prefix('\u{feff}').expect("policy first BOM");
    let plain_view = plain_body.strip_prefix('\u{feff}').expect("plain first BOM");
    assert_eq!(json_payloads(policy_view), json_payloads(plain_view));
    assert_eq!(assembled_arguments(policy_view), assembled_arguments(plain_view));

    harness.shutdown().await;
}

#[tokio::test]
async fn buffered_policy_ignores_bom_only_at_stream_start() {
    let first = json!({
        "id": "bom-first",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": TOOL_MODEL,
        "choices": [{"index": 0, "delta": {"content": "first"}, "finish_reason": null}],
        "policy_marker": "rewrite-me"
    });
    let second = json!({
        "id": "bom-second",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": TOOL_MODEL,
        "choices": [{"index": 0, "delta": {"content": "second"}, "finish_reason": null}],
        "policy_marker": "rewrite-me"
    });
    let body = format!(
        "\u{feff}{}\u{feff}{}",
        sse_event(&first.to_string(), "\n"),
        sse_event(&second.to_string(), "\n"),
    );

    let harness = BufferedHarness::start(body).await;
    let response = harness.post(POLICY_ROUTE, "strict").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body expected");

    assert_eq!(body.matches('\u{feff}').count(), 2, "both BOMs stay byte-for-byte");
    assert_eq!(
        body.matches("POLICY-RAN").count(),
        1,
        "only the stream-start frame is policy-evaluated"
    );
    assert_eq!(body.matches("rewrite-me").count(), 1, "the mid-stream BOM hides its field");

    harness.shutdown().await;
}

#[tokio::test]
async fn policy_modes_agree_when_fragments_survive() {
    let token = token_for(TOOL_FRAGMENT);
    for ending in ["\n", "\r\n", "\r"] {
        assert_policy_parity(token_stream(ending, &token), ending, true).await;
        assert_policy_parity(plain_stream(ending), ending, false).await;
    }
}

#[tokio::test]
async fn output_policy_transform_and_block_win() {
    let token = token_for(TOOL_FRAGMENT);

    let transform_harness = BufferedHarness::start(token_stream("\n", &token)).await;
    let transformed = transform_harness.post(POLICY_ROUTE, "custom").await;
    assert_eq!(transformed.status(), StatusCode::OK);
    let transformed = transformed.text().await.expect("transformed body expected");
    assert!(transformed.contains("[POLICY]"), "transform rule must rewrite the token");
    assert!(!transformed.contains("acme"), "a transformed token must not restore");
    let metrics = transform_harness.metrics().await;
    assert_eq!(counter_value(&metrics, "pokrov_rehydrated_tokens_total"), Some(0));
    assert_eq!(counter_value(&metrics, "pokrov_unrestored_tokens_total"), Some(0));
    transform_harness.shutdown().await;

    let block_body = argument_event("{\"text\":\"block-me\"}", 0, 0, "\n");
    let block_harness = BufferedHarness::start(block_body).await;
    let blocked = block_harness.post(POLICY_ROUTE, "minimal").await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    let error: Value = blocked.json().await.expect("policy error body expected");
    assert_eq!(error["error"]["code"], "policy_blocked");
    let metrics = block_harness.metrics().await;
    assert_eq!(counter_value(&metrics, "pokrov_rehydrated_tokens_total"), Some(0));
    block_harness.shutdown().await;

    let ambiguous = sse_event(
        concat!(
            "{\"id\":\"chatcmpl-block\",\"choices\":[{\"index\":0,",
            "\"delta\":{\"tool_calls\":[{\"index\":0,\"index\":0,",
            "\"function\":{\"arguments\":\"block-me\"}}]},",
            "\"finish_reason\":null}]}"
        ),
        "\n",
    );
    let ambiguous_harness = BufferedHarness::start(ambiguous).await;
    let blocked = ambiguous_harness.post(POLICY_ROUTE, "minimal").await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    let error: Value = blocked.json().await.expect("policy error body expected");
    assert_eq!(error["error"]["code"], "policy_blocked");
    let metrics = ambiguous_harness.metrics().await;
    assert_eq!(counter_value(&metrics, "pokrov_rehydrated_tokens_total"), Some(0));
    ambiguous_harness.shutdown().await;
}
