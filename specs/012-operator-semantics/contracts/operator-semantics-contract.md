# Contract: Operator Semantics Freeze

## 1. Scope

This contract freezes transform semantics for Pokrov core anonymization outcomes after policy resolution. It applies to plain-text and structured JSON flows consumed by runtime proxy and evaluation paths.

In scope:

- supported operators: `replace`, `redact`, `mask`, `hash`, `keep`
- deterministic transform order from resolved hits
- explicit blocked-vs-transformed outcomes
- JSON-safe structured traversal guarantees
- metadata-only explain/audit compatibility

Out of scope:

- runtime custom/lambda operators
- reversible anonymization/deanonymization
- external control-plane driven transform behavior

## 2. Supported Operator Contract

### 2.1 Allowed values

Only the following operator values are valid:

- `replace`
- `redact`
- `mask`
- `hash`
- `keep`

### 2.2 Unsupported operator behavior

Unsupported operator references MUST fail closed:

- final action is `block`
- metadata-only reason code is `unsupported_operator`
- no payload is forwarded downstream

## 3. Deterministic Execution Contract

Transform application order is fixed:

1. consume post-policy resolved hits
2. respect overlap suppression outcomes
3. apply operators in stable deterministic order
4. emit transformed result or terminal block outcome

Requirements:

- identical resolved-hit input + identical profile => identical output and metadata
- suppressed hits are never re-applied
- `hash` is one-way and deterministic within a profile context

## 4. JSON-Safe Transform Contract

For non-blocking outcomes:

- object/array structure remains intact
- only string leaves are transformed directly
- non-string leaves are unchanged
- output remains valid JSON

## 5. Blocked Outcome Contract

When final action is `block`:

- downstream execution receives no sanitized payload
- metadata-only explain and audit summaries are still produced
- block path is treated as a policy outcome, not runtime crash behavior

## 6. Metadata Safety Contract

Allowed metadata content:

- request_id
- decision/action summaries
- operator counts and statuses
- reason codes and precedence traces

Prohibited metadata content:

- raw payload fragments
- raw matched spans
- reversible secret material

Special rule:

- `keep` must be explicit in explain/audit metadata and never treated as silent passthrough.

## 7. Consumer Guarantees

Runtime/evaluation consumers may rely on:

- stable blocked/transformed outcome shapes
- deterministic replay semantics
- metadata-only explain/audit safety
- JSON validity preservation for structured non-blocking flows
