//! Opaque frames: flush-before-raw ordering, continuity breakage, and
//! complete-event bypass/restore paths.

use super::*;

#[test]
fn chat_opaque_frames_flush_then_break_continuity() {
    let (map, token) = map_with("acme-corp");
    let prefix = token[..12].to_string();
    let rest = token[12..].to_string();
    let pending = format!("{{\"x\":\"{prefix}");
    let completing = format!("{rest}\"}}");
    let malformed = b"data: {\"not-json\"}\n\n".to_vec();
    let multi = b"data: {\"a\":1}\ndata: {\"b\":2}\n\n".to_vec();
    let invalid = b"data: \xff\xfe\n\n".to_vec();
    let error_control = b"event: error\ndata: {\"message\":\"boom\"}\n\n".to_vec();
    let error_object = b"data: {\"error\":{\"message\":\"boom\"}}\n\n".to_vec();
    let error_unusable = b"data: {\"error\":{\"message\":\"boom\"},\"choices\":[{}]}\n\n".to_vec();
    let error_null_finish =
        b"data: {\"error\":{},\"choices\":[{\"index\":0,\"finish_reason\":null}]}\n\n".to_vec();
    let bare_data = b"data\n\n".to_vec();
    let mixed_bare = b"data\ndata: {\"a\":1}\n\n".to_vec();
    let frames: [(&str, &[u8], &[u8]); 9] = [
        ("malformed", &malformed, b"not-json"),
        ("multi", &multi, b"data: {\"a\":1}"),
        ("invalid", &invalid, b"\xff\xfe"),
        ("error-control", &error_control, b"boom"),
        ("error-object", &error_object, b"boom"),
        ("error-unusable", &error_unusable, b"\"choices\":[{}]"),
        ("error-null-finish", &error_null_finish, b"\"error\":{}"),
        ("bare-data", &bare_data, b"data\n\n"),
        ("mixed-bare", &mixed_bare, b"data\ndata:"),
    ];
    for (label, frame, marker) in frames {
        for split in [frame.len(), frame.len() / 2] {
            let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
            let first = format!("{}\n\n", tool_event(0, 0, &pending));
            let out = rehydrator.feed(first.as_bytes());
            assert!(!String::from_utf8_lossy(&out).contains(prefix.as_str()), "{label}: held");
            assert_eq!(rehydrator.report().unrestored, 0, "{label}: nothing flushed yet");

            let mut out = rehydrator.feed(&frame[..split]);
            out.extend(rehydrator.feed(&frame[split..]));
            assert!(
                position(&out, prefix.as_bytes()) < position(&out, marker),
                "{label}: literal suffix must precede the raw frame"
            );
            assert_eq!(
                out.windows(frame.len()).filter(|window| *window == frame).count(),
                1,
                "{label}: raw frame forwarded once"
            );

            let completing = format!("{}\n\n", tool_event(0, 0, &completing));
            let out = rehydrator.feed(completing.as_bytes());
            let text = String::from_utf8(out).unwrap();
            assert!(!text.contains("acme-corp"), "{label}: no join across the gap");
            assert!(text.contains(rest.as_str()), "{label}: held rest stays literal");

            let healthy = format!("{}\n\n", text_event(&token));
            let out = rehydrator.feed(healthy.as_bytes());
            assert!(
                String::from_utf8_lossy(&out).contains("acme-corp"),
                "{label}: text continuity survives"
            );
            assert!(rehydrator.finish().is_empty());
            assert_eq!(rehydrator.report().restored, 1, "{label}: one restore");
        }
    }
}

#[test]
fn chat_bypass_complete_event_flushes_then_breaks_continuity() {
    let (mut map, token) = map_with("acme-corp");
    let text_token = map.token_for(&TokenDeriver::new(b"task2-key"), "globex");
    let prefix = token[..12].to_string();
    let rest = token[12..].to_string();
    let (text_prefix, text_rest) = text_token.split_at(12);
    let pending = format!("{{\"x\":\"{prefix}");
    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map);
    rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
    rehydrator.feed(format!("{}\n\n", text_event(text_prefix)).as_bytes());

    let frame = b"data: {\"opaque\":true}\n\n";
    let out = rehydrator.bypass_complete_event(frame);
    assert!(position(&out, prefix.as_bytes()) < position(&out, frame));
    assert!(out.ends_with(frame));
    assert!(
        !String::from_utf8_lossy(&out).contains(text_prefix),
        "source ambiguity keeps text carries pending"
    );

    let out = rehydrator.feed(format!("{}\n\n", text_event(text_rest)).as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("globex"));
    let completing = format!("{rest}\"}}");
    let out = rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &completing)).as_bytes());
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("acme-corp"));
    assert!(text.contains(rest.as_str()));
}

#[test]
fn chat_bypass_source_oversize_flushes_arguments_then_text() {
    let (mut map, argument_token) = map_with("acme-corp");
    let text_token = map.token_for(&TokenDeriver::new(b"task2-key"), "globex");
    let argument_prefix = &argument_token[..12];
    let (text_prefix, text_rest) = text_token.split_at(12);
    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map);
    let pending = format!("{{\"x\":\"{argument_prefix}");
    rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
    rehydrator.feed(format!("{}\n\n", text_event(text_prefix)).as_bytes());

    let frame = format!("{}\n\n", padded_text_event(MAX_EVENT_BUFFER_BYTES + 1, "keepalive"));
    let out = rehydrator.bypass_source_oversize_event(frame.as_bytes());
    assert!(out.ends_with(frame.as_bytes()));
    assert_eq!(out.windows(frame.len()).filter(|window| *window == frame.as_bytes()).count(), 1);
    assert!(position(&out, argument_prefix.as_bytes()) < position(&out, text_prefix.as_bytes()));
    assert!(position(&out, text_prefix.as_bytes()) < position(&out, frame.as_bytes()));

    let rest_event = format!("{text_rest} after");
    let out = rehydrator.feed(format!("{}\n\n", text_event(&rest_event)).as_bytes());
    assert!(!String::from_utf8_lossy(&out).contains("globex"));
    assert!(String::from_utf8_lossy(&out).contains(text_rest));
    let out = rehydrator.feed(format!("{}\n\n", text_event(&text_token)).as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("globex"));
    assert_eq!(rehydrator.report().restored, 1);
}

#[test]
fn chat_feed_complete_event_restores_over_budget_frame() {
    let (map, token) = map_with("acme-corp");
    let content = format!("pre {token} post");
    // Models a source-admissible frame grown past the budget by policy
    // re-encoding; the caller guarantees the complete frame.
    let frame = format!("{}\n\n", padded_text_event(MAX_EVENT_BUFFER_BYTES + 64, &content));
    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
    let out = rehydrator.feed_complete_event(frame.as_bytes());
    assert!(String::from_utf8_lossy(&out).contains("pre acme-corp post"));
    assert!(!String::from_utf8_lossy(&out).contains(token.as_str()));
    assert!(rehydrator.finish().is_empty());
    let mut capped = SseStreamRehydrator::new_chat_completions(map);
    assert_eq!(capped.feed(frame.as_bytes()), frame.as_bytes());
}

#[test]
fn chat_feed_complete_event_keeps_opaque_frames_raw() {
    let (map, token) = map_with("acme-corp");
    let prefix = token[..12].to_string();
    let rest = token[12..].to_string();
    let pending = format!("{{\"x\":\"{prefix}");
    let frames: [&[u8]; 4] = [
        b"data: \xff\xfe\n\n",
        b"data: {\"a\":1}\ndata: {\"b\":2}\n\n",
        b"data: {\"error\":{}}\n\n",
        b"data: {nope\n\n",
    ];
    for frame in frames {
        let mut rehydrator = SseStreamRehydrator::new_chat_completions(map.clone());
        rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &pending)).as_bytes());
        let out = rehydrator.feed_complete_event(frame);
        assert!(position(&out, prefix.as_bytes()) < position(&out, frame));
        assert!(out.ends_with(frame));
        let completing = format!("{rest}\"}}");
        let out = rehydrator.feed(format!("{}\n\n", tool_event(0, 0, &completing)).as_bytes());
        assert!(!String::from_utf8_lossy(&out).contains("acme-corp"));
        assert!(String::from_utf8_lossy(&out).contains(rest.as_str()));
    }
}
