//! Opaque fallbacks: gap ordering, duplicate members, syntax faults, byte
//! reuse, framing preservation, and ambiguity probing.

use super::*;

#[test]
fn flushes_gap_suffixes_in_global_index_order() {
    let (map, tokens) = map_for(&["order-a", "order-b"]);
    let mut harness = Harness::new(map);
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(5, &format!("{{\"v\":\"{}", &tokens[0][..20]))],
        false,
    ));
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(2, &format!("{{\"v\":\"{}", &tokens[0][..20]))],
        false,
    ));
    harness.rehydrator.rehydrate_event(&event(
        1,
        &[(0, &format!("{{\"v\":\"{}", &tokens[1][..20]))],
        false,
    ));
    let gaps = format!(
        "data: {}",
        json!({"choices": [
            {"index": 0, "delta": {}}, {"index": 0, "delta": {}},
            {"index": 1, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "a"}}, {"index": 0, "function": {"arguments": "b"}}]}}
        ]})
    );
    let out = harness.rehydrator.rehydrate_event(&gaps);
    let blocks: Vec<&str> = out.split("\n\n").collect();
    assert_eq!(blocks.len(), 3);
    let first: Value = serde_json::from_str(blocks[0].strip_prefix("data: ").unwrap()).unwrap();
    let second: Value = serde_json::from_str(blocks[1].strip_prefix("data: ").unwrap()).unwrap();
    let first_tools: Vec<u64> = first["choices"][0]["delta"]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["index"].as_u64().unwrap())
        .collect();
    assert_eq!(first_tools, vec![2, 5]);
    assert_eq!(first["choices"][0]["index"], 0);
    assert_eq!(second["choices"][0]["index"], 1);
    assert_eq!(blocks[2], gaps);
}

#[test]
fn duplicate_members_forward_raw() {
    let (map, tokens) = map_for(&["never", "opaque-token"]);
    let mut harness = Harness::new(map.clone());
    for payload in [
        "{\"choices\":[{\"index\":0,\"index\":1,\"delta\":{}}]}",
        "{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"a\",\"arguments\":\"b\"}}]}}]}",
        "{\"choices\":[{\"\\u0069ndex\":0,\"index\":1,\"delta\":{}}]}",
    ] {
        let line = format!("data: {payload}");
        assert_eq!(harness.rehydrator.rehydrate_event(&line), line); assert!(has_ambiguous_chat_tool_members(payload));
    }
    assert!(!harness.rehydrator.chat_arguments.as_ref().unwrap().admissions_open);
    let mut harness = Harness::new(map.clone());
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{{\"v\":\"{}", &tokens[1][..40]))],
        false,
    ));
    let raw = "data: {\"choices\":[{\"index\":0,\"index\":1,\"delta\":{}}]}";
    let out = harness.rehydrator.rehydrate_event(raw);
    let (prelude, body) = out.split_once("\n\n").expect("held prefix flushes first");
    assert!(prelude.contains(&tokens[1][..40]));
    assert_eq!(body, raw);
    let arguments = serde_json::to_string(&format!("{{\"v\":\"{}\"}}", tokens[1])).unwrap();
    let malformed = format!("data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{arguments}}}}},\"bad\"]}}}}]}}");
    let mut harness = Harness::new(map);
    assert_eq!(harness.rehydrator.rehydrate_event(&malformed), malformed);
    assert_eq!(harness.report().unrestored, 1);
    assert_eq!(harness.report().restored, 0);
}

#[test]
fn syntax_and_depth_faults_pass_through() {
    let (map, tokens) = map_for(&["fault-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map.clone());
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{{\"v\":\"{}", &token[..40]))],
        false,
    ));
    let out = harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{}\\q", &token[40..]))],
        false,
    ));
    assert_eq!(arg_for(&parse_events(&out)[0], 0, 0), Some(&*format!("{token}\\q")));
    assert_eq!(harness.report().restored, 0);
    assert_eq!(harness.report().unrestored, 1);
    let (map, tokens) = map_for(&["deep-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map.clone());
    let deep = format!("{}\"{}\"{}", "[".repeat(32), token, "]".repeat(32));
    let (text, report) = feed_all(&map, &[event(0, &[(0, &deep)], false)], 0, 0);
    assert_eq!(text, format!("{}\"{}\"{}", "[".repeat(32), "deep-token", "]".repeat(32)));
    assert_eq!(report.restored, 1);
    let line = event(0, &[(0, &"[".repeat(33))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&line), line);
    let later = event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&later), later);
    assert_eq!(harness.report().restored, 0);
    let (map, tokens) = map_for(&["second-dot"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map);
    let malformed = event(0, &[(0, &format!("[1.2.3,\"{token}\"]"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&malformed), malformed);
    assert_eq!(harness.report().restored, 0);
    let later = event(0, &[(0, &format!("[\"{token}\"]"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&later), later);
    let marker = format!("[1.2.3,\"__PKV_{}__\"]", "f".repeat(64));
    let marker = event(0, &[(0, &marker)], false);
    let mut harness = Harness::new(map_for(&["second-dot-marker"]).0);
    assert_eq!(harness.rehydrator.rehydrate_event(&marker), marker);
    assert_eq!(harness.report().unrestored, 1);
}

#[test]
fn unchanged_event_reuses_original_bytes() {
    let (map, _) = map_for(&["unused-token"]);
    let arguments =
        serde_json::to_string("{\"city\":\"Paris\",\"note\":\"a\\\"b\\\\c\\u0041\"}").unwrap();
    let line = format!("data: {{ \"model\" : \"m\" , \"id\" : \"chatcmpl-1\" , \"created\" : 1 , \"object\" : \"chat.completion.chunk\" , \"choices\" : [ {{ \"finish_reason\" : null , \"index\" : 0 , \"delta\" : {{ \"tool_calls\" : [ {{ \"type\" : \"function\" , \"id\" : \"call_1\" , \"index\" : 0 , \"function\" : {{ \"name\" : \"f\" , \"arguments\" : {arguments} }} }} ] }} }} ] }}");
    let mut harness = Harness::new(map);
    assert_eq!(harness.rehydrator.rehydrate_event(&line), line);
    assert_eq!(harness.report().restored, 0);
    assert_eq!(harness.report().unrestored, 0);
}

#[test]
fn preserves_chat_block_separators_and_control_lines() {
    let (map, tokens) = map_for(&["control-token", "changed-token"]);
    let payload = "{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}]}}]}";
    for block in [
        format!("id: {}\r\ndata: {payload}\r\n", tokens[0]),
        format!("id: {}\rdata: {payload}\r", tokens[0]),
    ] {
        assert_eq!(Harness::new(map.clone()).rehydrator.rehydrate_event(&block), block);
    }
    let changed = event(0, &[(0, &format!("{{\"v\":\"{}\"}}", tokens[1]))], false);
    let out = Harness::new(map).rehydrator.rehydrate_event(&format!("id: x\r\n{changed}\r\n"));
    assert!(out.starts_with("id: x\r\ndata: "));
    assert!(out.ends_with("\r\n"));
    assert!(out.contains("changed-token"));
}

#[test]
fn has_ambiguous_members_checks_relevant_paths() {
    assert!(!crate::rehydrate::has_ambiguous_chat_tool_members("{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"x\"}}]}}]}"));
    assert!(has_ambiguous_chat_tool_members(
        "{\"choices\":[{\"index\":0,\"index\":1,\"delta\":{}}]}"
    ));
    assert!(has_ambiguous_chat_tool_members("{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"x\",\"arguments\":\"y\"}}]}}]}"));
    assert!(has_ambiguous_chat_tool_members(
        "{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[],\"tool_calls\":[]}}]}"
    ));
    assert!(!has_ambiguous_chat_tool_members(
        "{\"meta\":{\"index\":0,\"index\":1},\"choices\":[{\"index\":0,\"delta\":{}}]}"
    ));
    assert!(has_ambiguous_chat_tool_members("not json"));
}

#[test]
fn effective_event_type_uses_last_event_field() {
    let (map, tokens) = map_for(&["message-token"]);
    let token = &tokens[0];
    let data = event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
    let message = format!("event: error\nevent: message\n{data}");
    let mut harness = Harness::new(map.clone());
    let out = harness.rehydrator.rehydrate_event(&message);
    let line = out.lines().find(|line| line.starts_with("data: ")).expect("data line");
    let parsed: Value = serde_json::from_str(line.strip_prefix("data: ").unwrap()).unwrap();
    let args = arg_for(&parsed, 0, 0).unwrap();
    assert_eq!(serde_json::from_str::<Value>(args).unwrap()["v"], "message-token");
    assert_eq!(harness.report().restored, 1);
    let error = format!("event: message\nevent: error\n{data}");
    let mut harness = Harness::new(map);
    assert_eq!(harness.rehydrator.rehydrate_event(&error), error);
    assert_eq!(harness.report().restored, 0);
}

#[test]
fn event_value_removes_only_one_leading_space() {
    let (map, tokens) = map_for(&["space-token"]);
    let token = &tokens[0];
    for prefix in ["event:  error\n", "event: error \n"] {
        let mut harness = Harness::new(map.clone());
        let first = harness.rehydrator.rehydrate_event(&event(
            0,
            &[(0, &format!("{{\"v\":\"{}", &token[..40]))],
            false,
        ));
        let complete = event(0, &[(0, &format!("{}\"}}", &token[40..]))], false);
        let out = harness.rehydrator.rehydrate_event(&format!("{prefix}{complete}"));
        let line = out.lines().find(|line| line.starts_with("data: ")).expect("data line");
        let parsed: Value = serde_json::from_str(line.strip_prefix("data: ").unwrap()).unwrap();
        let mut events = parse_events(&first);
        events.push(parsed);
        assert_eq!(
            serde_json::from_str::<Value>(&assembled(&events, 0, 0)).unwrap()["v"],
            "space-token"
        );
        assert_eq!(harness.report().restored, 1);
        assert!(harness.rehydrator.chat_arguments.as_ref().unwrap().admissions_open);
    }
}

#[test]
fn scans_many_comment_lines_without_per_line_descriptors() {
    let (map, tokens) = map_for(&["scan-token"]);
    let token = &tokens[0];
    let frame = |data: &str| {
        let mut frame = String::new();
        for _ in 0..20_000 {
            frame.push_str(": comment\n");
        }
        frame.push_str(data);
        frame.push('\n');
        for _ in 0..20_000 {
            frame.push_str(": comment\n");
        }
        frame
    };
    let plain = frame(&event(0, &[(0, "{\"v\":\"plain\"}")], false));
    assert!(plain.len() < 1 << 20);
    let mut harness = Harness::new(map.clone());
    assert_eq!(harness.rehydrator.rehydrate_event(&plain), plain);
    assert_eq!(harness.report().restored, 0);
    let restored = frame(&event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false));
    assert!(restored.len() < 1 << 20);
    let mut harness = Harness::new(map);
    let out = harness.rehydrator.rehydrate_event(&restored);
    assert_ne!(out, restored);
    let line = out.lines().find(|line| line.starts_with("data: ")).expect("data line");
    let parsed: Value = serde_json::from_str(line.strip_prefix("data: ").unwrap()).unwrap();
    let args = arg_for(&parsed, 0, 0).unwrap();
    assert_eq!(serde_json::from_str::<Value>(args).unwrap()["v"], "scan-token");
    assert_eq!(harness.report().restored, 1);
}
