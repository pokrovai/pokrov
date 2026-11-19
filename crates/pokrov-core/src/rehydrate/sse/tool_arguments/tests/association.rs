//! Pair isolation, index validation, duplicate rejection, and quarantine
//! associations.

use super::*;

#[test]
fn isolates_interleaved_pairs_and_member_order() {
    let (map, tokens) = map_for(&["alpha-token", "beta-token", "gamma-token"]);
    let (alpha, beta, gamma) = (&tokens[0], &tokens[1], &tokens[2]);
    let mut harness = Harness::new(map);
    let mut events = Vec::new();
    events.extend(harness.feed(&event(0, &[(0, &format!("{{\"v\":\"{}", &alpha[..10]))], false)));
    events.extend(harness.feed(&event(0, &[(1, &format!("{{\"v\":\"{beta}\"}}"))], false)));
    events.extend(harness.feed(&event(1, &[(0, &format!("{{\"v\":\"{}", &gamma[..20]))], false)));
    events.extend(harness.feed(&event(0, &[(0, &format!("{}\"}}", &alpha[10..]))], false)));
    events.extend(harness.feed(&event(1, &[(0, &format!("{}\"}}", &gamma[20..]))], false)));
    assert_eq!(
        serde_json::from_str::<Value>(&assembled(&events, 0, 0)).unwrap()["v"],
        "alpha-token"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&assembled(&events, 0, 1)).unwrap()["v"],
        "beta-token"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&assembled(&events, 1, 0)).unwrap()["v"],
        "gamma-token"
    );
    let (map, tokens) = map_for(&["order-token"]);
    let args = serde_json::to_string(&format!("{{\"v\":\"{}\"}}", tokens[0])).unwrap();
    let line = format!("data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"function\":{{\"arguments\":{args}}},\"index\":0}}]}}}}]}}");
    let events = Harness::new(map).feed(&line);
    assert_eq!(
        serde_json::from_str::<Value>(&assembled(&events, 0, 0)).unwrap()["v"],
        "order-token"
    );
    let (map, tokens) = map_for(&["multi-a", "multi-b"]);
    let mut harness = Harness::new(map);
    let events = harness.feed(&event(
        0,
        &[
            (0, &format!("{{\"v\":\"{}\"}}", tokens[0])),
            (1, &format!("{{\"v\":\"{}\"}}", tokens[1])),
        ],
        false,
    ));
    assert_eq!(serde_json::from_str::<Value>(&assembled(&events, 0, 0)).unwrap()["v"], "multi-a");
    assert_eq!(serde_json::from_str::<Value>(&assembled(&events, 0, 1)).unwrap()["v"], "multi-b");
    let choice_event = format!(
        "data: {}",
        json!({"choices": [
            {"index": 2, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": format!("{{\"v\":\"{}\"}}", tokens[0])}}]}},
            {"index": 3, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": format!("{{\"v\":\"{}\"}}", tokens[1])}}]}}
        ]})
    );
    let events = harness.feed(&choice_event);
    assert_eq!(serde_json::from_str::<Value>(&assembled(&events, 2, 0)).unwrap()["v"], "multi-a");
    assert_eq!(serde_json::from_str::<Value>(&assembled(&events, 3, 0)).unwrap()["v"], "multi-b");
    assert_eq!(harness.report().restored, 4);
}

#[test]
fn leaves_keys_unknown_tokens_and_ambiguous_indices_literal() {
    let (map, tokens) = map_for(&["key-token"]);
    let mut harness = Harness::new(map.clone());
    let key_event = event(0, &[(0, &format!("{{\"{}\":\"value\"}}", tokens[0]))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&key_event), key_event);
    assert_eq!(harness.report().restored, 0);
    let unknown = format!("{{\"v\":\"__PKV_{}__\"}}", "f".repeat(64));
    let mut harness = Harness::new(map.clone());
    let unknown_event = event(0, &[(0, &unknown)], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&unknown_event), unknown_event);
    assert_eq!(harness.report().restored, 0);
    assert_eq!(harness.report().unrestored, 1);
    let mut harness = Harness::new(map);
    let first = harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{{\"v\":\"{}", &tokens[0][..40]))],
        false,
    ));
    let metadata = format!(
        "data: {}",
        json!({"choices": [{"delta": {"tool_calls": [{"id": "c", "function": {"name": "f"}}]}}]})
    );
    let out = harness.rehydrator.rehydrate_event(&metadata);
    let (_, body) = out.split_once("\n\n").expect("held suffix flushes first");
    assert_eq!(body, metadata);
    assert_eq!(out.matches(&tokens[0][..40]).count(), 1);
    let complete = event(0, &[(0, &format!("{}\"}}", &tokens[0][40..]))], false);
    let complete_out = harness.rehydrator.rehydrate_event(&complete);
    assert_eq!(complete_out, complete);
    let inner: String = [&first, &out, &complete_out]
        .into_iter()
        .flat_map(|text| parse_events(text))
        .filter_map(|event| arg_for(&event, 0, 0).map(str::to_owned))
        .collect();
    assert_eq!(serde_json::from_str::<Value>(&inner).unwrap()["v"], tokens[0].as_str());
    assert_eq!(harness.report().restored, 0);
    assert_eq!(harness.report().unrestored, 1);
}

#[test]
fn rejects_invalid_and_repeated_indices() {
    let (map, tokens) = map_for(&["index-token"]);
    let token = &tokens[0];
    let bad_args = serde_json::to_string(&format!("{{\"v\":\"{token}\"}}")).unwrap();
    for index in ["-1", "0.5", "\"0\""] {
        let line = format!("data: {{\"choices\":[{{\"index\":{index},\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{bad_args}}}}}]}}}}]}}");
        let mut harness = Harness::new(map.clone());
        assert_eq!(harness.rehydrator.rehydrate_event(&line), line, "choice index {index}");
        assert_eq!(harness.report().restored, 0);
    }
    let missing = format!("data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{bad_args}}}}}]}}}}]}}");
    assert_eq!(Harness::new(map.clone()).rehydrator.rehydrate_event(&missing), missing);
    for index in [None, Some(json!(-1)), Some(json!(0.5)), Some(json!("0"))] {
        let mut tool = json!({"function": {"arguments": format!("{{\"v\":\"{token}\"}}")}});
        if let Some(index) = index {
            tool["index"] = index;
        }
        let gap = format!(
            "data: {}",
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [tool]}}]})
        );
        let mut harness = Harness::new(map.clone());
        let prefix = format!("{{\"v\":\"{}", &token[..40]);
        let tail = format!("{}\"}}", &token[40..]);
        for choice in [0, 1] {
            harness.feed(&event(choice, &[(0, &prefix)], false));
        }
        let out = harness.rehydrator.rehydrate_event(&gap);
        let (flush, body) = out.split_once("\n\n").unwrap();
        assert_eq!(body, gap);
        let chunks = parse_events(flush);
        assert_eq!(chunks[0]["choices"].as_array().unwrap().len(), 1);
        assert_eq!(arg_for(&chunks[0], 0, 0), Some(&token[..40]));
        let affected = event(0, &[(0, &tail)], false);
        assert_eq!(harness.rehydrator.rehydrate_event(&affected), affected);
        let unaffected = harness.feed(&event(1, &[(0, &tail)], false));
        assert_eq!(arg_for(&unaffected[0], 1, 0), Some("index-token\"}"));
        assert!(harness.rehydrator.finish().is_empty());
        assert_eq!((harness.report().restored, harness.report().unrestored), (1, 2));
    }
    for (repeated, label) in [
        (format!("data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{bad_args}}}}}]}}}}}},{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{bad_args}}}}}]}}}}]}}"), "choice"),
        (format!("data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{bad_args}}}}},{{\"index\":0,\"function\":{{\"arguments\":{bad_args}}}}}]}}}}]}}"), "tool"),
    ] {
        let mut harness = Harness::new(map.clone());
        harness.feed(&event(0, &[(0, &format!("{{\"v\":\"{}", &token[..40]))], false));
        let out = harness.rehydrator.rehydrate_event(&repeated);
        let (prelude, body) = out.split_once("\n\n").unwrap_or_else(|| panic!("duplicate {label} must flush first"));
        assert!(prelude.contains(&token[..40])); assert_eq!(body, repeated); assert_eq!(harness.report().restored, 0);
        let late = event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
        assert_eq!(harness.rehydrator.rehydrate_event(&late), late);
    }
}

#[test]
fn non_string_arguments_quarantine_pair() {
    let (map, tokens) = map_for(&["non-string-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map);
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{{\"v\":\"{}", &token[..40]))],
        false,
    ));
    let invalid = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":null}}]}}]}";
    let out = harness.rehydrator.rehydrate_event(invalid);
    let (prelude, body) = out.split_once("\n\n").expect("held prefix flushes first");
    assert!(prelude.contains(&token[..40]));
    assert_eq!(body, invalid);
    assert_eq!(harness.report().restored, 0);
    let later = event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&later), later);
    assert_eq!(harness.report().restored, 0);
}
