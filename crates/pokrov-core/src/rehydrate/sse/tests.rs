use super::*;
use crate::rehydrate::TokenDeriver;

fn map_with(fragment: &str) -> (RehydrationMap, String) {
    let deriver = TokenDeriver::new(b"test-key");
    let mut map = RehydrationMap::new();
    let token = map.token_for(&deriver, fragment);
    (map, token)
}

fn delta_event(content: &str) -> String {
    format!("data: {{\"object\":\"chat.completion.chunk\",\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}")
}

fn indexed_delta_event(index: i64, content: &str) -> String {
    format!("data: {{\"object\":\"chat.completion.chunk\",\"choices\":[{{\"index\":{index},\"delta\":{{\"content\":\"{content}\"}}}}]}}")
}

fn responses_delta_event(content: &str) -> String {
    format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"{content}\"}}")
}

#[test]
fn event_rehydrator_restores_token_split_across_delta_events() {
    let (map, token) = map_with("acme-corp");
    let (first, second) = token.split_at(token.len() / 2);
    let mut rehydrator = EventRehydrator::new(map);

    // First delta ends mid-token: the carry holds the prefix back and the
    // client sees nothing yet.
    let out1 = rehydrator.rehydrate_event(&delta_event(first));
    let parsed1: Value = serde_json::from_str(out1.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed1["choices"][0]["delta"]["content"], "");
    assert_eq!(rehydrator.report().unrestored, 0);

    // Second delta completes the token: client receives the fragment.
    let out2 = rehydrator.rehydrate_event(&delta_event(second));
    let parsed2: Value = serde_json::from_str(out2.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed2["choices"][0]["delta"]["content"], "acme-corp");
    assert_eq!(rehydrator.report().restored, 1);
}

#[test]
fn event_rehydrator_keeps_carries_isolated_per_choice_index() {
    let (mut map, token_zero) = map_with("acme-corp");
    let token_one = map.token_for(&TokenDeriver::new(b"test-key"), "globex");
    let (z_first, z_second) = token_zero.split_at(token_zero.len() / 2);
    let (o_first, o_second) = token_one.split_at(token_one.len() / 2);
    let mut rehydrator = EventRehydrator::new(map);

    // Interleave partial tokens of choice 0 and choice 1: each carry must
    // join only its own index.
    let out = rehydrator.rehydrate_event(&indexed_delta_event(0, z_first));
    let parsed: Value = serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], "");

    let out = rehydrator.rehydrate_event(&indexed_delta_event(1, o_first));
    let parsed: Value = serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], "");

    let out = rehydrator.rehydrate_event(&indexed_delta_event(1, o_second));
    let parsed: Value = serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], "globex");

    let out = rehydrator.rehydrate_event(&indexed_delta_event(0, z_second));
    let parsed: Value = serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], "acme-corp");
    assert_eq!(rehydrator.report().restored, 2);
}

#[test]
fn event_rehydrator_flushes_only_terminated_choice_carry() {
    let (mut map, token_zero) = map_with("acme-corp");
    map.token_for(&TokenDeriver::new(b"test-key"), "globex");
    let partial_zero = &token_zero[..12];
    let mut rehydrator = EventRehydrator::new(map);

    rehydrator.rehydrate_event(&indexed_delta_event(0, partial_zero));
    rehydrator.rehydrate_event(&indexed_delta_event(1, "visible"));

    // Choice 0 finishes mid-token: its carry flushes as a synthetic event
    // with the matching index while choice 1 is unaffected.
    let out = rehydrator.rehydrate_event(
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}",
    );
    let (synthetic, terminal) = out.split_once("\n\n").unwrap();
    let parsed: Value = serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(parsed["choices"][0]["index"], 0);
    assert_eq!(parsed["choices"][0]["delta"]["content"], partial_zero);
    assert!(terminal.contains("finish_reason"));
}

#[test]
fn event_rehydrator_carries_responses_output_text_delta() {
    let (map, token) = map_with("acme-corp");
    let (first, second) = token.split_at(token.len() / 2);
    let mut rehydrator = EventRehydrator::new(map);

    let out = rehydrator.rehydrate_event(&responses_delta_event(first));
    let parsed: Value = serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed["delta"], "");

    let out = rehydrator.rehydrate_event(&responses_delta_event(second));
    let parsed: Value = serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
    assert_eq!(parsed["delta"], "acme-corp");
    assert_eq!(rehydrator.report().restored, 1);
}

#[test]
fn event_rehydrator_flushes_responses_carry_on_done_type() {
    let (map, token) = map_with("acme-corp");
    let partial = &token[..12];
    let mut rehydrator = EventRehydrator::new(map);

    rehydrator.rehydrate_event(&responses_delta_event(partial));
    let out = rehydrator
        .rehydrate_event("data: {\"type\":\"response.output_text.done\",\"text\":\"done\"}");

    let (synthetic, done) = out.split_once("\n\n").unwrap();
    let parsed: Value = serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(parsed["type"], "response.output_text.delta");
    assert_eq!(parsed["delta"], partial);
    assert!(done.contains("response.output_text.done"));
}

#[test]
fn event_rehydrator_escapes_restored_fragment_inside_json() {
    let (map, token) = map_with("acme\"corp\nsuffix");
    let mut rehydrator = EventRehydrator::new(map);

    let out = rehydrator.rehydrate_event(&delta_event(&token));
    let payload = out.strip_prefix("data: ").unwrap();
    let parsed: Value = serde_json::from_str(payload).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], "acme\"corp\nsuffix");
    // The raw line must contain the escaped form, never a literal newline.
    assert!(!payload.contains('\n'));
    assert!(payload.contains("\\n"));
}

#[test]
fn event_rehydrator_flushes_carry_before_done() {
    let (map, token) = map_with("acme-corp");
    let partial = &token[..10];
    let mut rehydrator = EventRehydrator::new(map);

    rehydrator.rehydrate_event(&delta_event(partial));
    let out = rehydrator.rehydrate_event("data: [DONE]");

    // The held prefix is delivered as its own delta event ahead of [DONE].
    let (synthetic, done) = out.split_once("\n\n").unwrap();
    let parsed: Value = serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], partial);
    assert_eq!(done, "data: [DONE]");
}

#[test]
fn event_rehydrator_reverts_fragment_that_would_break_framing() {
    let (map, token) = map_with("acme\ncorp");
    let mut rehydrator = EventRehydrator::new(map);

    // Non-JSON data line: restoring would inject a real newline.
    let line = format!("data: RAW {token} TAIL");
    let out = rehydrator.rehydrate_event(&line);
    assert_eq!(out, line);
    let report = rehydrator.report();
    assert_eq!(report.restored, 0);
    assert!(report.unrestored >= 1);
}

#[test]
fn stream_rehydrator_restores_token_split_across_byte_chunks() {
    let (map, token) = map_with("acme-corp");
    let event = format!("{}\n\n", delta_event(&token));
    let mut rehydrator = SseStreamRehydrator::new(map);

    let (first, second) = event.split_at(event.len() / 2 + 5);
    assert!(rehydrator.feed(first.as_bytes()).is_empty());
    let out = rehydrator.feed(second.as_bytes());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("acme-corp"), "restored body must contain the fragment");
    assert!(rehydrator.finish().is_empty());
}

#[test]
fn stream_rehydrator_emits_crlf_terminated_events() {
    let (map, token) = map_with("acme-corp");
    let mut rehydrator = SseStreamRehydrator::new(map);

    // CRLF framing: the event must terminate on `\r\n\r\n`, not hang
    // waiting for `\n\n`.
    let event = format!("{}\r\n\r\n", delta_event(&token));
    let out = rehydrator.feed(event.as_bytes());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("acme-corp"), "CRLF event must restore the token");
    assert!(text.ends_with("\r\n\r\n"));
    assert!(rehydrator.finish().is_empty());
}

#[test]
fn stream_rehydrator_bounds_unterminated_event_buffer() {
    let (map, token) = map_with("acme-corp");
    let mut rehydrator = SseStreamRehydrator::new(map);

    // An oversized event without a terminator flushes verbatim instead
    // of growing the buffer without bound.
    let mut flood = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 8];
    flood.extend_from_slice(token.as_bytes());
    let out = rehydrator.feed(&flood);
    assert_eq!(out.len(), flood.len());
    assert!(out.ends_with(token.as_bytes()));
    assert_eq!(rehydrator.report().unrestored, 1);
    assert!(rehydrator.is_idle());
}

#[test]
fn stream_rehydrator_drain_returns_tail_and_pending_carry() {
    let (map, token) = map_with("acme-corp");
    let partial = &token[..12];
    let mut rehydrator = SseStreamRehydrator::new(map);

    // A complete event holding a partial token (carry pending) followed
    // by an unterminated event fragment.
    rehydrator.feed(format!("{}\n\n", delta_event(partial)).as_bytes());
    rehydrator.feed(b"data: {\"cho");

    let drained = rehydrator.drain();
    let text = String::from_utf8(drained).unwrap();
    // Raw buffered bytes survive verbatim, then the carry flushes as a
    // synthetic delta so no received content is lost.
    assert!(text.starts_with("data: {\"cho"));
    let synthetic = text.split("\n\n").nth(1).expect("carry event follows");
    let parsed: Value = serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["content"], partial);
    assert!(rehydrator.finish().is_empty());
}

#[test]
fn stream_rehydrator_passes_oversized_event_remainder_verbatim() {
    let (map, token) = map_with("acme\ncorp");
    let mut rehydrator = SseStreamRehydrator::new(map);

    // Overflow mid-event, then the remainder arrives containing a full
    // token — it must NOT be restored into the already-emitted partial
    // JSON frame (the fragment contains a newline; substituting it as
    // plain text would corrupt the emitted bytes' framing anyway).
    let first = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 1];
    let out = rehydrator.feed(&first);
    assert_eq!(out.len(), first.len());

    let remainder = format!("tail {token} end\n\n");
    let out = rehydrator.feed(remainder.as_bytes());
    assert_eq!(String::from_utf8(out).unwrap(), remainder);
    assert!(rehydrator.report().unrestored >= 1);

    // After the terminator, normal restore resumes for the next event.
    let next = format!("{}\n\n", delta_event(&token));
    let out = rehydrator.feed(next.as_bytes());
    let text = String::from_utf8(out).unwrap();
    // The fragment contains a newline; inside a JSON delta leaf serde
    // escapes it, so the event restores fine.
    assert!(text.contains("acme\\ncorp"));
}

#[test]
fn stream_rehydrator_carry_flush_counts_unrestored_markers() {
    let (map, token) = map_with("acme-corp");
    let partial = &token[..12];
    let mut rehydrator = SseStreamRehydrator::new(map);
    rehydrator.feed(format!("{}\n\n", delta_event(partial)).as_bytes());
    assert_eq!(rehydrator.report().unrestored, 0);

    // The flushed carry emits a visible incomplete token marker — it is
    // unrestored and must be counted for audit/metrics.
    rehydrator.finish();
    assert_eq!(rehydrator.report().unrestored, 1);
}

#[test]
fn event_rehydrator_bounds_carry_key_count() {
    let (map, token) = map_with("acme-corp");
    let partial = &token[..12];
    let mut rehydrator = EventRehydrator::new(map);

    // Spray more distinct indices than the carry cap: extra legs get no
    // carry join and emit their partial markers unrestored instead.
    for index in 0..(MAX_CARRY_KEYS as i64 + 1) {
        rehydrator.rehydrate_event(&indexed_delta_event(index, partial));
    }
    assert_eq!(rehydrator.delta_carry.len(), MAX_CARRY_KEYS);
    // The last (over-cap) index emitted its marker unrestored.
    assert_eq!(rehydrator.report().unrestored, 1);
}

#[test]
fn stream_rehydrator_passthrough_keeps_split_boundary() {
    // Every terminator form is exercised with its boundary split across
    // the passthrough flush: the oversized event's trailing EOL byte is
    // retained, so the next event is not swallowed into passthrough.
    // `\r\n\r` alone is already a complete boundary (lone-CR blank line),
    // so only true splits of the terminator are exercised here.
    for (terminator_first, terminator_rest) in [("\n", "\n"), ("\r\n", "\r\n"), ("\r", "\r")] {
        let (map, token) = map_with("acme-corp");
        let mut rehydrator = SseStreamRehydrator::new(map);

        // Overflow mid-event; the flood's final byte is the first half
        // of the oversized event's terminator.
        let mut first = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 1];
        first.extend_from_slice(terminator_first.as_bytes());
        let out = rehydrator.feed(&first);
        // The trailing EOL prefix must stay buffered, not flushed.
        assert!(
            out.len() < first.len(),
            "terminator {terminator_first:?}{terminator_rest:?}: out {} vs first {}",
            out.len(),
            first.len()
        );

        let rest = format!("{}{}", terminator_rest, delta_event(&token));
        rehydrator.feed(rest.as_bytes());
        let out = rehydrator.feed(b"\n\n");
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("acme-corp"),
            "next event after overflow must restore, terminator {terminator_first:?}{terminator_rest:?}: {text}"
        );
        assert!(rehydrator.finish().is_empty());
    }
}

#[test]
fn stream_rehydrator_restores_event_with_terminator_split_at_any_point() {
    // The incremental cursor must re-examine the whole pending
    // terminator prefix: every split position of `\r\n\r\n` and `\r\r`
    // still yields the event on the second chunk, without overflow.
    for terminator in ["\r\n\r\n", "\r\r", "\n\n"] {
        let (map, token) = map_with("acme-corp");
        let event = format!("{delta}{terminator}", delta = delta_event(&token));
        for split in 0..=terminator.len() {
            let (head, tail) = event.split_at(event.len() - terminator.len() + split);
            let mut rehydrator = SseStreamRehydrator::new(map.clone());
            let mut out = rehydrator.feed(head.as_bytes());
            out.extend(rehydrator.feed(tail.as_bytes()));
            // The event must be emitted by `feed` as soon as the
            // terminator completes — not delayed to `finish` at EOF.
            let text = String::from_utf8(out).unwrap();
            assert!(
                text.contains("acme-corp"),
                "terminator {terminator:?} split at {split}: {text}"
            );
            rehydrator.finish();
        }
    }
}

#[test]
fn stream_rehydrator_rescans_only_appended_bytes() {
    // Regression guard for incremental scanning: a near-limit event fed
    // in small chunks must still terminate and restore correctly — the
    // cursor keeps the scan linear.
    let (map, token) = map_with("acme-corp");
    let mut rehydrator = SseStreamRehydrator::new(map);
    let mut out = Vec::new();
    for _ in 0..64 {
        out.extend(rehydrator.feed(&vec![b'x'; 1024]));
    }
    out.extend(rehydrator.feed(format!("{}\n\n", delta_event(&token)).as_bytes()));
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("acme-corp"));
}

#[test]
fn stream_rehydrator_forwards_non_utf8_events_verbatim() {
    let (map, _) = map_with("acme-corp");
    let mut rehydrator = SseStreamRehydrator::new(map);
    let block = b"data: \xff\xfe\x00\n\n".to_vec();
    let out = rehydrator.feed(&block);
    assert_eq!(out, block);
}

#[test]
fn stream_rehydrator_finish_flushes_carry_as_synthetic_delta() {
    let (map, token) = map_with("acme-corp");
    let partial = &token[..12];
    let mut rehydrator = SseStreamRehydrator::new(map);
    rehydrator.feed(format!("{}\n\n", delta_event(partial)).as_bytes());

    let tail = rehydrator.finish();
    let text = String::from_utf8(tail).unwrap();
    let event: Value =
        serde_json::from_str(text.strip_prefix("data: ").unwrap().trim_end()).unwrap();
    assert_eq!(event["choices"][0]["delta"]["content"], partial);
}
