# SSE Tool Argument Rehydration

This specification extends [Reversible Tokenization](../../../specs/015-reversible-tokenization/spec.md) for streaming `/v1/chat/completions`. When a model emits a known token across SSE deltas in `choices[].delta.tool_calls[].function.arguments`, the client must reconstruct the original argument value by concatenating the deltas for that tool call as usual.

The scope covers modern OpenAI Chat Completions tool calls with either value of `output_sanitization`. Legacy `function_call`, Responses, Anthropic, non-streaming responses, and MCP are outside this change.

## Approaches

The selected approach processes argument deltas incrementally. It tracks the inner JSON context and holds only a trailing prefix that can complete a known token. With output sanitization disabled, arguments continue to reach the client during generation.

Buffering a complete argument object would allow ordinary JSON parsing before substitution. That approach delays argument delivery until completion and requires memory proportional to the size of the arguments. It was considered because it simplifies validation of the inner JSON.

## Requirements

1. Use the existing map for the current request and match the literal token spelling in the decoded SSE argument deltas. Handle both whole tokens and tokens split across any number of SSE deltas. Run reconstruction and restoration after output policy evaluation and payload tracing wherever those stages apply.
2. Associate argument deltas with the pair of nonnegative integer `choice.index` and `tool_calls[].index` values. Keep argument state separate from text deltas. Handle multiple choices and calls in a single event and interleaved calls across events. JSON member order and the presence of a tool ID or name in subsequent deltas have no effect on association.
3. Restore tokens only inside string values of the inner argument JSON. Apply JSON string escaping to restored fragments before encoding the outer SSE event. Preserve argument keys, structural punctuation, and other bytes around the replacement. For well-formed upstream arguments, the concatenated result must remain valid JSON and contain the exact original string value, including quotes, backslashes, control characters, and Unicode. Apply this rule to whole tokens too, and process each argument field once.
4. Preserve the order of argument bytes within each call and the order of upstream events. Preserve tool names, tool IDs, indices, finish reasons, and fields unrelated to argument restoration. Forward eligible argument bytes as soon as they can be emitted safely. A token split only across transport chunks must work equivalently to the same bytes delivered in a single chunk.
5. Retain bounded state per stream. Limit the number of tracked argument streams, the JSON parsing state, and retained token-prefix bytes. The prefix must be shorter than a known token it can complete. Existing SSE buffering limits and text restoration capacity continue to apply. Release argument state at choice completion or stream teardown.
6. Use the existing metadata-only restoration counters. A successful restoration counts once, regardless of how many deltas carried the token. The restoration stage emits no argument contents or restored fragments to diagnostic sinks. Any configured payload trace remains before restoration.

## Completion and fallback

- If a choice finishes in an event with a final argument delta, process that delta before releasing its pending suffix. Flush any remaining argument suffixes for that choice before the client observes its `finish_reason`. Other choices continue independently. Any added delta uses the original choice and tool indices in a client-compatible Chat Completions chunk.
- At `[DONE]`, EOF, or a forwarded upstream error, emit pending suffixes as received while the downstream connection can still accept output. A pending suffix goes before the terminal marker or, if present, before any raw incomplete event forwarded at the end. Preserve upstream error behavior and discard state when the downstream connection closes.
- Unknown, mutated, and incomplete tokens remain visible. If an index is missing, invalid, or ambiguous, or an event is bypassed, flush any earlier pending bytes for the affected calls before forwarding the opaque event. When the affected calls cannot be identified, flush all pending argument bytes. Never guess the destination for an unassociated fragment.
- When safe JSON context is unavailable or a state limit is reached, pass the affected argument bytes through. Resume restoration only where a safe context can be established. An unknown or bypassed segment must never be joined with a later fragment as if the segment had not occurred. A malformed upstream argument stream can remain malformed. The proxy preserves received bytes where it cannot transform them safely.

## Acceptance criteria

| Scenario | Expected result |
| --- | --- |
| Known token appears whole, or is split at any position into two or more argument deltas | After concatenation, the tool receives the exact original value with both levels of JSON valid. Other argument bytes are preserved. |
| Restored value contains quotes, a backslash, LF, CR, a tab, or Unicode, or an existing JSON escape spans deltas | Decoding the assembled arguments returns the original string. Existing escapes remain intact and each SSE event remains valid. |
| Two tool calls in one choice, several choices, or several tool deltas in one event | Each call receives its own bytes and restored values. A `function` member preceding `index` behaves identically to the reverse order. |
| A token appears in an inner JSON key, an unknown marker appears in a value, or index association is ambiguous | The affected marker remains literal. No bytes are assigned to another call. |
| Final argument bytes and `finish_reason` arrive together, or the stream ends at `[DONE]`, EOF, or an upstream error | Remaining bytes preserve their per-call order and precede termination when the client can still receive them. No successful terminal event is fabricated. |
| Argument state reaches its limit or an event must be passed through | Pending bytes are preserved. Restoration does not join across the opaque fragment, and ordinary text restoration retains its existing capacity. |
| The same stream is served with and without output sanitization | Both paths reconstruct the same arguments when the output policy leaves the token fragments intact. A policy block or transformation continues to take precedence. |
| A stream contains no eligible tokens | Argument contents and unrelated SSE fields remain unchanged. |

## Implementation handoff

The design is ready for planning. Verify the argument parsing and fallback boundaries against the existing SSE paths before implementation. Choose explicit bounds for argument parsing state within the existing stream budget. If the parser cannot preserve both JSON levels incrementally within bounded state, reopen the delivery contract before changing behavior.
