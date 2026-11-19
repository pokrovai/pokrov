//! Token restoration across whole, split, byte-wise, and nested arguments,
//! including final-delta ordering before choice finish.

use super::*;

#[test]
fn restores_whole_and_every_argument_split() {
    let (map, tokens) = map_for(&["acme-corp"]);
    let token = &tokens[0];
    assert_eq!(token.len(), 72);
    assert!(token.len() <= MAX_TOKEN_BYTES);
    let whole = format!("{{\"name\":\"{token}-utils\",\"count\":3}}");
    let (text, report) = feed_all(&map, &[event(0, &[(0, &whole)], false)], 0, 0);
    assert_eq!(text, "{\"name\":\"acme-corp-utils\",\"count\":3}");
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["name"], "acme-corp-utils");
    assert_eq!(report.restored, 1);
    for split in 1..token.len() {
        let left = format!("{{\"name\":\"{}", &token[..split]);
        let right = format!("{}-utils\",\"count\":3}}", &token[split..]);
        let (text, report) = feed_all(
            &map,
            &[event(0, &[(0, &left)], false), event(0, &[(0, &right)], false)],
            0,
            0,
        );
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap()["name"],
            "acme-corp-utils",
            "split {split}"
        );
        assert_eq!(report.restored, 1);
    }
    let mut byte_events = vec![event(0, &[(0, "{\"v\":\"")], false)];
    byte_events
        .extend(token.bytes().map(|b| event(0, &[(0, std::str::from_utf8(&[b]).unwrap())], false)));
    byte_events.push(event(0, &[(0, "\"}")], false));
    let (text, report) = feed_all(&map, &byte_events, 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "acme-corp");
    assert_eq!(report.restored, 1);
    let adjacent = format!("{{\"v\":\"{token}{token}\"}}");
    let (text, report) = feed_all(&map, &[event(0, &[(0, &adjacent)], false)], 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "acme-corpacme-corp");
    assert_eq!(report.restored, 2);
    let (text, _) =
        feed_all(&map, &[event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false)], 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "acme-corp");
    let stray = format!("{{\"v\":\"__PKV_zz_{token}\"}}");
    let (text, report) = feed_all(&map, &[event(0, &[(0, &stray)], false)], 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "__PKV_zz_acme-corp");
    assert_eq!(report.restored, 1);
    assert_eq!(report.unrestored, 1);
    let (deriver, mut nested) = (deriver(), RehydrationMap::new());
    let inner = nested.token_for(&deriver, "innocent");
    let outer = nested.token_for(&deriver, &format!("pre{inner}post"));
    let (text, report) =
        feed_all(&nested, &[event(0, &[(0, &format!("{{\"v\":\"{outer}\"}}"))], false)], 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], format!("pre{inner}post"));
    assert_eq!(report.restored, 1);
    let (collision, token) = collision_map("collision-victim");
    for split in 1..token.len() {
        let first = event(0, &[(0, &format!("{{\"v\":\"{}", &token[..split]))], false);
        let second = event(0, &[(0, &format!("{}\"}}", &token[split..]))], false);
        let (text, report) = feed_all(&collision, &[first, second], 0, 0);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "collision-victim");
        assert_eq!(report.restored, 1);
    }
}

#[test]
fn processes_final_delta_before_choice_finish() {
    let (map, tokens) = map_for(&["final-token"]);
    let token = &tokens[0];
    let first = event(0, &[(0, &format!("{{\"v\":\"{}", &token[..40]))], false);
    let last = event(0, &[(0, &format!("{}\"}}", &token[40..]))], true);
    let (text, report) = feed_all(&map, &[first, last], 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "final-token");
    assert_eq!(report.restored, 1);
    let mut harness = Harness::new(map.clone());
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{{\"v\":\"{}", &token[..40]))],
        false,
    ));
    let out = harness.rehydrator.rehydrate_event(&finish_event(0));
    let (prelude, body) = out.split_once("\n\n").expect("suffix flushes first");
    let parsed: Value = serde_json::from_str(prelude.strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(parsed["object"], "chat.completion.chunk");
    assert_eq!(parsed["id"], "chatcmpl-test");
    assert_eq!(parsed["model"], "test-model");
    assert_eq!(parsed["created"], 1);
    assert!(parsed["choices"][0]["finish_reason"].is_null());
    assert!(parsed["choices"][0]["delta"]["tool_calls"][0].get("id").is_none());
    assert_eq!(body, finish_event(0));
    assert_eq!(harness.report().restored, 0);
    let mut harness = Harness::new(map.clone());
    for (choice, tool) in [(0, 0), (0, 1), (1, 0)] {
        harness.rehydrator.rehydrate_event(&event(
            choice,
            &[(tool, &format!("{{\"v\":\"{}", &token[..40]))],
            false,
        ));
    }
    let out = harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{}\"}}", &token[40..]))],
        true,
    ));
    let blocks: Vec<&str> = out.split("\n\n").collect();
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].contains(&token[..40]));
    assert!(!blocks[0].contains("\"stop\""));
    assert!(blocks[1].contains("final-token"));
    assert!(blocks[1].contains("finish_reason"));
    assert_eq!(harness.rehydrator.chat_arguments.as_ref().unwrap().streams.len(), 1);
    assert!(harness.rehydrator.rehydrate_event("data: [DONE]").contains(&token[..40]));
    assert!(harness.rehydrator.finish().is_empty());
    let (map, tokens) = map_for(&["done-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map.clone());
    harness.rehydrator.rehydrate_event(&event(
        0,
        &[(0, &format!("{{\"v\":\"{}", &token[..40]))],
        false,
    ));
    let out = harness.rehydrator.rehydrate_event("data: [DONE]");
    let (prelude, done) = out.split_once("\n\n").expect("suffix flushes before DONE");
    assert!(prelude.contains(&token[..40]));
    assert_eq!(done, "data: [DONE]");
    assert!(harness.rehydrator.finish().is_empty());
    let after = event(1, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&after), after);
    let (map, tokens) = map_for(&["late-token"]);
    let token = &tokens[0];
    let mut harness = Harness::new(map);
    harness.rehydrator.rehydrate_event(&event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], true));
    assert_eq!(harness.report().restored, 1);
    let late = event(0, &[(0, &format!("{{\"v\":\"{token}\"}}"))], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&late), late);
    assert_eq!(harness.report().restored, 1);
    let (map, tokens) = map_for(&["env-token"]);
    let token = &tokens[0];
    let args = serde_json::to_string(&format!("{{\"v\":\"{}", &token[..40])).unwrap();
    let oversized = format!("data: {{\"id\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{args}}}}}]}}}}]}}", "x".repeat(257));
    let mut harness = Harness::new(map.clone());
    assert_eq!(harness.rehydrator.rehydrate_event(&oversized), oversized);
    assert_eq!(harness.report().unrestored, 1);
    assert!(harness.rehydrator.finish().is_empty());
    let bare = format!("data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{args}}}}}]}}}}]}}");
    let mut harness = Harness::new(map);
    harness.rehydrator.rehydrate_event(&bare);
    let events = parse_events(&harness.rehydrator.finish().join("\n\n"));
    assert_eq!(events[0]["object"], "chat.completion.chunk");
    assert!(
        events[0].get("id").is_none()
            && events[0].get("model").is_none()
            && events[0].get("created").is_none()
    );
    let fractional = format!("data: {{\"created\":1.5,\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":{args}}}}}]}}}}]}}");
    let mut harness = Harness::new(map_for(&["env-token"]).0);
    harness.rehydrator.rehydrate_event(&fractional);
    assert_eq!(parse_events(&harness.rehydrator.finish().join("\n\n"))[0]["created"], 1.5);
}
