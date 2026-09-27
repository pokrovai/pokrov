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
const TOKEN_HEX_LEN: usize = 12;

/// `__PKV_` plus the hex body; narrows `BTreeMap` lookups to the collision
/// domain of one base token instead of scanning every known token.
const TOKEN_BASE_LEN: usize = TOKEN_PREFIX.len() + TOKEN_HEX_LEN;

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

    /// Base token for a fragment: `__PKV_<12 lowercase hex>__`, a valid
    /// identifier fragment in code contexts.
    fn base_token(&self, fragment: &str) -> String {
        let digest = hmac_sha256(&self.key, fragment.as_bytes());
        let mut token =
            String::with_capacity(TOKEN_PREFIX.len() + TOKEN_HEX_LEN + TOKEN_SUFFIX.len());
        token.push_str(TOKEN_PREFIX);
        for byte in &digest[..TOKEN_HEX_LEN / 2] {
            use fmt::Write;
            let _ = write!(token, "{byte:02x}");
        }
        token.push_str(TOKEN_SUFFIX);
        token
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

    /// Returns the deterministic token for a fragment. A 48-bit prefix
    /// collision with a different fragment is disambiguated by a `_c<N>`
    /// suffix assigned in deterministic insertion order.
    pub fn token_for(&mut self, deriver: &TokenDeriver, fragment: &str) -> String {
        self.spans_total += 1;
        if let Some(token) = self.fragment_to_token.get(fragment) {
            return token.clone();
        }

        let base = deriver.base_token(fragment);
        let mut token = base.clone();
        let mut collision = 0u32;
        while self.token_to_fragment.contains_key(&token) {
            collision += 1;
            token = format!("{}_c{}{}", &base[..base.len() - TOKEN_SUFFIX.len()], collision, TOKEN_SUFFIX);
        }

        self.token_to_fragment.insert(token.clone(), fragment.to_string());
        self.fragment_to_token.insert(fragment.to_string(), token.clone());
        token
    }

    /// Longest known token matching the head of `text`, or `None`.
    /// Collision-suffixed tokens share the base prefix, so longest wins.
    /// The 18-byte `__PKV_<hex>` base narrows the BTreeMap range scan to
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

/// Byte-level incremental restorer for streaming response bodies. Tokens may
/// be split across chunk boundaries, so the longest pending tail that is a
/// proper prefix of a known token is held back until it resolves or the
/// stream ends.
pub struct StreamRehydrator {
    map: RehydrationMap,
    pending: Vec<u8>,
    restored_total: u32,
    unrestored_total: u32,
}

impl StreamRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self { map, pending: Vec::new(), restored_total: 0, unrestored_total: 0 }
    }

    /// Feeds one chunk and returns the bytes safe to emit downstream.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        if self.map.is_empty() {
            return chunk.to_vec();
        }
        self.pending.extend_from_slice(chunk);
        let cut = self.holdback_index();
        let emit: Vec<u8> = self.pending.drain(..cut).collect();
        let (restored, report) = rehydrate_bytes(&emit, &self.map);
        self.restored_total = self.restored_total.saturating_add(report.restored);
        self.unrestored_total = self.unrestored_total.saturating_add(report.unrestored);
        restored
    }

    /// Flushes the held-back tail at end of stream; an unterminated token
    /// prefix is forwarded verbatim rather than dropped.
    pub fn finish(&mut self) -> Vec<u8> {
        if self.map.is_empty() {
            return std::mem::take(&mut self.pending);
        }
        let pending = std::mem::take(&mut self.pending);
        let (restored, report) = rehydrate_bytes(&pending, &self.map);
        self.restored_total = self.restored_total.saturating_add(report.restored);
        self.unrestored_total = self.unrestored_total.saturating_add(report.unrestored);
        restored
    }

    /// Total tokens restored so far; used by streaming callers to report
    /// incremental rehydration metrics without touching the map.
    pub fn restored_total(&self) -> u32 {
        self.restored_total
    }

    /// `__PKV_` markers emitted so far that matched no known token. The
    /// holdback keeps every `__PKV_`-initiated tail pending while it could
    /// still resolve, so the count is exact even when a marker straddles a
    /// chunk boundary.
    pub fn unrestored_total(&self) -> u32 {
        self.unrestored_total
    }

    /// Earliest index at which the pending tail is a proper prefix of a known
    /// token; bytes before it can never complete a token and may be emitted.
    /// Scans `__PKV_` occurrences left to right: complete token matches advance
    /// the resolved boundary so a token's own `__` terminator is not mistaken
    /// for the start of the next token.
    fn holdback_index(&self) -> usize {
        let len = self.pending.len();
        let mut resolved_end = 0usize;
        let mut scan = 0usize;
        while scan < len {
            let Some(offset) = find_subslice(&self.pending[scan..], TOKEN_PREFIX.as_bytes()) else {
                break;
            };
            let pos = scan + offset;
            if let Some((token, _)) = self.map.longest_match_at(&self.pending[pos..]) {
                resolved_end = pos + token.len();
                scan = resolved_end;
                continue;
            }
            if self.is_proper_token_prefix(&self.pending[pos..]) {
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
            if self.is_proper_token_prefix(&self.pending[idx..]) {
                return idx;
            }
        }
        len
    }

    /// Whether `tail` is a non-empty strict prefix of at least one known
    /// token. Keys starting with `tail` sort adjacently, so the first
    /// `range` entry decides — no full-map scan per holdback check.
    fn is_proper_token_prefix(&self, tail: &[u8]) -> bool {
        if tail.is_empty() {
            return false;
        }
        let Ok(tail) = std::str::from_utf8(tail) else {
            return false;
        };
        self.map
            .token_to_fragment
            .range::<str, _>((Bound::Included(tail), Bound::Unbounded))
            .next()
            .is_some_and(|(token, _)| token.len() > tail.len() && token.starts_with(tail))
    }
}

impl fmt::Debug for StreamRehydrator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamRehydrator")
            .field("pending_bytes", &self.pending.len())
            .field("map", &self.map)
            .finish()
    }
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

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
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
        rehydrate_text, rehydrate_value, RehydrationMap, StreamRehydrator, TokenDeriver,
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

    #[test]
    fn token_is_deterministic_and_identifier_safe() {
        let deriver = deriver();
        let first = deriver.base_token("acme-corp");
        let second = deriver.base_token("acme-corp");

        assert_eq!(first, second);
        assert!(first.starts_with("__PKV_"));
        assert!(first.ends_with("__"));
        assert!(first.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'));
    }

    #[test]
    fn different_keys_yield_different_tokens() {
        let left = TokenDeriver::new(b"key-one").base_token("acme-corp");
        let right = TokenDeriver::new(b"key-two").base_token("acme-corp");
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
        let (restored, report) =
            rehydrate_text("literal __PKV_ffffffffffff__ here", &map);
        assert_eq!(restored, "literal __PKV_ffffffffffff__ here");
        assert_eq!(report.restored, 0);
        // Well-formed but foreign token markers are counted as unrestored:
        // the LLM-mutated/unknown marker remains visible to the client and
        // the counter feeds audit/metrics observability.
        assert_eq!(report.unrestored, 1);
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
    fn stream_rehydrator_restores_token_split_across_chunks() {
        let deriver = deriver();
        let map = map_with(&deriver, &["acme-corp"]);
        let token = map.token_to_fragment.keys().next().expect("token exists").clone();
        let split = token.len() / 2;

        let mut rehydrator = StreamRehydrator::new(map);
        let mut emitted = Vec::new();
        emitted.extend(rehydrator.feed(b"data: \"use "));
        emitted.extend(rehydrator.feed(&token.as_bytes()[..split]));
        emitted.extend(rehydrator.feed(&token.as_bytes()[split..]));
        emitted.extend(rehydrator.feed(b"::init\"\n\n"));
        emitted.extend(rehydrator.finish());

        let text = String::from_utf8(emitted).expect("utf8 output");
        assert_eq!(text, "data: \"use acme-corp::init\"\n\n");
    }

    #[test]
    fn stream_rehydrator_flushes_unterminated_prefix_verbatim() {
        let deriver = deriver();
        let map = map_with(&deriver, &["acme-corp"]);
        let mut rehydrator = StreamRehydrator::new(map);
        let mut emitted = rehydrator.feed(b"tail __PKV_fff");
        emitted.extend(rehydrator.finish());
        assert_eq!(emitted, b"tail __PKV_fff".to_vec());
    }

    #[test]
    fn stream_rehydrator_empty_map_passthrough() {
        let mut rehydrator = StreamRehydrator::new(RehydrationMap::new());
        assert_eq!(rehydrator.feed(b"abc"), b"abc".to_vec());
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
        let fragments: Vec<String> =
            (0..500).map(|idx| format!("fragment-{idx:04}")).collect();
        let tokens: Vec<String> =
            fragments.iter().map(|f| map.token_for(&deriver, f)).collect();

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
