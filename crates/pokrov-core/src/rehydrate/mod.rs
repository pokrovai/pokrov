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
const TOKEN_HEX_LEN: usize = 64;

/// `__PKV_` plus the hex body; narrows `BTreeMap` lookups to the collision
/// domain of one base token instead of scanning every known token.
const TOKEN_BASE_LEN: usize = TOKEN_PREFIX.len() + TOKEN_HEX_LEN;

mod sse;

pub use sse::{
    event_boundary_end, has_ambiguous_chat_tool_members, EventRehydrator, SseStreamRehydrator,
    MAX_EVENT_BUFFER_BYTES,
};

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

    /// Returns the deterministic token for a fragment: `__PKV_<64 lowercase
    /// hex>__` — the fragment's full keyed HMAC-SHA256 rendered. The mapping
    /// is a pure function of (key, fragment): identical fragments yield
    /// identical tokens across requests and insertion order can never affect
    /// the result, as FR-002 requires.
    ///
    /// For completeness the map still disambiguates a collision with a
    /// `_c<8hex>` suffix drawn from chained HMAC material
    /// (`fragment#pkv-collision-<level>`); the suffix sequence is
    /// deterministic per fragment. Reaching it at all requires a full
    /// 256-bit HMAC-SHA256 collision — a hash break, at which point no
    /// tokenization scheme survives anyway.
    pub fn token_for(&mut self, deriver: &TokenDeriver, fragment: &str) -> String {
        self.spans_total += 1;
        if let Some(token) = self.fragment_to_token.get(fragment) {
            return token.clone();
        }

        let base =
            format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", hex_encode(&deriver.digest_for(fragment)));

        let mut token = base.clone();
        let mut level = 0u32;
        while self.token_to_fragment.contains_key(&token) {
            level += 1;
            let suffix = collision_suffix(deriver, fragment, level);
            token =
                format!("{}_c{}{}", &base[..base.len() - TOKEN_SUFFIX.len()], suffix, TOKEN_SUFFIX);
        }

        self.token_to_fragment.insert(token.clone(), fragment.to_string());
        self.fragment_to_token.insert(fragment.to_string(), token.clone());
        token
    }

    /// Longest known token matching the head of `text`, or `None`.
    /// Collision-suffixed tokens share the base prefix, so longest wins.
    /// The 70-byte `__PKV_<hex64>` base narrows the BTreeMap range scan to
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
/// chained HMAC over `fragment#pkv-collision-<level>` — the base token
/// already consumes the entire fragment digest. Because every suffix derives
/// from the fragment's own keyed hash chain, the escalation sequence never
/// depends on request content or mint ordering.
fn collision_suffix(deriver: &TokenDeriver, fragment: &str, level: u32) -> String {
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
mod tests;
