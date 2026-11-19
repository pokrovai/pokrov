//! Bounded state: pair capacity, choice fences, tombstones, and per-event
//! entry limits.

use super::*;

#[test]
fn pair_capacity_and_state_budget() {
    assert_eq!(MAX_ARGUMENT_PAIRS, 32);
    assert_eq!(MAX_CHOICE_FENCES, 32);
    assert_eq!(MAX_INNER_DEPTH, 32);
    assert_eq!(MAX_PENDING_PREFIX, 81);
    assert_eq!(MAX_TOKEN_BYTES, 82);
    let (map, tokens) = map_for(&["capacity-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map);
    let mut last =
        harness.feed(&long_envelope_event(0, &[(0, &format!("{{\"v\":\"{}", &token[..40]))]));
    for pair in 1..33u64 {
        last = harness.feed(&event(
            pair / 2,
            &[(pair % 2, &format!("{{\"v\":\"{}", &token[..40]))],
            false,
        ));
    }
    let streams = harness.rehydrator.chat_arguments.as_ref().unwrap();
    assert_eq!(streams.streams.len(), 32);
    assert!(!streams.admissions_open);
    assert!(arg_for(&last[0], 16, 0).unwrap().contains(&token[..40]));
    let complete = event(0, &[(0, &format!("{}\"}}", &token[40..]))], false);
    assert!(!harness.feed(&complete).is_empty());
    assert_eq!(harness.report().restored, 1);
    let rejected = event(20, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&rejected), rejected);
    for choice in 16..48u64 {
        harness.rehydrator.rehydrate_event(&finish_event(choice));
    }
    let null_tools: Vec<Value> =
        (0..32).map(|i| json!({"index": i, "function": {"arguments": null}})).collect();
    harness.rehydrator.rehydrate_event(&format!(
        "data: {}",
        json!({"choices": [{"index": 48, "delta": {"tool_calls": null_tools}}]})
    ));
    let state = harness.rehydrator.chat_arguments.as_ref().unwrap();
    let envelope = state.envelope.as_ref().expect("bounded envelope retained");
    assert_eq!(
        (envelope.id.as_ref().unwrap().len(), envelope.model.as_ref().unwrap().len()),
        (MAX_ENVELOPE_FIELD_BYTES, MAX_ENVELOPE_FIELD_BYTES)
    );
    let retained = std::mem::size_of::<ArgumentStreams>()
        + state.streams.capacity() * std::mem::size_of::<(ArgumentKey, ArgumentDeltaState)>()
        + state.closed_pairs.capacity() * std::mem::size_of::<ArgumentKey>()
        + state.choice_fences.capacity() * std::mem::size_of::<u64>()
        + state.envelope.as_ref().map_or(0, |e| {
            e.id.as_ref().map_or(0, String::capacity) + e.model.as_ref().map_or(0, String::capacity)
        });
    assert!(retained <= 8192, "retained tool state {retained} exceeds 8 KiB");
}

#[test]
fn choice_fences_bound_new_admissions() {
    let (map, tokens) = map_for(&["fence-token"]);
    let mut harness = Harness::new(map);
    for choice in 0..MAX_CHOICE_FENCES as u64 {
        harness.rehydrator.rehydrate_event(&finish_event(choice));
    }
    assert_eq!(
        harness.rehydrator.chat_arguments.as_ref().unwrap().choice_fences.len(),
        MAX_CHOICE_FENCES
    );
    assert!(harness.rehydrator.chat_arguments.as_ref().unwrap().admissions_open);
    harness.rehydrator.rehydrate_event(&finish_event(MAX_CHOICE_FENCES as u64));
    let streams = harness.rehydrator.chat_arguments.as_ref().unwrap();
    assert!(!streams.admissions_open);
    assert_eq!(streams.choice_fences.len(), MAX_CHOICE_FENCES);
    let line = event(100, &[(0, &format!("{{\"v\":\"{}\"}}", tokens[0]))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&line), line);
}

#[test]
fn finished_choice_frees_slots_before_admission() {
    let (map, tokens) = map_for(&["free-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map);
    for pair in 0..32u64 {
        harness.rehydrator.rehydrate_event(&event(
            pair / 2,
            &[(pair % 2, &format!("{{\"v\":\"{}", &token[..20]))],
            false,
        ));
    }
    assert_eq!(harness.rehydrator.chat_arguments.as_ref().unwrap().streams.len(), 32);
    let combined = format!(
        "data: {}",
        json!({"choices": [
            {"index": 16, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": format!("{{\"v\":\"{}", &token[..20])}}]}},
            {"index": 0, "delta": {}, "finish_reason": "stop"}
        ]})
    );
    harness.rehydrator.rehydrate_event(&combined);
    assert_eq!(harness.rehydrator.chat_arguments.as_ref().unwrap().streams.len(), 31);
    assert!(harness.rehydrator.chat_arguments.as_ref().unwrap().admissions_open);
    harness.feed(&event(16, &[(0, &format!("{}\"}}", &token[20..]))], false));
    assert_eq!(harness.report().restored, 1);
}

#[test]
fn finished_choice_clears_pair_tombstones() {
    let (map, tokens) = map_for(&["tombstone-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map.clone());
    let null_tools: Vec<Value> =
        (0..32).map(|i| json!({"index": i, "function": {"arguments": null}})).collect();
    harness.rehydrator.rehydrate_event(&format!(
        "data: {}",
        json!({"choices": [{"index": 0, "delta": {"tool_calls": null_tools}}]})
    ));
    assert_eq!(harness.rehydrator.chat_arguments.as_ref().unwrap().closed_pairs.len(), 32);
    harness.rehydrator.rehydrate_event(&finish_event(0));
    assert!(harness.rehydrator.chat_arguments.as_ref().unwrap().closed_pairs.is_empty());
    let bad = format!(
        "data: {}",
        json!({"choices": [{"index": 1, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": null}}]}}]})
    );
    harness.rehydrator.rehydrate_event(&bad);
    let events = harness.feed(&event(1, &[(1, &format!("{{\"v\":\"{token}\"}}"))], false));
    assert_eq!(
        serde_json::from_str::<Value>(&assembled(&events, 1, 1)).unwrap()["v"],
        "tombstone-token"
    );
    assert_eq!(harness.report().restored, 1);
    let mut harness = Harness::new(map);
    harness.rehydrator.rehydrate_event(&format!(
        "data: {}",
        json!({"choices": [{"index": 0, "delta": {"tool_calls": null_tools.clone()}}]})
    ));
    let mut final_tools = null_tools.clone();
    final_tools.extend((0..32).map(|i| json!({"index": 32 + i, "function": {"arguments": "]"}})));
    let combined = format!(
        "data: {}",
        json!({"choices": [
            {"index": 0, "delta": {"tool_calls": final_tools}, "finish_reason": "stop"},
            {"index": 1, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": null}}]}}
        ]})
    );
    harness.rehydrator.rehydrate_event(&combined);
    assert!(harness.rehydrator.chat_arguments.as_ref().unwrap().admissions_open);
    let events = harness.feed(&event(2, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false));
    assert_eq!(
        serde_json::from_str::<Value>(&assembled(&events, 2, 0)).unwrap()["v"],
        "tombstone-token"
    );
    assert_eq!(harness.report().restored, 1);
}

#[test]
fn bounds_state_without_using_text_carry_slots() {
    let (map, tokens) = map_for(&["tool-token", "text-token"]);
    let (tool_token, text_token) = (&tokens[0], &tokens[1]);
    let mut harness = Harness::new(map);
    for pair in 0..32u64 {
        harness.rehydrator.rehydrate_event(&event(
            pair / 2,
            &[(pair % 2, &format!("{{\"v\":\"{}", &tool_token[..20]))],
            false,
        ));
    }
    for choice in 0..128u64 {
        harness.rehydrator.rehydrate_event(&content_event(choice, &text_token[..30]));
    }
    assert_eq!(harness.rehydrator.delta_carry.len(), 128);
    for choice in 0..128u64 {
        harness.rehydrator.rehydrate_event(&content_event(choice, &text_token[30..]));
    }
    assert_eq!(harness.report().restored, 128);
    assert!(harness.rehydrator.delta_carry.is_empty());
    let rejected = event(100, &[(0, &format!("{{\"v\":\"{tool_token}\"}}"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&rejected), rejected);
}

#[test]
fn over_limit_events_are_opaque() {
    let (map, tokens) = map_for(&["limit-token"]);
    let token = &tokens[0];
    let choices = |count: u64| {
        format!(
            "data: {}",
            json!({"choices": (0..count).map(|i| json!({"index": i, "delta": {"content": &token[..30]}})).collect::<Vec<_>>()})
        )
    };
    let overflow = choices(129);
    assert_eq!(Harness::new(map.clone()).rehydrator.rehydrate_event(&overflow), overflow);
    let mut harness = Harness::new(map.clone());
    harness.rehydrator.rehydrate_event(&choices(128));
    assert_eq!(harness.rehydrator.delta_carry.len(), 128);
    let tools = |count: u64| {
        std::iter::once(Value::Null)
            .chain((1..count).map(|i| json!({"index": i, "function": {"arguments": ""}})))
            .collect::<Vec<_>>()
    };
    let payload =
        json!({"choices": [{"index": 0, "delta": {"tool_calls": tools(128)}}]}).to_string();
    assert!(has_ambiguous_chat_tool_members(&payload));
    let line = format!("data: {payload}");
    assert_eq!(Harness::new(map.clone()).rehydrator.rehydrate_event(&line), line);
    let within = format!(
        "data: {}",
        json!({"choices": [{"index": 0, "delta": {"tool_calls": tools(127)}}]})
    );
    assert_eq!(Harness::new(map).rehydrator.rehydrate_event(&within), within);
}
