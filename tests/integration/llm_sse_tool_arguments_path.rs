use std::time::Duration;

use futures_util::StreamExt;
use reqwest::StatusCode;
use serde_json::Value;

use super::llm_proxy_test_support::MockProviderMode;
use super::sse_tool_argument_test_support::{
    assembled_tool_arguments, counter_value, json_payloads, sse_data, sse_payloads,
    start_live_chat_harness, token_for, tool_argument_chunk, tool_arguments_event,
    tool_finish_event, CREATED_AT, DONE_EVENT, TOOL_EVENT_ID, TOOL_FRAGMENT, TOOL_MODEL,
};

// The upstream request is tokenized before forwarding; the same deterministic
// token is embedded in the mock provider's tool-argument deltas. The restored
// fragment contains a quote, so the round trip also proves inner-JSON escaping.
// The first upstream event already carries a strict token prefix: the frame
// emitted while that prefix is held must contain only the safe JSON prefix.
#[tokio::test]
async fn live_tool_arguments_arrive_before_upstream_completion() {
    let token = token_for(TOOL_FRAGMENT);
    let (head, remaining) = token.split_at(20);
    let (mid, tail) = remaining.split_at(30);

    let safe_prefix = "{\"query\":\"refactor ";
    let first_event = tool_arguments_event(&format!("{safe_prefix}{head}"));
    let second_event = tool_arguments_event(mid);
    let third_event = tool_arguments_event(&format!("{tail} utils\",\"limit\":1}}"));

    // Split the second event in the middle of the remaining token bytes so a
    // token fragment also crosses an HTTP transport chunk boundary.
    let token_offset =
        second_event.find(mid).expect("transport split fixture must contain the token bytes");
    let split_at = token_offset + mid.len() / 2;
    let (second_prefix, second_suffix) = second_event.split_at(split_at);

    let harness = start_live_chat_harness(MockProviderMode::SseChunked {
        status: 200,
        chunks: vec![
            first_event,
            second_prefix.to_string(),
            format!("{second_suffix}{third_event}"),
            format!("{}{DONE_EVENT}", tool_finish_event()),
        ],
        delay_ms: 800,
    })
    .await;

    // One deadline covers request submission through the first complete SSE
    // frame; buffering before response headers would also exceed it.
    let (mut stream, first_bytes) = tokio::time::timeout(Duration::from_millis(500), async {
        let response = harness.post_streaming_tool_request().await;
        assert_eq!(response.status(), StatusCode::OK);
        let mut stream = response.bytes_stream();
        let mut collected = Vec::new();
        while !collected.windows(2).any(|window| window == b"\n\n") {
            let item = stream
                .next()
                .await
                .expect("stream should yield a frame before the deadline")
                .expect("first stream item should be successful");
            collected.extend_from_slice(&item);
        }
        (stream, collected)
    })
    .await
    .expect("the first complete frame must arrive before the delayed follow-up chunk");

    // The transport may split a frame across items, but no later provider
    // chunk may contribute bytes before this deadline.
    let frame_len = first_bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .expect("the collected bytes must contain a frame delimiter")
        + 2;
    assert_eq!(
        first_bytes.len(),
        frame_len,
        "only the first complete frame may arrive before the delayed chunk"
    );
    let mut body =
        String::from_utf8(first_bytes[..frame_len].to_vec()).expect("SSE output is UTF-8");
    let first_payloads = sse_payloads(&body);
    assert_eq!(first_payloads.len(), 1, "the first feed must emit exactly the first event");
    let first_payload: Value =
        serde_json::from_str(&first_payloads[0]).expect("first payload must stay valid JSON");
    assert_eq!(
        arguments_of(&first_payload),
        safe_prefix,
        "the first frame must contain exactly the safe prefix while the token prefix stays held"
    );
    assert!(!body.contains("__PKV_"), "held token bytes must not be emitted early");

    while let Some(item) = stream.next().await {
        let chunk = item.expect("stream should not fail after the first frame");
        body.push_str(std::str::from_utf8(&chunk).expect("SSE output is UTF-8"));
    }

    assert!(!body.contains("__PKV_"), "no token fragments may remain visible");
    let assembled = assembled_tool_arguments(&body);
    let decoded: Value =
        serde_json::from_str(&assembled).expect("assembled arguments must be valid JSON");
    assert_eq!(decoded["query"], "refactor acme\"corp utils");
    assert_eq!(decoded["limit"], 1);
    assert_no_fabricated_terminals(&body, 1, 1);

    let captured = harness.provider.captured_requests().await;
    assert_eq!(captured.len(), 1);
    let captured_text =
        serde_json::to_string(&captured[0]).expect("captured request should serialize");
    assert!(captured_text.contains(&token), "upstream request must carry the deterministic token");
    assert!(!captured_text.contains("acme"), "upstream request must not carry the original");

    harness.shutdown().await;
}

#[tokio::test]
async fn live_tool_suffix_precedes_done_eof_and_abort() {
    let token = token_for(TOOL_FRAGMENT);
    let (head, remaining) = token.split_at(20);
    let (mid, tail) = remaining.split_at(30);
    let safe_prefix = "{\"query\":\"refactor ";
    let first_event = tool_arguments_event(&format!("{safe_prefix}{head}"));
    let mid_event = tool_arguments_event(mid);
    let pending_suffix = format!("{head}{mid}");
    let raw_tail = "data: {\"id\":\"chatcmpl-live-tool\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"unterminated";

    // `[DONE]` with the token prefix still pending: the suffix is flushed
    // literally before the marker, with no fabricated finish event.
    let ok_body = format!("{first_event}{mid_event}{DONE_EVENT}");
    let harness =
        start_live_chat_harness(MockProviderMode::Sse { status: 200, body: ok_body }).await;
    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body should be readable");

    let payloads = json_payloads(&body);
    assert_eq!(payloads.len(), 3, "two upstream events plus the flushed suffix");
    assert_eq!(arguments_of(&payloads[0]), safe_prefix);
    assert_eq!(arguments_of(&payloads[1]), "");
    assert_synthetic_suffix(&payloads[2], &pending_suffix);
    assert_no_fabricated_terminals(&body, 0, 1);
    assert_eq!(sse_payloads(&body).last().map(String::as_str), Some("[DONE]"));

    harness.shutdown().await;

    // A finishing event that carries the final argument bytes: the pending
    // token is completed in the same event, restored, and the finish reason
    // stays on that event.
    let mut finish_chunk = tool_argument_chunk(&format!("{mid}{tail} utils\",\"limit\":1}}"));
    finish_chunk["choices"][0]["finish_reason"] = Value::String("tool_calls".to_string());
    let finish_event = sse_data(&finish_chunk);
    let finish_body = format!("{first_event}{finish_event}{DONE_EVENT}");
    let harness =
        start_live_chat_harness(MockProviderMode::Sse { status: 200, body: finish_body }).await;
    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body should be readable");

    let payloads = json_payloads(&body);
    assert_eq!(payloads.len(), 2, "the final delta must not need a synthetic suffix");
    assert_eq!(arguments_of(&payloads[0]), safe_prefix);
    let restored_tail = r#"acme\"corp utils","limit":1}"#;
    assert_eq!(arguments_of(&payloads[1]), restored_tail);
    assert_eq!(payloads[1]["choices"][0]["finish_reason"], "tool_calls");
    let assembled = assembled_tool_arguments(&body);
    assert_eq!(assembled, format!("{safe_prefix}{restored_tail}"));
    let decoded: Value =
        serde_json::from_str(&assembled).expect("assembled arguments must be valid JSON");
    assert_eq!(decoded["query"], "refactor acme\"corp utils");
    assert_no_fabricated_terminals(&body, 1, 1);
    assert_eq!(
        sse_payloads(&body).last().map(String::as_str),
        Some("[DONE]"),
        "the finishing frame must precede [DONE]"
    );

    harness.shutdown().await;

    // EOF with an unterminated raw event: complete frames, then the synthetic
    // suffix frame, then the raw tail verbatim.
    let eof_body = format!("{first_event}{mid_event}{raw_tail}");
    let harness =
        start_live_chat_harness(MockProviderMode::Sse { status: 200, body: eof_body }).await;
    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body should be readable");
    assert_pending_frames_before_raw_tail(&body, raw_tail, safe_prefix, &pending_suffix);
    assert_no_fabricated_terminals(&body, 0, 0);

    harness.shutdown().await;

    // An upstream body error must relay the queued suffix and raw bytes before
    // the original error surfaces; a client timeout is not acceptable proof.
    let abort_body = format!("{first_event}{mid_event}{raw_tail}");
    let harness = start_live_chat_harness(MockProviderMode::SseAbort {
        status: 200,
        chunks: vec![abort_body],
        delay_ms: 50,
    })
    .await;
    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut collected = Vec::new();
    let mut transfer_error = None;
    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        match item {
            Ok(chunk) => collected.extend_from_slice(&chunk),
            Err(error) => {
                transfer_error = Some(error);
                break;
            }
        }
    }
    let transfer_error = transfer_error.expect("the client must observe the upstream abort");
    assert!(
        !transfer_error.is_timeout(),
        "abort must surface as a body-transfer failure, not a client timeout: {transfer_error}"
    );
    assert!(
        transfer_error.is_body() || transfer_error.is_decode(),
        "abort must surface as a body-transfer failure: {transfer_error}"
    );
    let body = String::from_utf8(collected).expect("flushed bytes are UTF-8");
    assert_pending_frames_before_raw_tail(&body, raw_tail, safe_prefix, &pending_suffix);
    assert_no_fabricated_terminals(&body, 0, 0);

    harness.shutdown().await;
}

// Wire fixture deliberately uses noncanonical JSON whitespace, SSE control
// lines, and CRLF framing so a parse-reserialize implementation cannot pass.
// The token appears in an inner JSON key and must stay literal.
#[tokio::test]
async fn live_tool_argument_no_eligible_token_preserves_frames() {
    let token = token_for(TOOL_FRAGMENT);
    let inner = format!("{{\"{token}\":\"plain value\",\"limit\":2}}");
    let arguments = serde_json::to_string(&inner).expect("inner arguments should encode");
    let payload = r#"{ "id" : "chatcmpl-live-tool" , "object" : "chat.completion.chunk" , "created" : 1700000000 , "model" : "gpt-4o-mini" , "choices" : [ { "index" : 0 , "delta" : { "tool_calls" : [ { "index" : 0 , "function" : { "arguments" : __ARGUMENTS__ } } ] } , "finish_reason" : null } ] }"#
        .replace("__ARGUMENTS__", &arguments);
    let finish_payload = r#"{ "id" : "chatcmpl-live-tool" , "choices" : [ { "index" : 0 , "delta" : {} , "finish_reason" : "tool_calls" } ] }"#;
    let upstream_body = format!(
        ": keepalive\r\nevent: message\r\ndata: {payload}\r\n\r\nevent: message\r\ndata: {finish_payload}\r\n\r\ndata: [DONE]\r\n\r\n"
    );

    let harness =
        start_live_chat_harness(MockProviderMode::Sse { status: 200, body: upstream_body.clone() })
            .await;

    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body should be readable");
    assert_eq!(body, upstream_body, "unchanged frames must be forwarded byte-for-byte");

    let first_payload = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .next()
        .expect("the first wire payload must parse");
    assert_eq!(first_payload["id"], TOOL_EVENT_ID);
    assert_eq!(first_payload["object"], "chat.completion.chunk");
    assert_eq!(first_payload["created"], CREATED_AT);
    assert_eq!(first_payload["model"], TOOL_MODEL);
    let emitted_arguments = first_payload["choices"][0]["delta"]["tool_calls"][0]["function"]
        ["arguments"]
        .as_str()
        .expect("tool arguments must be a string");
    assert_eq!(emitted_arguments, inner);
    assert!(
        emitted_arguments.contains(&token),
        "a request-known token in an inner key must stay literal"
    );

    let captured = harness.provider.captured_requests().await;
    assert_eq!(captured.len(), 1);
    let captured_text =
        serde_json::to_string(&captured[0]).expect("captured request should serialize");
    assert!(captured_text.contains(&token), "the request map must be nonempty");
    assert!(!captured_text.contains("acme"), "upstream request must not carry the original");

    harness.shutdown().await;
}

#[tokio::test]
async fn live_tool_argument_metrics_count_once_without_argument_data() {
    let token = token_for(TOOL_FRAGMENT);
    let (head, tail) = token.split_at(24);
    let restored_body = format!(
        "{}{}{}{}{DONE_EVENT}",
        tool_arguments_event("{\"query\":\"refactor "),
        tool_arguments_event(head),
        tool_arguments_event(&format!("{tail} utils\",\"limit\":1}}")),
        tool_finish_event(),
    );
    let harness =
        start_live_chat_harness(MockProviderMode::Sse { status: 200, body: restored_body }).await;

    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body should be readable");
    assert!(!body.contains("__PKV_"), "the split token must be restored exactly once");

    let metrics = harness.metrics_body().await;
    assert_eq!(
        counter_value(&metrics, "pokrov_rehydrated_tokens_total"),
        Some(1),
        "one split token must increment the restore counter once"
    );
    assert!(!metrics.contains(&token), "metrics must not expose token data");
    assert!(!metrics.contains("acme"), "metrics must not expose restored values");
    assert!(!metrics.contains("refactor"), "metrics must not expose argument data");
    harness.shutdown().await;

    // A strict prefix of the known token containing the full `__PKV_` marker
    // stays pending through `[DONE]` and is flushed literally exactly once.
    let pending = &token[..40];
    let unrestored_body = format!(
        "{}{DONE_EVENT}",
        tool_arguments_event(&format!("{{\"query\":\"refactor {pending}")),
    );
    let harness =
        start_live_chat_harness(MockProviderMode::Sse { status: 200, body: unrestored_body }).await;

    let response = harness.post_streaming_tool_request().await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("stream body should be readable");
    assert!(body.contains(pending), "the held marker prefix must be flushed literally");
    assert!(!body.contains(&token), "an incomplete token must never be restored");

    let metrics = harness.metrics_body().await;
    assert_eq!(
        counter_value(&metrics, "pokrov_unrestored_tokens_total"),
        Some(1),
        "one flushed complete marker must increment the unrestored counter once"
    );
    assert!(!metrics.contains(pending), "metrics must not expose marker data");
    assert!(!metrics.contains("query"), "metrics must not expose argument data");

    harness.shutdown().await;
}

fn arguments_of(payload: &Value) -> &str {
    payload["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .expect("tool arguments must be present as a string")
}

fn assert_synthetic_suffix(payload: &Value, expected: &str) {
    assert_eq!(payload["object"], "chat.completion.chunk");
    assert_eq!(payload["id"], TOOL_EVENT_ID);
    assert_eq!(payload["choices"][0]["index"], 0);
    assert_eq!(payload["choices"][0]["finish_reason"], Value::Null);
    assert_eq!(payload["choices"][0]["delta"]["tool_calls"][0]["index"], 0);
    assert_eq!(arguments_of(payload), expected);
}

fn assert_pending_frames_before_raw_tail(
    body: &str,
    raw_tail: &str,
    safe_prefix: &str,
    pending_suffix: &str,
) {
    assert!(
        body.ends_with(raw_tail),
        "the raw partial event must be forwarded verbatim at the end"
    );
    let frames_part = &body[..body.len() - raw_tail.len()];
    assert!(frames_part.ends_with("\n\n"), "complete frames must end before the raw tail");
    let frames = json_payloads(frames_part);
    assert_eq!(frames.len(), 3, "two upstream events plus the flushed suffix");
    assert_eq!(arguments_of(&frames[0]), safe_prefix);
    assert_eq!(arguments_of(&frames[1]), "");
    assert_synthetic_suffix(&frames[2], pending_suffix);
}

// Chat payloads may finish only through the upstream finish reason; synthetic
// suffix chunks carry `finish_reason: null` and never fabricate `[DONE]`.
// Unterminated raw tails are intentionally incomplete frames and are checked
// verbatim by the caller, so only parseable payloads are inspected here.
fn assert_no_fabricated_terminals(body: &str, expected_finishes: usize, expected_done: usize) {
    let mut finishes = 0;
    for payload in sse_payloads(body) {
        if payload == "[DONE]" {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };
        let Some(choices) = parsed.get("choices").and_then(Value::as_array) else {
            continue;
        };
        for choice in choices {
            let finish = &choice["finish_reason"];
            assert!(
                finish.is_null() || finish.as_str() == Some("tool_calls"),
                "no fabricated terminal chunk may be emitted: {finish}"
            );
            if finish.as_str() == Some("tool_calls") {
                finishes += 1;
            }
        }
    }
    assert_eq!(finishes, expected_finishes);
    let done_count = sse_payloads(body).iter().filter(|payload| *payload == "[DONE]").count();
    assert_eq!(done_count, expected_done, "only upstream terminals may be forwarded");
}
