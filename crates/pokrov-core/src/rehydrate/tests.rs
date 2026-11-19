use serde_json::json;

use super::{
    hex_encode, rehydrate_text, rehydrate_value, RehydrationMap, SseStreamRehydrator, TokenDeriver,
    TOKEN_BASE_LEN, TOKEN_PREFIX, TOKEN_SUFFIX,
};

fn deriver() -> TokenDeriver {
    TokenDeriver::new(b"test-rehydration-key")
}

fn map_with(deriver: &TokenDeriver, fragments: &[&str]) -> RehydrationMap {
    let mut map = RehydrationMap::new();
    for fragment in fragments {
        map.token_for(deriver, fragment);
    }
    map
}

fn base_of(deriver: &TokenDeriver, fragment: &str) -> String {
    format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&deriver.digest_for(fragment)))
}

#[test]
fn token_is_deterministic_and_identifier_safe() {
    let deriver = deriver();
    let first = base_of(&deriver, "acme-corp");
    let second = base_of(&deriver, "acme-corp");

    assert_eq!(first, second);
    assert!(first.starts_with("__PKV_"));
    assert!(first.ends_with("__"));
    assert!(first.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'));
}

#[test]
fn different_keys_yield_different_tokens() {
    let left = base_of(&TokenDeriver::new(b"key-one"), "acme-corp");
    let right = base_of(&TokenDeriver::new(b"key-two"), "acme-corp");
    assert_ne!(left, right);
}

#[test]
fn same_fragment_reuses_token_within_map() {
    let deriver = deriver();
    let mut map = RehydrationMap::new();
    let first = map.token_for(&deriver, "acme-corp");
    let second = map.token_for(&deriver, "acme-corp");
    assert_eq!(first, second);
    assert_eq!(map.spans_total(), 2);
}

#[test]
fn rehydrate_text_restores_tokens_and_counts_occurrences() {
    let deriver = deriver();
    let map = map_with(&deriver, &["acme-corp"]);
    let token = map.token_to_fragment.keys().next().expect("token exists").clone();

    let text = format!("use {token}-utils::init; // also {token}");
    let (restored, report) = rehydrate_text(&text, &map);

    assert_eq!(restored, "use acme-corp-utils::init; // also acme-corp");
    assert_eq!(report.restored, 2);
    assert_eq!(report.unrestored, 0);
}

#[test]
fn rehydrate_text_never_rescans_restored_fragments() {
    let deriver = deriver();
    let mut map = RehydrationMap::new();
    let trap = map.token_for(&deriver, "innocent");
    let nested = map.token_for(&deriver, &trap);

    let (restored, report) = rehydrate_text(&format!("{nested} end"), &map);

    // The restored value is itself a known token; single-pass scanning
    // must not substitute it again.
    assert_eq!(restored, format!("{trap} end"));
    assert_eq!(report.restored, 1);
}

#[test]
fn rehydrate_text_leaves_unknown_tokens_untouched() {
    let deriver = deriver();
    let map = map_with(&deriver, &["acme-corp"]);
    let foreign = "literal __PKV_ffffffffffffffffffffffff__ here";
    let (restored, report) = rehydrate_text(foreign, &map);
    assert_eq!(restored, foreign);
    assert_eq!(report.restored, 0);
    // Well-formed but foreign token markers are counted as unrestored:
    // the LLM-mutated/unknown marker remains visible to the client and
    // the counter feeds audit/metrics observability.
    assert_eq!(report.unrestored, 1);
}

#[test]
fn token_format_is_a_64_hex_digit_identifier_fragment() {
    let deriver = deriver();
    let mut map = RehydrationMap::new();
    let token = map.token_for(&deriver, "acme-corp");
    assert!(token.starts_with("__PKV_") && token.ends_with("__"));
    assert_eq!(token.len(), TOKEN_BASE_LEN + TOKEN_SUFFIX.len());
    assert!(token[TOKEN_PREFIX.len()..token.len() - TOKEN_SUFFIX.len()]
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
}

#[test]
fn collision_suffix_assignment_is_deterministic_across_insertion_order() {
    let deriver = deriver();
    let fragment = "collision-victim";

    // The base slot a fragment maps to is a pure function of its digest.
    let base = format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&deriver.digest_for(fragment)));
    let suffix_l1 = super::collision_suffix(&deriver, fragment, 1);
    let suffixed =
        format!("{}_c{suffix_l1}{TOKEN_SUFFIX}", &base[..base.len() - TOKEN_SUFFIX.len()]);

    // Map A: base slot occupied first → the victim escalates to _c<hex>.
    let mut map_a = RehydrationMap::new();
    map_a.token_to_fragment.insert(base.clone(), "occupant".to_string());
    map_a.fragment_to_token.insert("occupant".to_string(), base.clone());
    let token_a = map_a.token_for(&deriver, fragment);
    assert_eq!(token_a, suffixed);

    // Map B built in a different order must land on the identical suffix:
    // the escalation material derives from the fragment's digest, not from
    // a per-request counter.
    let mut map_b = RehydrationMap::new();
    map_b.token_to_fragment.insert(base.clone(), "occupant".to_string());
    map_b.fragment_to_token.insert("occupant".to_string(), base.clone());
    let token_b = map_b.token_for(&deriver, fragment);
    assert_eq!(token_a, token_b);

    // Base tokens restore to the occupant, suffixed tokens to the victim —
    // ambiguity is impossible within one map.
    let (restored, _) = rehydrate_text(&format!("{token_a} {base}"), &map_a);
    assert_eq!(restored, "collision-victim occupant");
}

#[test]
fn token_minting_is_order_independent_for_unrelated_fragments() {
    // FR-002: a fragment must receive the same token no matter which other
    // fragments were minted before it in the request.
    let deriver = deriver();
    let mut forward = RehydrationMap::new();
    forward.token_for(&deriver, "alpha");
    forward.token_for(&deriver, "beta");
    let target_first_order = forward.token_for(&deriver, "target-fragment");

    let mut reverse = RehydrationMap::new();
    let target_last_order = reverse.token_for(&deriver, "target-fragment");
    reverse.token_for(&deriver, "beta");
    reverse.token_for(&deriver, "alpha");

    assert_eq!(target_first_order, target_last_order);
}

#[test]
fn rehydrate_value_restores_only_string_leaves() {
    let deriver = deriver();
    let map = map_with(&deriver, &["acme-corp"]);
    let token = map.token_to_fragment.keys().next().expect("token exists").clone();
    let payload = json!({
        "a": token,
        "nested": {"b": 3, "c": ["ok", format!("x{token}y")]},
    });

    let (restored, report) = rehydrate_value(payload, &map);

    assert_eq!(restored["a"], "acme-corp");
    assert_eq!(restored["nested"]["b"], 3);
    assert_eq!(restored["nested"]["c"][1], "xacme-corpy");
    assert_eq!(report.restored, 2);
}

#[test]
fn sse_stream_rehydrator_restores_token_split_across_chunks() {
    let deriver = deriver();
    let map = map_with(&deriver, &["acme-corp"]);
    let token = map.token_to_fragment.keys().next().expect("token exists").clone();
    let split = token.len() / 2;

    let mut rehydrator = SseStreamRehydrator::new(map);
    let mut emitted = Vec::new();
    emitted.extend(rehydrator.feed(b"data: \"use "));
    emitted.extend(rehydrator.feed(&token.as_bytes()[..split]));
    emitted.extend(rehydrator.feed(&token.as_bytes()[split..]));
    // The event is not terminated yet: nothing must be emitted early.
    assert!(emitted.is_empty());
    emitted.extend(rehydrator.feed(b"::init\"\n\n"));
    emitted.extend(rehydrator.finish());

    let text = String::from_utf8(emitted).expect("utf8 output");
    assert_eq!(text, "data: \"use acme-corp::init\"\n\n");
}

#[test]
fn sse_stream_rehydrator_flushes_unterminated_prefix_verbatim() {
    let deriver = deriver();
    let map = map_with(&deriver, &["acme-corp"]);
    let mut rehydrator = SseStreamRehydrator::new(map);
    // No event terminator: bytes stay buffered and flush verbatim at end.
    assert!(rehydrator.feed(b"tail __PKV_fff").is_empty());
    let emitted = rehydrator.finish();
    assert_eq!(emitted, b"tail __PKV_fff".to_vec());
    assert_eq!(rehydrator.report().unrestored, 1);
}

#[test]
fn sse_stream_rehydrator_empty_map_passthrough() {
    let mut rehydrator = SseStreamRehydrator::new(RehydrationMap::new());
    assert_eq!(rehydrator.feed(b"abc\n\n"), b"abc\n\n".to_vec());
    assert!(rehydrator.finish().is_empty());
}

#[test]
fn map_debug_output_excludes_fragments() {
    let deriver = deriver();
    let map = map_with(&deriver, &["acme-corp"]);
    let rendered = format!("{map:?}");
    assert!(!rendered.contains("acme-corp"));
    assert!(rendered.contains("tokens: 1"));
}

#[test]
fn hmac_sha256_matches_rfc4231_vectors() {
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // RFC 4231 Test Case 1: key shorter than the block size.
    assert_eq!(
        hex(&super::hmac_sha256(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    // RFC 4231 Test Case 2: key from a plain string.
    assert_eq!(
        hex(&super::hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    // RFC 4231 Test Case 6: key longer than the block size (hashed first).
    assert_eq!(
        hex(&super::hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn rehydrate_text_scales_on_representative_payload() {
    // Representative payload: ~1 MiB with a few hundred distinct
    // fragments; per-occurrence lookups must stay near-logarithmic.
    let deriver = deriver();
    let mut map = RehydrationMap::new();
    let fragments: Vec<String> = (0..500).map(|idx| format!("fragment-{idx:04}")).collect();
    let tokens: Vec<String> = fragments.iter().map(|f| map.token_for(&deriver, f)).collect();

    let mut body = String::with_capacity(1 << 20);
    while body.len() < (1 << 20) {
        for token in &tokens {
            body.push_str(token);
            body.push(' ');
        }
    }

    let started = std::time::Instant::now();
    let (_, report) = rehydrate_text(&body, &map);
    let elapsed = started.elapsed();

    assert!(report.restored > 10_000);
    // Generous bound for debug builds; real budget is P95 <= 50 ms total.
    assert!(elapsed < std::time::Duration::from_secs(2), "rehydration too slow: {elapsed:?}");
}
