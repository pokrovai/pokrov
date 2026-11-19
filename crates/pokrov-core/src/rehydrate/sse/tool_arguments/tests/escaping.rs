//! Escaping of restored fragments and marker counting across emission
//! boundaries.

use super::*;

#[test]
fn escapes_original_and_preserves_split_inner_escapes() {
    let fragment = "acme\"corp\\path\nline\rend\ttab\u{0}nul \u{fc}n\u{ef}code";
    let (map, tokens) = map_for(&[fragment]);
    let (text, report) =
        feed_all(&map, &[event(0, &[(0, &format!("{{\"v\":\"{}\"}}", tokens[0]))], false)], 0, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], fragment);
    assert_eq!(report.restored, 1);
    let (map, _) = map_for(&["unused"]);
    let inner = "{\"v\":\"a\\\"b\\\\c\\u0041d\"}";
    let events: Vec<String> = inner
        .bytes()
        .map(|b| event(0, &[(0, std::str::from_utf8(&[b]).unwrap())], false))
        .collect();
    let (text, _) = feed_all(&map, &events, 0, 0);
    assert_eq!(text, inner);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], "a\"b\\cAd");
    assert!(text.contains("\\u0041"));
    let (marker_map, marker_tokens) = map_for(&["marker-victim"]);
    let marker = &marker_tokens[0];
    let inner = format!("{{\"v\":\"_\\u005f{}\"}}", &marker[2..]);
    let (text, report) = feed_all(&marker_map, &[event(0, &[(0, &inner)], false)], 0, 0);
    assert_eq!(text, inner);
    assert_eq!(report.restored, 0);
    assert_eq!(report.unrestored, 0);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["v"], marker.as_str());
}

#[test]
fn counts_marker_split_across_emission_and_release() {
    let (map, _) = map_for(&["split-counter"]);
    let mut harness = Harness::new(map);
    harness.rehydrator.rehydrate_event(&event(0, &[(0, "{\"v\":\"__PKV__")], false));
    assert_eq!(harness.report().unrestored, 0);
    let released = parse_events(&harness.rehydrator.finish().join("\n\n"));
    assert_eq!(harness.report().unrestored, 1);
    assert_eq!(arg_for(&released[0], 0, 0), Some("__"));
    let (map, _) = map_for(&["boundary-counter"]);
    let mut harness = Harness::new(map);
    let boundary = event(0, &[(0, r#"["__PKV","_"]"#)], false);
    assert_eq!(harness.rehydrator.rehydrate_event(&boundary), boundary);
    assert_eq!(harness.report().unrestored, 0);
}
