//! Reversible tokenization: deterministic keyed pseudonyms with per-request
//! restore maps. Fragments marked by the `[PKV_TOKEN]` replacement template are
//! substituted with identifier-safe tokens before upstream forwarding and
//! restored verbatim on the response path back to the client.

use std::{collections::BTreeMap, fmt, ops::Bound};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{traversal::map_string_leaves, types::EvaluateResult};

/// Replacement template sentinel marking a `replace` rule as reversible.
/// Hits on such rules emit deterministic tokens and record the original
/// fragment in the request-scoped `RehydrationMap`.
pub const REVERSIBLE_TEMPLATE: &str = "[PKV_TOKEN]";

const TOKEN_PREFIX: &str = "__PKV_";
const TOKEN_SUFFIX: &str = "__";
const TOKEN_HEX_LEN: usize = 32;

/// `__PKV_` plus the hex body; narrows `BTreeMap` lookups to the collision
/// domain of one base token instead of scanning every known token.
const TOKEN_BASE_LEN: usize = TOKEN_PREFIX.len() + TOKEN_HEX_LEN;

mod sse;

pub use sse::{event_boundary_end, EventRehydrator, SseStreamRehydrator};

/// Derives deterministic pseudonym tokens from secret key material.
/// Keyed HMAC-SHA256 keeps tokens stable across requests and proxy paths
/// (LLM and MCP) without any persistent vault; dictionary recovery of a
/// fragment from its token requires the key.
#[derive(Clone)]
pub struct TokenDeriver {
    key: Vec<u8>,
}

impl TokenDeriver {
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    /// HMAC-SHA256 digest of the fragment under the derivation key — the
    /// single source of every token byte, base and collision suffix alike.
    fn digest_for(&self, fragment: &str) -> [u8; 32] {
        hmac_sha256(&self.key, fragment.as_bytes())
    }
}

/// Best-effort scrub of derivation key material on teardown.
impl Drop for TokenDeriver {
    fn drop(&mut self) {
        self.key.fill(0);
    }
}

impl fmt::Debug for TokenDeriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenDeriver(***)")
    }
}

/// Per-request token ↔ original map. Deliberately not serializable: original
/// fragments must never reach audit, logs, or API responses.
#[derive(Clone, Default)]
pub struct RehydrationMap {
    token_to_fragment: BTreeMap<String, String>,
    fragment_to_token: BTreeMap<String, String>,
    spans_total: u32,
}

impl RehydrationMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.token_to_fragment.is_empty()
    }

    /// Total resolved spans that were tokenized (same fragment may repeat).
    pub fn spans_total(&self) -> u32 {
        self.spans_total
    }

    /// Returns the deterministic token for a fragment: `__PKV_<32 lowercase
    /// hex>__` taken from the fragment's keyed digest. The 128-bit body makes
    /// collisions cryptographically unreachable, so the fragment→token mapping
    /// is a pure function of (key, fragment) — identical across requests and
    /// independent of insertion order, as FR-002 requires.
    ///
    /// For completeness the map still disambiguates a body collision with a
    /// `_c<8hex>` suffix drawn from the same digest (bytes [16..20], then
    /// [20..24], and so on); the suffix sequence is deterministic per
    /// fragment rather than sequential per request. A residual
    /// winner-take-base corner would need a real 128-bit collision inside a
    /// single request and is unreachable without the key. If the digest
    /// runway is ever exhausted, further material is derived by chained HMAC.
    pub fn token_for(&mut self, deriver: &TokenDeriver, fragment: &str) -> String {
        self.spans_total += 1;
        if let Some(token) = self.fragment_to_token.get(fragment) {
            return token.clone();
        }

        let digest = deriver.digest_for(fragment);
        let base = format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&digest[..16]));

        let mut token = base.clone();
        let mut level = 0u32;
        while self.token_to_fragment.contains_key(&token) {
            level += 1;
            let suffix = collision_suffix(deriver, fragment, &digest, level);
            token =
                format!("{}_c{}{}", &base[..base.len() - TOKEN_SUFFIX.len()], suffix, TOKEN_SUFFIX);
        }

        self.token_to_fragment.insert(token.clone(), fragment.to_string());
        self.fragment_to_token.insert(fragment.to_string(), token.clone());
        token
    }

    /// Longest known token matching the head of `text`, or `None`.
    /// Collision-suffixed tokens share the base prefix, so longest wins.
    /// The 38-byte `__PKV_<hex32>` base narrows the BTreeMap range scan to
    /// the collision domain, keeping lookups O(log n + collisions).
    fn longest_match_at(&self, text: &[u8]) -> Option<(&str, &str)> {
        if text.len() < TOKEN_BASE_LEN {
            return None;
        }
        // Tokens are pure ASCII; non-UTF-8 here cannot match any key prefix.
        let base_prefix = std::str::from_utf8(&text[..TOKEN_BASE_LEN]).ok()?;
        self.token_to_fragment
            .range::<str, _>((Bound::Included(base_prefix), Bound::Unbounded))
            .take_while(|(token, _)| token.starts_with(base_prefix))
            .filter(|(token, _)| text.starts_with(token.as_bytes()))
            .max_by_key(|(token, _)| token.len())
            .map(|(token, original)| (token.as_str(), original.as_str()))
    }
}

impl fmt::Debug for RehydrationMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RehydrationMap")
            .field("tokens", &self.token_to_fragment.len())
            .field("spans_total", &self.spans_total)
            .finish()
    }
}

/// Evaluation result paired with the request-scoped rehydration map.
/// Returned by `SanitizationEngine::evaluate_with_rehydration`.
pub struct EvaluateOutcome {
    pub result: EvaluateResult,
    pub rehydration: RehydrationMap,
}

/// Borrowed context threaded through the transform stage so `replace` rules
/// carrying the reversible sentinel can allocate tokens and record originals.
pub struct RehydrationContext<'a> {
    pub deriver: &'a TokenDeriver,
    pub map: &'a mut RehydrationMap,
}

/// Restore-pass counters. `restored` counts map hits substituted back;
/// `unrestored` counts `__PKV_` marker occurrences that matched no known
/// token — model-mutated or foreign tokens left visible by design.
#[derive(Clone, Copy, Debug, Default)]
pub struct RehydrateReport {
    pub restored: u32,
    pub unrestored: u32,
}

/// Restores every exact token occurrence in `text`. The input is scanned in a
/// single pass and restored fragments are never re-scanned, so originals that
/// themselves contain token-looking text cannot corrupt the output.
pub fn rehydrate_text(text: &str, map: &RehydrationMap) -> (String, RehydrateReport) {
    if map.is_empty() {
        return (text.to_string(), RehydrateReport::default());
    }
    let (bytes, report) = rehydrate_bytes(text.as_bytes(), map);
    // Tokens are pure ASCII; replacing them with UTF-8 fragments preserves
    // validity of the surrounding text.
    let restored_text = String::from_utf8(bytes)
        .expect("rehydration replaces ASCII tokens within valid UTF-8 input");
    (restored_text, report)
}

/// Restores token occurrences across all JSON string leaves, preserving
/// structure. Takes the value by ownership so empty maps cost no clone on
/// the hot response path.
pub fn rehydrate_value(value: Value, map: &RehydrationMap) -> (Value, RehydrateReport) {
    if map.is_empty() {
        return (value, RehydrateReport::default());
    }
    let mut report = RehydrateReport::default();
    let (mapped, _) = map_string_leaves(&value, &mut |_pointer, text| {
        let (out, leaf) = rehydrate_text(text, map);
        report.restored = report.restored.saturating_add(leaf.restored);
        report.unrestored = report.unrestored.saturating_add(leaf.unrestored);
        out
    });
    (mapped, report)
}

/// Earliest index at which the pending tail is a proper prefix of a known
/// token; bytes before it can never complete a token and may be emitted.
/// Scans `__PKV_` occurrences left to right: complete token matches advance
/// the resolved boundary so a token's own `__` terminator is not mistaken
/// for the start of the next token.
fn holdback_index(pending: &[u8], map: &RehydrationMap) -> usize {
    let len = pending.len();
    let mut resolved_end = 0usize;
    let mut scan = 0usize;
    while scan < len {
        let Some(offset) = find_subslice(&pending[scan..], TOKEN_PREFIX.as_bytes()) else {
            break;
        };
        let pos = scan + offset;
        if let Some((token, _)) = map.longest_match_at(&pending[pos..]) {
            resolved_end = pos + token.len();
            scan = resolved_end;
            continue;
        }
        if is_proper_token_prefix(&pending[pos..], map) {
            return pos;
        }
        // Literal marker text: resume one byte later so an overlapping
        // `__PKV_` occurrence is still discovered.
        scan = pos + 1;
    }

    // Tails shorter than `__PKV_` cannot be found by the scan above but may
    // still grow into a token (e.g. a chunk ending in `__`). They are only
    // considered beyond the last resolved token boundary.
    let tail_start = len.saturating_sub(TOKEN_PREFIX.len() - 1).max(resolved_end);
    for idx in tail_start..len {
        if is_proper_token_prefix(&pending[idx..], map) {
            return idx;
        }
    }
    len
}

/// Whether `tail` is a non-empty strict prefix of at least one known
/// token. Keys starting with `tail` sort adjacently, so the first
/// `range` entry decides — no full-map scan per holdback check.
fn is_proper_token_prefix(tail: &[u8], map: &RehydrationMap) -> bool {
    if tail.is_empty() {
        return false;
    }
    let Ok(tail) = std::str::from_utf8(tail) else {
        return false;
    };
    map.token_to_fragment
        .range::<str, _>((Bound::Included(tail), Bound::Unbounded))
        .next()
        .is_some_and(|(token, _)| token.len() > tail.len() && token.starts_with(tail))
}

fn rehydrate_bytes(data: &[u8], map: &RehydrationMap) -> (Vec<u8>, RehydrateReport) {
    let mut out = Vec::with_capacity(data.len());
    let mut report = RehydrateReport::default();
    let mut rest = data;

    while let Some(pos) = find_subslice(rest, TOKEN_PREFIX.as_bytes()) {
        out.extend_from_slice(&rest[..pos]);
        let candidate = &rest[pos..];
        match map.longest_match_at(candidate) {
            Some((token, original)) => {
                out.extend_from_slice(original.as_bytes());
                report.restored += 1;
                rest = &candidate[token.len()..];
            }
            None => {
                // Any `__PKV_` marker surviving exact matching is an anomaly
                // worth counting: mutated tokens stay visible to the client.
                report.unrestored += 1;
                out.extend_from_slice(TOKEN_PREFIX.as_bytes());
                rest = &candidate[TOKEN_PREFIX.len()..];
            }
        }
    }

    out.extend_from_slice(rest);
    (out, report)
}

/// Counts `__PKV_` marker occurrences in bytes that pass through unrestored
/// (verbatim overflow or error-path flushes). Every marker — known token or
/// foreign — stays visible and counts as unrestored.
pub(crate) fn count_unrestored_markers(data: &[u8], map: &RehydrationMap) -> u32 {
    let mut count = 0u32;
    let mut rest = data;
    while let Some(pos) = find_subslice(rest, TOKEN_PREFIX.as_bytes()) {
        count = count.saturating_add(1);
        let candidate = &rest[pos..];
        match map.longest_match_at(candidate) {
            Some((token, _)) => rest = &candidate[token.len()..],
            None => rest = &candidate[TOKEN_PREFIX.len()..],
        }
    }
    count
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn hex_encode(bytes: &[u8]) -> String {
    use fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Deterministic `_c<8hex>` suffix material for collision `level` (1-based):
/// digest bytes [16..20] for level 1, then [20..24], and so on through the
/// four-group runway; beyond that the chain is extended by chained HMAC over
/// `fragment#pkv-collision-<level>`. Because every suffix derives from the
/// fragment's own keyed digest, the escalation sequence never depends on
/// request content or mint ordering.
fn collision_suffix(
    deriver: &TokenDeriver,
    fragment: &str,
    digest: &[u8; 32],
    level: u32,
) -> String {
    // The base token consumes digest[..16]; the runway offers four 4-byte
    // groups before chained HMAC material takes over.
    const RUNWAY_LEVELS: u32 = 4;
    if level <= RUNWAY_LEVELS {
        let start = 16 + 4 * (level as usize - 1);
        return hex_encode(&digest[start..start + 4]);
    }
    let extended =
        hmac_sha256(&deriver.key, format!("{fragment}#pkv-collision-{level}").as_bytes());
    hex_encode(&extended[..4])
}

/// HMAC-SHA256 per RFC 2104 over the workspace `sha2` dependency; kept local
/// to avoid pulling an extra crypto crate for a single construction.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_LEN: usize = 64;

    let mut block_key = [0u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        let hashed = Sha256::digest(key);
        block_key[..hashed.len()].copy_from_slice(&hashed);
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_LEN];
    let mut opad = [0x5cu8; BLOCK_LEN];
    for idx in 0..BLOCK_LEN {
        ipad[idx] ^= block_key[idx];
        opad[idx] ^= block_key[idx];
    }

    let inner = Sha256::new().chain_update(ipad).chain_update(message).finalize();
    Sha256::new().chain_update(opad).chain_update(inner).finalize().into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        hex_encode, rehydrate_text, rehydrate_value, RehydrationMap, SseStreamRehydrator,
        TokenDeriver, TOKEN_BASE_LEN, TOKEN_PREFIX, TOKEN_SUFFIX,
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
        let digest = deriver.digest_for(fragment);
        format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&digest[..16]))
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
    fn token_format_is_a_32_hex_digit_identifier_fragment() {
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
        let digest = deriver.digest_for(fragment);
        let base = format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&digest[..16]));
        let suffix_l1 = hex_encode(&digest[16..20]);
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
}
