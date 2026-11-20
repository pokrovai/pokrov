//! Terminal flush placement: synthetic suffixes before raw tails, `[DONE]`,
//! and overflow framing without a trailing newline.

use super::*;

#[test]
fn chat_terminal_flush_precedes_raw_tail() {
    let (map, token) = map_with("acme-corp");
    let prefix = token[..12].to_string();
    let pending = format!("{{\"x\":\"{prefix}");
    for tail in [None, Some(format!("data: {{\"partial\":\"{token}"))] {
        let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
        rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
        assert!(!rehydrator.is_idle(), "pending argument bytes are work");
        if let Some(tail) = &tail {
            assert!(rehydrator.feed(tail.as_bytes()).is_empty());
        }
        let out = rehydrator.finish();
        if let Some(tail) = &tail {
            assert!(position(&out, prefix.as_bytes()) < position(&out, tail.as_bytes()));
            assert!(out.ends_with(tail.as_bytes()), "raw tail is forwarded verbatim");
        } else {
            assert!(position(&out, prefix.as_bytes()) < out.len());
        }
        let expected_unrestored = if tail.is_some() { 2 } else { 1 };
        assert_eq!(rehydrator.report().unrestored, expected_unrestored);
        assert_eq!(rehydrator.report().restored, 0);
        assert!(rehydrator.finish().is_empty());
        assert_eq!(rehydrator.report().unrestored, expected_unrestored);
    }
}

#[test]
fn chat_terminal_tail_stays_last_after_text_and_arguments() {
    let (mut map, argument_token) = map_with("acme-corp");
    let text_token = map.token_for(&TokenDeriver::new(b"task2-key"), "globex");
    let argument_prefix = argument_token[..12].to_string();
    let text_prefix = text_token[..12].to_string();
    let pending = format!("{{\"x\":\"{argument_prefix}");
    let tails = [
        "data: [DONE]".to_string(),
        format!(
            "data: {}",
            serde_json::json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
        ),
    ];
    for tail in tails {
        for drain in [false, true] {
            let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
            rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
            rehydrator.feed(format!("{}\n\n", text_event(&text_prefix)).as_bytes());
            assert!(rehydrator.feed(tail.as_bytes()).is_empty());
            let terminal = if drain { rehydrator.drain() } else { rehydrator.finish() };
            assert!(terminal.ends_with(tail.as_bytes()));
            assert!(!terminal.ends_with(b"\n"), "raw tail stays unterminated");
            let prefix = &terminal[..terminal.len() - tail.len()];
            let dispatched = String::from_utf8_lossy(prefix)
                .lines()
                .filter(|line| line.starts_with("data: "))
                .count();
            assert_eq!(dispatched, 2, "only the argument and text carries dispatch");
            assert!(
                position(prefix, argument_prefix.as_bytes())
                    < position(prefix, text_prefix.as_bytes())
            );
            assert!(!String::from_utf8_lossy(prefix).contains("[DONE]"));
            let report = rehydrator.report();
            assert!(rehydrator.finish().is_empty());
            assert_eq!(rehydrator.report().unrestored, report.unrestored);
        }
    }
}

#[test]
fn chat_done_flushes_before_marker_and_blocks_later_tools() {
    let (mut map, token) = map_with("acme-corp");
    let text_token = map.token_for(&TokenDeriver::new(b"task2-key"), "globex");
    let prefix = token[..12].to_string();
    let pending = format!("{{\"x\":\"{prefix}");
    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map);
    rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());

    let out = rehydrator.feed(b"data: [DONE]\n\n");
    let suffix_at = position(&out, prefix.as_bytes());
    let done_at = position(&out, b"data: [DONE]");
    assert!(suffix_at < done_at, "suffix before [DONE]");

    let complete = format!("{{\"x\":\"{token}\"}}");
    let out = rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &complete)).as_bytes());
    assert!(!String::from_utf8_lossy(&out).contains("acme-corp"));
    let healthy = format!("{}\n\n", text_event(&text_token));
    let out = rehydrator.feed(healthy.as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("globex"));
    assert_eq!(rehydrator.report().restored, 1);

    let tail = format!("data: {{\"after\":\"{token}");
    rehydrator.feed(tail.as_bytes());
    let out = rehydrator.finish();
    assert_eq!(out, tail.as_bytes(), "raw tail is forwarded once");
    assert!(!String::from_utf8_lossy(&out).contains("acme-corp"));
    let report = rehydrator.report();
    assert!(rehydrator.finish().is_empty());
    assert_eq!(rehydrator.report().unrestored, report.unrestored);
}

#[test]
fn chat_terminal_framing_after_overflow_without_trailing_eol() {
    let (mut map, token) = map_with("acme-corp");
    let text_token = map.token_for(&TokenDeriver::new(b"task2-key"), "text");
    let tool_prefix = &token[..12];
    let text_prefix = &text_token[..12];
    let raw = format!(
        "data: {{\"choices\":[{{\"index\":0,\"finish_reason\":\"tool_calls\"}}],\"pad\":\"{}\"}}",
        "x".repeat(MAX_EVENT_BUFFER_BYTES)
    );
    for drain in [false, true] {
        let mut driver = SseStreamRehydrator::new_chat_completions(map.clone());
        let pending = format!("{{\"x\":\"{tool_prefix}");
        driver.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
        driver.feed(format!("{}\n\n", text_event(text_prefix)).as_bytes());
        let out = driver.feed(raw.as_bytes());
        let carries = out.strip_suffix(raw.as_bytes()).expect("exact raw tail last");
        assert!(carries.ends_with(b"\n\n"));
        assert!(
            position(carries, tool_prefix.as_bytes()) < position(carries, text_prefix.as_bytes())
        );
        assert!(event_boundary_end(&out[carries.len()..]).is_none());
        assert!(if drain { driver.drain() } else { driver.finish() }.is_empty());
        assert!(driver.finish().is_empty());
        assert_eq!(driver.report().unrestored, 2);
    }
}
