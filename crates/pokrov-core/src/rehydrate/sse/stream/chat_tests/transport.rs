//! Transport partitioning: chunk-split equivalence, delimiter handling,
//! event budgets, and BOM handling.

use super::*;

#[test]
fn chat_transport_splits_are_equivalent() {
    let (map, token) = map_with("acme-世界");
    let changed_args =
        format!("{{\"city\":\"{token}\",\"note\":\"héllo \\\"q\\\" \\\\ \\u00e9\"}}");
    let unchanged_args = "{\"city\":\"héllo\",\"note\":\"q\\\" \\\\ \\u00e9\"}";
    let expected_args = changed_args.replace(&token, "acme-世界");
    let changed_event = tool_event(0, 0, &changed_args);
    let unchanged_event = tool_event(0, 0, unchanged_args);
    for terminator in ["\n\n", "\r\n\r\n", "\r\r"] {
        let expected = format!("{}{terminator}", tool_event(0, 0, &expected_args));
        let changed = format!("{changed_event}{terminator}");
        let mut whole = SseStreamRehydrator::new_chat_completions(map.clone());
        let emitted = whole.feed(changed.as_bytes());
        assert!(
            String::from_utf8_lossy(&emitted).contains("acme-世界"),
            "restored data is emitted during generation, not at finish"
        );
        assert!(whole.finish().is_empty());
        assert_eq!(emitted, expected.as_bytes());
        for split in 0..=changed.len() {
            let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
            let out = feed_chunks(
                &mut rehydrator,
                &[&changed.as_bytes()[..split], &changed.as_bytes()[split..]],
            );
            assert_eq!(out, expected.as_bytes(), "terminator {terminator:?} split at {split}");
        }

        let unchanged = format!("{unchanged_event}{terminator}");
        let baseline = feed_chunks(
            &mut SseStreamRehydrator::new_chat_completions(map.clone()),
            &[unchanged.as_bytes()],
        );
        assert_eq!(baseline, unchanged.as_bytes());
        for split in 0..=unchanged.len() {
            let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
            let out = feed_chunks(
                &mut rehydrator,
                &[&unchanged.as_bytes()[..split], &unchanged.as_bytes()[split..]],
            );
            assert_eq!(out, unchanged.as_bytes(), "unchanged split at {split}");
        }
    }
}

#[test]
fn chat_delimiter_split_does_not_swallow_next_frame() {
    let (map, token) = map_with("acme-corp");
    let args = format!("{{\"x\":\"{token}\"}}");
    let first = tool_event(0, 0, &args);
    let second = tool_event(1, 0, &args);
    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map);
    let mut out = rehydrator.feed(format!("{first}\r\n\r").as_bytes());
    out.extend(rehydrator.feed(format!("\n{second}\r\n\r\n").as_bytes()));
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches("acme-corp").count(), 2);
    assert_eq!(text.matches("chatcmpl-task2").count(), 2);
    assert!(rehydrator.finish().is_empty());
}

#[test]
fn chat_event_budget_matches_whole_and_piecemeal_delivery() {
    let (mut map, token) = map_with("acme-corp");
    let held = map.token_for(&TokenDeriver::new(b"task2-key"), "held-arg");
    let held_prefix = held[..12].to_string();
    let held_rest = held[12..].to_string();
    let content = format!("pre {token} post");
    let prepare = |rehydrator: &mut SseStreamRehydrator| {
        let pending = format!("{{\"x\":\"{held_prefix}");
        rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
    };

    let body = padded_text_event(MAX_EVENT_BUFFER_BYTES, &content);
    let frame = format!("{body}\n\n");
    let mut whole = SseStreamRehydrator::new_chat_completions(map.clone());
    prepare(&mut whole);
    let whole_out = whole.feed(frame.as_bytes());
    let mut split = SseStreamRehydrator::new_chat_completions(map.clone());
    prepare(&mut split);
    let mut split_out = split.feed(body.as_bytes());
    assert!(split_out.is_empty(), "an exact-cap body waits for its delimiter");
    split_out.extend(split.feed(b"\n\n"));
    assert_eq!(split_out, whole_out, "exact cap is transport independent");
    assert!(String::from_utf8_lossy(&split_out).contains("pre acme-corp post"));
    assert_eq!(split.report().restored, 1);
    assert_eq!(split.report().unrestored, 0);
    let tail = split.finish();
    assert!(position(&tail, held_prefix.as_bytes()) < tail.len());
    assert_eq!(split.report().unrestored, 1);
    assert_eq!(split.report().restored, 1);

    // Build one valid oversized JSON event and split it at cap+1. Both
    // pieces stay JSON bytes: opacity must come from the budget, not from a
    // parse failure.
    let body = padded_text_event(MAX_EVENT_BUFFER_BYTES + 65, &content);
    let split_at = MAX_EVENT_BUFFER_BYTES + 1;
    let first = &body.as_bytes()[..split_at];
    let remainder = &body.as_bytes()[split_at..];
    let frame = format!("{body}\n\n");
    let mut whole = SseStreamRehydrator::new_chat_completions(map.clone());
    prepare(&mut whole);
    let whole_out = whole.feed(frame.as_bytes());
    assert!(whole_out.ends_with(frame.as_bytes()));
    assert!(!String::from_utf8_lossy(&whole_out).contains("pre acme-corp post"));
    assert!(
        position(&whole_out, held_prefix.as_bytes()) < position(&whole_out, frame.as_bytes()),
        "flushed suffix precedes the over-cap raw frame"
    );
    assert_eq!(whole.report().unrestored, 2, "held prefix plus the raw full token");
    let mut split = SseStreamRehydrator::new_chat_completions(map.clone());
    prepare(&mut split);
    let mut split_out = split.feed(first);
    assert!(
        position(&split_out, held_prefix.as_bytes()) < position(&split_out, first),
        "flushed suffix precedes the over-cap raw prefix"
    );
    assert!(split_out.ends_with(first));
    assert!(!String::from_utf8_lossy(&split_out).contains("pre acme-corp post"));
    let raw = split.feed(remainder);
    assert_eq!(raw, remainder, "JSON remainder forwarded verbatim");
    split_out.extend(raw);
    let delimiter = split.feed(b"\n\n");
    assert_eq!(delimiter, b"\n\n", "delimiter forwarded verbatim");
    split_out.extend(delimiter);
    assert_eq!(split_out, whole_out, "over-cap delivery is transport independent");
    assert_eq!(split.report().unrestored, 2, "held prefix plus the raw full token");

    let completing = format!("{held_rest}\"}}");
    let completing_frame = format!("{}\n\n", tool_event(0, 0, &completing));
    let out = split.feed(completing_frame.as_bytes());
    assert_eq!(out, completing_frame.as_bytes(), "no cross-gap rewrite");
    assert!(!String::from_utf8_lossy(&out).contains("held-arg"));
    let healthy = format!("{}\n\n", text_event(&token));
    let out = split.feed(healthy.as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("acme-corp"));
    assert_eq!(split.report().restored, 1);
    assert_eq!(split.report().unrestored, 2);
    assert!(split.finish().is_empty());
    assert_eq!(split.report().restored, 1);
    assert_eq!(split.report().unrestored, 2);
}

#[test]
fn chat_split_delimiter_before_raw_gap_matches_whole_delivery() {
    let (map, token) = map_with("acme-corp");
    let prefix = token[..12].to_string();
    let pending = format!("{{\"x\":\"{prefix}");
    let whole_frame = format!("{}\r\n\r\n", tool_event(0, 0, &pending));
    let split_head = format!("{}\r\n\r", tool_event(0, 0, &pending));
    let opaque = b"data: \xff\xfe\r\n\r\n";
    let whole_input = [whole_frame.as_bytes(), opaque].concat();
    let split_rest = [b"\n".as_slice(), opaque].concat();
    let mut whole = SseStreamRehydrator::new_chat_completions(map.clone());
    let whole_out = whole.feed(&whole_input);
    let mut split = SseStreamRehydrator::new_chat_completions(map);
    let mut split_out = split.feed(split_head.as_bytes());
    split_out.extend(split.feed(&split_rest));
    assert_eq!(split_out, whole_out);
    assert!(position(&whole_out, prefix.as_bytes()) < position(&whole_out, opaque));
}

#[test]
fn chat_bom_is_forwarded_and_first_event_restores() {
    let (map, token) = map_with("acme-corp");
    let arguments = format!("{{\"x\":\"{token}\"}}");
    let frame = format!("\u{FEFF}{}\r\n\r\n", tool_event(0, 0, &arguments));
    let bytes = frame.as_bytes();
    for split in 0..=3 {
        let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
        let mut out = rehydrator.feed(&bytes[..split]);
        out.extend(rehydrator.feed(&bytes[split..]));
        out.extend(rehydrator.finish());
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with('\u{FEFF}'));
        assert_eq!(text.matches('\u{FEFF}').count(), 1);
        assert!(text.contains("acme-corp"));
    }
}

#[test]
fn chat_midstream_bom_does_not_dispatch_done() {
    let (map, token) = map_with("acme-corp");
    let (prefix, rest) = token.split_at(12);
    let mut driver = SseStreamRehydrator::new_chat_completions(map);
    driver.feed(format!("{}\n\n", tool_event(0, 0, &format!("{{\"x\":\"{prefix}"))).as_bytes());
    let midstream = b"\xEF\xBB\xBFdata: [DONE]\n\n";
    assert_eq!(driver.feed(midstream), midstream);
    let out = driver.feed(format!("{}\n\n", tool_event(0, 0, &format!("{rest}\"}}"))).as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("acme-corp"));
    assert_eq!(driver.report().restored, 1);
}

#[test]
fn chat_initial_bom_does_not_consume_event_budget() {
    let (map, token) = map_with("acme-corp");
    let body = format!("\u{FEFF}{}", padded_text_event(MAX_EVENT_BUFFER_BYTES, &token));
    let frame = format!("{body}\r\n\r\n");
    let mut whole = SseStreamRehydrator::new_chat_completions(map.clone());
    let expected = whole.feed(frame.as_bytes());
    assert!(expected.starts_with(UTF8_BOM));
    assert!(String::from_utf8_lossy(&expected).contains("acme-corp"));
    let mut split = SseStreamRehydrator::new_chat_completions(map.clone());
    assert!(split.feed(&body.as_bytes()[..1]).is_empty());
    assert!(split.feed(&body.as_bytes()[1..]).is_empty());
    assert_eq!(split.feed(b"\r\n\r\n"), expected);
    let mut complete = SseStreamRehydrator::new_chat_completions(map);
    assert_eq!(complete.feed_complete_event(frame.as_bytes()), expected);
}

#[test]
fn default_driver_delimiter_split_preserves_bytes() {
    let (map, token) = map_with("acme-corp");
    let args = format!("{{\"x\":\"{token}\"}}");
    let first = format!("{}\r\n\r", tool_event(0, 0, &args));
    let second = format!("{}\r\n\r\n", text_event("visible"));
    let mut rehydrator = SseStreamRehydrator::new(map);
    let out = rehydrator.feed(first.as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("acme-corp"));
    assert!(out.ends_with(b"\r\n\r"));
    assert!(rehydrator.is_idle());
    let out = rehydrator.feed(b"\n");
    assert!(out.is_empty(), "the default driver buffers the split LF");
    assert!(!rehydrator.is_idle(), "the buffered LF is pending work");
    let out = rehydrator.feed(second.as_bytes());
    assert_eq!(out, format!("\n{second}").as_bytes());
    assert!(out.ends_with(second.as_bytes()));
    assert!(rehydrator.finish().is_empty());
}
