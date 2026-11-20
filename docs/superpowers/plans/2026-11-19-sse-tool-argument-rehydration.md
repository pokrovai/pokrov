# SSE Tool Argument Rehydration Implementation Plan

**Goal:** Restore request-known tokens in streaming Chat Completions tool arguments without buffering complete arguments, changing upstream event order, or exposing restored values to diagnostics.

**Architecture:** Extend the existing SSE rehydrator with an explicit Chat Completions mode. An incremental inner-JSON parser owns state per `(choice.index, tool_calls[].index)`. Both proxy paths use the same restoration engine; the buffered path still evaluates output policy and emits its configured payload trace before calling it.

**Tech stack:** Rust 2021, stable 1.85+, existing `serde`, `serde_json`, Tokio, reqwest, and Cargo test harnesses.

**Spec:** `docs/superpowers/specs/2026-11-19-sse-tool-argument-rehydration-design.md`, extending `specs/015-reversible-tokenization/spec.md`.

## Global Constraints

- Enable the new mode only on streaming `/v1/chat/completions`, only with a nonempty request map, and only for `choices[].delta.tool_calls[].function.arguments`. Do not change legacy `function_call`, Responses, Anthropic, non-streaming, or MCP behavior. Preserve existing text and generic leaf restoration on unrelated fields of processable events.
- Match literal bytes of the **decoded outer** `arguments` string. Do not decode inner JSON escapes for token matching. Escape substitutions for the **inner** string first; let serde encode the outer event second.
- No new dependencies, config options, metrics, labels, diagnostic payload fields, or trace points. Retain the configured buffered-body limit and all 128 existing text carry keys. Keep changed Rust files at or below 550 lines; declare and describe extracted submodules.
- **Verified extension points:** `EventRehydrator::rehydrate_json_event` currently sends argument strings through generic `rehydrate_value`, which can replace a whole token without understanding inner JSON. `SseStreamRehydrator::{feed, finish, drain}` owns transport framing; its current error drain emits the raw tail before synthetic carries. `LLMProxyHandler::handle_stream_response` has a buffered policy → trace → restore branch and a live branch that reports cumulative restoration deltas. The live branch has no response-payload trace. The buffered `sanitize_sse_stream` splits on `\n\n` and reconstructs a terminated body, so it cannot be reused unchanged for Chat argument framing or raw-tail provenance.
- **Design gate:** `CONDITIONAL` pending implementation evidence for wire compatibility, policy parity, and the latency budget. If implementation disproves bounded incremental delivery or the ordering/privacy contracts below, stop and report the counterexample rather than buffering full arguments or silently narrowing scope.

### Bounds and fallback contract

| Resource | Bound and behavior |
|---|---|
| Live argument pairs | **32** per response, separate from text carry. At the 33rd simultaneous admission, pass that pair through and close **new** admissions for the rest of the response; existing trusted pairs continue. This prevents a rejected pair from being mistaken for a fresh JSON document when a slot later frees. Before any rejected admission, a finished choice frees its slots for new choices. |
| Choice fences | **32** choice IDs for completed or quarantined choices. These are index-only records, not retained parsers. If the fence table fills, close new admissions; release affected parsers normally and continue trusted, unaffected pairs. |
| Inner parser | **32** nested object/array levels, excluding the root sentinel. Fixed stack and scalar/escape state; no retained keys, strings, assembled JSON, or argument history. On depth 33 or lexical/grammar error, emit the previously held prefix followed by the unmodified current field, then quarantine that pair. |
| Pending token prefix | **81 bytes per pair, 2,592 bytes total.** Current tokens are 72 bytes normally or 82 with `_c<8hex>`. Retain only a nonempty **strict** prefix of a token in the request map; a complete match is emitted immediately, and a disproved prefix is released immediately. Assert in tests that both minted forms fit the matcher. |
| Outer work | Inspect at most **128 combined choice and tool-call entries per event**. Over-limit events are opaque for tool restoration. Keep bounded per-event descriptors; never allocate a descriptor for every item in an oversized array. Use the existing **1,048,576-byte** SSE event budget for Chat restoration, measured before the blank-line delimiter and independent of transport chunking. Buffered body collection remains at its existing configured limit (default 1 MiB; valid range 1 KiB–16 MiB). |
| Synthetic metadata | Retain one response envelope lazily, only when a suffix must be held: `id` and `model` at most **256 UTF-8 bytes each**, and numeric `created`. Do not truncate metadata. If a chunk cannot supply a bounded envelope and a hold would be needed, release its suffix literally instead. For upstreams that omit envelope fields, omit the same fields in the added chunk. |

Target **≤8 KiB retained tool state per active response**, including table capacities, prefix arrays, parser stacks, choice fences, and envelope. Test the size budget from the actual Rust types and capacities. This is separate from the existing SSE and policy buffers and gives bounded lookup and flush work; it does not by itself prove the **≤50 ms P95** budget. No full-argument accumulation, additional I/O, request-map copy, or per-byte allocation is needed.

**Association and gap rules:**

- Use `Value::as_u64` for both indices. Do not infer indices from positions, names, or IDs. Preflight the entire event before mutating any argument, so a second occurrence cannot invalidate an already-restored first occurrence.
- A duplicate tool index or invalid argument field with a known pair quarantines that pair. Missing/invalid tool index or a malformed tool-call list under a known choice quarantines that choice, including admission of unseen pairs under it. Duplicate choice indices quarantine that choice. Missing/invalid choice identity on an argument-bearing or finishing choice, untrustworthy duplicate JSON members, and opaque frames whose affected choices cannot be located flush all pending tool bytes and close new admissions. Affected states never restart on a later fragment merely because it begins with `{` or `"`. Well-identified other choices continue after a localized gap. Metadata-only valid deltas, usage chunks, and SSE comments/keepalives do not create a gap.
- Process a finishing choice’s final argument delta first. If that call has a pending suffix afterward, append it to the same `arguments` field. For a pending call with no usable final field in that event, emit a synthetic chunk **before** the original event. Then release all parser state for the choice. Synthetic chunks use `object: "chat.completion.chunk"`, the bounded upstream envelope, the original numeric indices, `finish_reason: null`, and `delta.tool_calls[].function.arguments`; they do not repeat tool names or IDs or invent a finish. Flush other terminal or ambiguous suffixes in deterministic choice/tool index order. Forward all original events in their original order.
- `[DONE]`, EOF, a forwarded body error, and opaque/incomplete bytes flush pending arguments literally **before** the marker, raw bytes, or error. Never create `[DONE]` or a successful finish. Downstream cancellation drops state without attempting output. A buffered read error or policy block still returns the existing error response; there is no partially delivered buffered SSE body to flush.
- `restored` increments once when a substitution is committed to output, not once per input delta. `unrestored` counts a complete literal `__PKV_` marker left visible in eligible text or counted on a bypass/flush, including a held marker released incomplete; a held `_` or `__PKV` is not yet a marker. Do not count intentionally ineligible inner keys as restoration attempts. Do not stitch a marker across opaque segments for counting, count held bytes twice, or rescan restored originals. Keep existing raw-bypass counting best-effort for markers split across raw segments. Neither report contains payload data.

## Review Focus

The approved spec has six numbered requirements and four completion/fallback bullets. The eight acceptance rows are mapped below.

| Spec acceptance row | Required evidence |
|---|---|
| 1. Whole and arbitrarily split known tokens | Task 1 `restores_whole_and_every_argument_split`; Task 2 `chat_transport_splits_are_equivalent` |
| 2. Both JSON levels, special characters, and split escapes | Task 1 `escapes_original_and_preserves_split_inner_escapes`; Task 3 HTTP round-trip |
| 3. Multiple choices/calls, multiple deltas per event, member order | Task 1 `isolates_interleaved_pairs_and_member_order` |
| 4. Keys, unknown markers, ambiguous association | Task 1 `leaves_keys_unknown_tokens_and_ambiguous_indices_literal`; Task 4 buffered duplicate-member regression |
| 5. Final delta, `[DONE]`, EOF, upstream error | Task 1 `processes_final_delta_before_choice_finish`; Tasks 2–3 terminal and abort tests |
| 6. Limits, bypass, and independent text capacity | Task 1 `bounds_state_without_using_text_carry_slots`; Task 2 `chat_opaque_frames_flush_then_break_continuity` |
| 7. Policy on/off parity and precedence | Task 4 `policy_modes_agree_when_fragments_survive`, `output_policy_transform_and_block_win`, and the trace test |
| 8. No eligible token | Task 1 `unchanged_event_reuses_original_bytes`; Task 3 no-eligible HTTP regression in both modes |

## Implementation Tasks

### Task 1: Implement bounded inner parsing and indexed restoration at the core event layer

**Depends on:** None.

**Files:**
- Modify `crates/pokrov-core/src/rehydrate/mod.rs` and `crates/pokrov-core/src/rehydrate/sse.rs`.
- Create `crates/pokrov-core/src/rehydrate/sse/tool_arguments.rs`, `sse/tool_arguments/inner_json.rs`, and `sse/tool_arguments/shape.rs` under that directory.
- Create `crates/pokrov-core/src/rehydrate/sse/tool_arguments/tests.rs`. Move existing inline suites unchanged to `crates/pokrov-core/src/rehydrate/tests.rs` and `crates/pokrov-core/src/rehydrate/sse/tests.rs`.

**Interfaces:**
- Consumes `RehydrationMap`, its exact-match/prefix helpers, `count_unrestored_markers`, and `RehydrateReport`.
- Adds `EventRehydrator::new_chat_completions(map: RehydrationMap) -> Self`; existing `new` retains its behavior.
- Adds internal `ArgumentKey { choice: u64, tool: u64 }`, `ArgumentStreams`, and `ArgumentDeltaState::push<'a>(&mut self, input: &'a str, map: &RehydrationMap) -> ArgumentDelta<'a>`. `ArgumentDelta` returns a `Cow<'a, str>`, a report, and a trusted/opaque outcome.
- Exposes documented `has_ambiguous_chat_tool_members(payload: &str) -> bool` from `pokrov_core::rehydrate` for Task 4. It operates on already valid outer JSON. An ordinary `Value` parse does not preserve duplicate object members.

- [ ] Add tests with real request maps: whole tokens; every two-way split and byte-by-byte deltas, including a deterministic test fixture for the collision spelling; adjacent tokens; token at a closing quote; a restored original containing a literal token spelling; and fragments with `"`, `\`, LF, CR, tab, NUL, and Unicode. Assemble deltas by pair, parse each outer event and assembled inner JSON, and assert surrounding bytes and exact decoded values. Split existing `\"`, `\\`, and `\uXXXX` across deltas. Assert an inner escaped spelling of a marker does not match.
- [ ] Implement the inner byte lexer. Use explicit object phases (first key or end, required key after comma, colon, value, comma or end), array phases (first value or end, required value after comma, comma or end), and root phases (value, done). Distinguish key strings from value strings; track backslash and four hex digits across fragments. Track literals and numbers with finite states and reconsume a delimiter after a completed scalar. Only literal, unescaped runs within a value string feed the token matcher. A closing quote, escape, or other disproof releases a candidate without joining across it. Retry overlapping candidate starts within the bounded token window so a stray underscore cannot hide a following known token. Do not feed restored bytes back into either parser or matcher.
- [ ] Append restored fragments directly into the output string: escape quote, backslash and controls, and copy Unicode spans unchanged. Borrow unchanged input and allocate output only for an actual replacement, carry movement, or fallback that prepends a held prefix. On parser fault, discard tentative edits and their counts for that field, release the pre-field prefix exactly once, and pass the field through.
- [ ] In `shape.rs`, preflight decoded paths and numeric indices; record **positions for writeback only**, never as identities. Detect repeated indices before processing any field. Use a bounded serde visitor with `IgnoredAny` on unrelated branches to check duplicate JSON members along the relevant paths (`choices`, `index`, `delta`, `tool_calls`, `function`, `arguments`, `finish_reason` in their respective objects). This also detects escaped duplicate member names. A single data payload with untrustworthy duplicate members is opaque; do not act on serde's last-wins interpretation.
- [ ] Extend `EventRehydrator::rehydrate_json_event` to move targeted `arguments` values out before `rehydrate_value`, including values at identifiable but invalid slots. Do not allow generic restoration inside a malformed tool-call subtree. Restore the detached values once using the argument engine or reattach them unchanged on bypass. Keep text carry processing independent. For valid association gaps, flush before the event and continue legacy handling of unrelated fields. For a truly opaque frame, use raw forwarding, not the legacy plain-line substituter. Re-encode the outer data line only when a value changes, preserving SSE control lines. Extend `[DONE]`, `finish`, reporting, and idle inspection to include argument suffixes; keep tool flushing separately callable inside the module for Task 2.
- [ ] Add the named tests from the review table. Include negative, missing, fractional and string indices; repeated choice/tool indices; duplicate JSON members; non-string arguments; syntax and depth faults; 32/33 pair and choice-fence boundaries; a late delta after finish; and a finishing event carrying final pieces for one call while another call and another choice have pending prefixes. Fill all **128** text carry slots while exhausting tool capacity; complete the text tokens and check their counts. Assert a healthy no-marker event with metadata and inner escapes is byte-identical, with a nonempty map.
- [ ] Run `cargo test -p pokrov-core rehydrate::` (new tests should fail before the behavior is installed and all pass at handoff).

**Acceptance:** The new core event mode restores only eligible values, rejects ambiguous joins, emits choice finishes in order, and satisfies the state bounds. All pre-existing core text, Responses, and generic restoration tests pass.

### Task 2: Complete the Chat byte-stream driver and its opaque/terminal ordering

**Depends on:** Task 1.

**Files:**
- Modify `crates/pokrov-core/src/rehydrate/sse.rs` and the re-exports in `crates/pokrov-core/src/rehydrate/mod.rs`.
- Move the byte-level implementation from `sse.rs` to `crates/pokrov-core/src/rehydrate/sse/stream.rs`.
- Create `crates/pokrov-core/src/rehydrate/sse/stream/chat_tests.rs`.

**Interfaces:**
- Adds `SseStreamRehydrator::new_chat_completions(map: RehydrationMap) -> Self`.
- Adds `SseStreamRehydrator::bypass_complete_event(&mut self, frame: &[u8]) -> Vec<u8>` for a caller supplying one complete frame at a frame boundary when pre-policy provenance requires raw forwarding. It emits pending tool suffixes before `frame` and invalidates tool continuity. The existing `new`, `event_boundary_end`, `feed`, `finish`, `drain`, `report`, and `is_idle` signatures stay intact.

- [ ] Add `chat_transport_splits_are_equivalent`, `chat_opaque_frames_flush_then_break_continuity`, and `chat_terminal_flush_precedes_raw_tail`. Compare assembled JSON and, for unchanged fixtures, exact bytes for all transport splits of small LF, CRLF, and lone-CR frames, including splits in UTF-8, outer escapes, the token, and terminators.
- [ ] Extend `feed`/`emit_block` in Chat mode. One single-`data` JSON line is eligible. A non-JSON data payload, multi-`data` frame, invalid UTF-8, in-band error frame (`event: error` or a top-level error object without a usable chunk), or overflow is opaque. At the first bypassed byte, flush pending arguments and invalidate association; do not generate another flush for the same overflow remainder. Enforce the same cap for a large complete frame delivered in one chunk and the same frame arriving piecemeal. Forward each raw block or overflow remainder once. Preserve original separators; after consuming a blank-line delimiter ending in CR at the end of a transport chunk, treat an immediately following LF as the rest of that delimiter, not as the beginning of a new frame. Keep the existing incremental scan cursor, and compact consumed buffer bytes once per feed, not once per event.
- [ ] In Chat `finish` and `drain`, write argument suffix chunks before an unterminated raw tail; do not send that tail through argument or plain-line restoration. Keep the pre-existing text-carry order on `drain` and the original byte-driver behavior in the default mode. `[DONE]` releases argument state and prevents later bytes from starting new tool parsers. `is_idle` includes pending argument bytes, not merely the presence of parser records. Drop performs no writes.
- [ ] Test `[DONE]`, EOF with and without raw bytes, `drain` with a raw tail, invalid UTF-8, malformed and multi-data frames, and over-limit frames delivered whole and split. For each gap, verify a literal suffix precedes the raw bytes, later lookalike fragments do not complete a token across the gap, and healthy later text still restores. Verify both cap boundaries and exactly-once reports after repeated terminal calls.
- [ ] Run `cargo test -p pokrov-core rehydrate::`.

**Acceptance:** Both frame partitioning and failure paths preserve every received argument byte that cannot safely be transformed. The driver emits safe data during generation, not only on `finish`. The default driver preserves its existing behavior.

### Task 3: Enable the shared driver in the live Chat proxy path

**Depends on:** Task 2.

**Files:**
- Modify `crates/pokrov-proxy-llm/src/handler/streaming.rs`.
- Create `crates/pokrov-proxy-llm/src/handler/streaming/passthrough.rs` for the live rehydration and deferred audit code currently in `streaming.rs`.
- Modify `tests/integration.rs`; create `tests/integration/llm_sse_tool_arguments_path.rs` and `tests/common/sse_tool_argument_test_support.rs`. Reuse `tests/common/llm_proxy_test_support.rs` unchanged.

**Interfaces:**
- Consumes `SseStreamRehydrator::new_chat_completions`.
- Keeps `LLMProxyHandler::rehydrating_byte_stream(&self, stream: SseBodyStream, map: RehydrationMap, audit: StreamAuditContext) -> SseBodyStream`. Use `audit.endpoint == "/v1/chat/completions"` at driver construction; no exported handler signature changes.

- [ ] Register HTTP tests using a reversible input rule and a mock provider that gets a deterministic known token. Put reusable config and request construction in the common fixture. Use existing `MockProviderMode::{Sse, SseChunked, SseAbort}`; do not extend the provider protocol or test infrastructure.
- [ ] Move `stream_audit`, `rehydrating_byte_stream`, and `StreamAuditContext` to `streaming/passthrough.rs` with parent-visible interfaces only. Select the Chat constructor only after the existing empty-map fast path. Leave the Responses conversion caller on the default constructor. Preserve `State::report`, filtering of empty chunks, deferred audit, backpressure, and the existing error-relay sequencing, including the existing delay before a queued error. Do not add any delay, trace, retry, or timeout.
- [ ] Add `live_tool_arguments_arrive_before_upstream_completion`: assert output available before a delayed follow-up provider chunk; the first event must contain the safe JSON prefix and no unresolved held bytes. Cover transport splitting and escaping in the same round-trip fixture. Verify captured upstream request uses the token and does not contain the original.
- [ ] Add `live_tool_suffix_precedes_done_eof_and_abort`: assert output offsets for the final argument/finish and `[DONE]`, for EOF with a raw partial event, and for `SseAbort` with a pending suffix and raw partial bytes. Read successful `bytes_stream()` items before the original `Err`; do not use `response.text()` for the abort case. Assert no fabricated terminal chunk. Add `live_tool_argument_no_eligible_token_preserves_frames` using a nonempty request map and assert exact SSE bytes and unchanged metadata. Check `/metrics` on a fresh runtime: a token split over several deltas increments `pokrov_rehydrated_tokens_total` by one; a flushed complete marker increments `pokrov_unrestored_tokens_total` by one. Counters remain unlabeled and contain no argument data.
- [ ] Run `cargo test -p pokrov-proxy-llm` and `cargo test --test integration llm_sse_tool_arguments_path::`.

**Acceptance:** The live Chat endpoint restores arguments before upstream completion, reports each occurrence once, and relays upstream errors after queued bytes. The change neither buffers the live body nor affects the Responses or empty-map callers.

### Task 4: Enable buffered Chat restoration after policy and trace, and verify parity

**Depends on:** Tasks 2–3.

**Files:**
- Modify `crates/pokrov-proxy-llm/src/handler/streaming.rs` and `crates/pokrov-proxy-llm/src/stream.rs`.
- Create `crates/pokrov-proxy-llm/src/stream/chat.rs`, `crates/pokrov-proxy-llm/src/handler/streaming/trace_tests.rs`, and `tests/integration/llm_sse_tool_argument_policy_path.rs`; register in `tests/integration.rs`.
- Add the release-mode test in `tests/performance/llm_sse_tool_argument_overhead.rs`; register it and the shared fixture in `tests/performance.rs`.
- Update the reversible-tokenization subsection in `docs/configuration.md` during implementation. No repository changelog was found; do not create one. No changes to metrics production files or Cargo manifests.

**Interfaces:**
- In `stream/chat.rs`, add crate-private `ChatPolicyBody { body: String, first_source_barrier: Option<Range<usize>> }`, `sanitize_chat_sse_stream(request_id: &str, profile_id: &str, raw_body: &str, evaluator: &SanitizationEngine) -> Result<ChatPolicyBody, LLMProxyError>` with the existing hit/action totals on its result, and `rehydrate_chat_sse_stream(body: ChatPolicyBody, map: RehydrationMap) -> (String, RehydrateReport)`. Keep the existing public `sanitize_sse_stream` and `rehydrate_sse_stream` signatures and their default callers.
- Extract a private per-JSON-payload policy helper from `sanitize_sse_stream`, shared with the Chat sanitizer. In `streaming.rs`, put the buffered trace-then-restore sequence in a private `LLMProxyHandler::finalize_buffered_sse` helper so the real production order can be tested with the existing feature-gated sink.

- [ ] Add the buffered regression first: a duplicate relevant JSON member is present on the wire, policy evaluation still runs, and after serde normalizes the payload its apparent index cannot be used to complete a pending argument. Also include CRLF input and a valid-looking but unterminated tail. The tail must not become a new complete argument event.
- [ ] For Chat mode, use the shared `event_boundary_end` over the buffered body to preserve physical frames and original separators, including the distinction between a complete frame and a raw tail. Use the exact existing `SanitizationEngine::evaluate` request, result aggregation, and error mapping for parseable data payloads. Do not restore or bypass policy during collection. Multi-data, ambiguous, and large frames still get the existing per-parseable-line policy evaluation; keep frame boundaries and data-line cardinality so the restorer can recognize opaque shapes. On malformed data, where no policy JSON can be evaluated, retain the original line.
- [ ] Record the first **post-policy byte range** corresponding to a complete source frame whose duplicate-member ambiguity or raw oversize could be hidden by re-encoding. Check source ambiguity before converting to `Value` for policy. Evaluate that frame normally; the range is out-of-band, never a payload field. Only the first such range is needed: the global barrier flushes all argument parsers and permanently closes admissions. For later normalized frames, still exclude all argument fields from generic restoration. In `rehydrate_chat_sse_stream`, move the map into the driver, feed bytes up to the recorded range, pass that complete frame to `bypass_complete_event`, feed the remainder, then finish. If no source barrier exists, feed the body directly. With no evaluator, use the unchanged source body and let the driver detect its barriers. The buffered body is UTF-8 and the driver emits that body plus UTF-8 serde output; preserve that invariant in the string conversion without lossy decoding.
- [ ] In `handle_stream_response`, select the Chat buffered adapter only for the exact endpoint and a nonempty map. First collect under `read_stream_body_with_limit`; then run policy under the existing `spawn_blocking` branch; then emit the existing feature-gated response trace with the post-policy, pre-restore body; then restore and publish totals. Leave the Responses conversion and default helper call order intact. Block/read-error paths must not reach the buffered finalizer. Buffered EOF flushes successful received suffixes; a buffered upstream abort returns the existing error before any SSE body is exposed.
- [ ] Add `policy_modes_agree_when_fragments_survive` using identical streams on policy-on/off routes for LF, CRLF, and CR: compare reconstructed arguments and unrelated fields, not policy-induced outer JSON formatting. Include a nonempty-map/no-eligible case in both modes. `output_policy_transform_and_block_win` uses a rule that transforms part of a would-be matching token, and a separate rule that blocks output; assert transformed bytes win, the blocked route returns the existing policy error, and neither produces a restoration count. Also check that output policy can still block a parseable ambiguous event.
- [ ] Under `#[cfg(all(test, feature = "llm_payload_trace"))]`, use a temporary `LlmPayloadTraceSink` and call the real buffered finalizer with a known split token. Assert the trace record `llm_final_response_payload` contains the pre-restore fragments, not the decoded original, whereas the returned client body reconstructs the original. Clean up the temporary trace. Assert no new response trace is added on the live route. Preserve existing audit fields `tokenized_spans_total`, `rehydrated_tokens_total`, and `unrestored_tokens_total` and Prometheus series `pokrov_tokenized_spans_total`, `pokrov_rehydrated_tokens_total`, and `pokrov_unrestored_tokens_total`; no caller migrations outside the two targeted proxy paths.
- [ ] Add `llm_sse_tool_argument_overhead_stays_within_budget`: use a local, no-delay provider, 32 KiB of representative traffic with interleaved calls and a mix of split, whole, and absent tokens, five warm-ups and 40 measured requests per policy mode. Measure through full body drain, with fixture generation outside the timer. Use the existing performance harness convention and gate each mode at P95 ≤50 ms in release. Record measured results; do not infer compliance from the memory cap.
- [ ] Document literal-only restoration, eligible inner values, both Chat modes, and bounded fail-visible behavior in `docs/configuration.md`. Replace the existing blanket statement that split tokens stay visible: an exact known split token can restore; mutated, incomplete, or bypassed markers stay literal. Keep config keys/defaults unchanged.
- [ ] Run `cargo test --test integration llm_sse_tool_argument_policy_path::`, `cargo test -p pokrov-proxy-llm --features llm_payload_trace`, and `cargo test --release --test performance llm_sse_tool_argument_overhead_stays_within_budget`.

**Acceptance:** Both routes use the same restoration semantics after policy where applicable. A policy block or transformation is not bypassed, and restored originals appear only on the intended client response path.

## Dependency / Execution Order

**Task 1 → Task 2 → Task 3 → Task 4.** Each task ends with a compiling tree and passing focused checks; no task requires another implementer’s session context. The first two tasks ship tested core entry points without changing production routing. Task 3 wires live Chat; Task 4 completes buffered Chat and cross-path evidence.

## Integrated Verification

Run after all tasks land. Do not auto-format. No commands were run during planning.

```sh
cargo test -p pokrov-core rehydrate::
cargo test -p pokrov-proxy-llm --features llm_payload_trace
cargo test -p pokrov-metrics exposes_tokenization_counters_in_prometheus_render
cargo test --test integration llm_sse_tool_argument
cargo test --test integration reversible_tokenization_path::
cargo test --test integration responses_
cargo test --release --test performance llm_sse_tool_argument_overhead_stays_within_budget
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
cargo fmt --check
```

## Risk Register

| Priority | Risk | Evidence required before merge |
|---|---|---|
| P0 | Raw replacement, duplicate outer members, or a gap can corrupt JSON or assign a restored value to the wrong call. | Tasks 1–2 must pass dual-decode assertions, all split tests, and association/gap negatives. Task 4 must prove policy normalization cannot erase source ambiguity. If bounded parsing cannot preserve both levels, reopen the delivery contract. |
| P0 | Restored values could be re-evaluated, traced, or logged. | Run the policy block/transform and feature-gated trace tests. Verify the existing audit and metrics publish numeric totals only. Do not add a live response trace to obtain evidence. |
| P1 | Synthetic chunks or re-encoding could change client-visible ordering, framing, or completion. | Test a standard envelope, same-event final arguments, separate pending calls, transport splits, raw tails, and a real HTTP body error. Assert unchanged frames byte-for-byte when no edit is needed; compare decoded fields when policy or restoration re-encodes. |
| P1 | Event rescanning, descriptor work, or allocations could miss the latency budget. | Run the release-mode P95 test in both modes; verify state-size assertions, one bounded prefix per pair, and no repeated full-buffer compaction or request-map copy. Record results, not estimates. |
| P2 | Existing files are already near or over the line cap. | Use the named extraction points: core legacy tests and byte driver, proxy passthrough/audit, and new tests in focused modules. Verify changed Rust file sizes without restructuring unrelated policy, metrics, or endpoint code. |