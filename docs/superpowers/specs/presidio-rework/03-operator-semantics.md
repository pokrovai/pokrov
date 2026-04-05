# Operator Semantics

Date: 2026-04-05
Status: Draft

## Context

Pokrov core needs a fixed set of safe anonymization operators that can be applied predictably after policy resolution.
This specification freezes the supported operators and their shared semantics so handler families and structured processing can target one transform contract.

## Goals

- Define the supported core operators.
- Freeze the execution order after overlap resolution.
- Guarantee valid JSON output for non-blocking flows.
- Keep operator behavior testable and deterministic.

## Supported Operators

- `replace`
- `redact`
- `mask`
- `hash`
- `keep`

## Non-Goals

- Runtime custom lambda operators.
- Reversible encrypt/decrypt paths in core.
- Deanonymization inside the proxy core.

## Shared Semantics

### `replace`
- Replaces the matched span with a configured stable replacement value.
- Replacement may depend on entity, category, and profile.
- Replacement must not require runtime code execution.

### `redact`
- Removes or hides the matched span using a fixed safe redaction strategy.
- Must preserve valid JSON and valid string encoding.

### `mask`
- Applies deterministic masking with profile-controlled masking parameters.
- Must define how visible prefix or suffix rules are applied.

### `hash`
- Produces a one-way transformed value.
- Must be deterministic only when the chosen profile says so.
- Core support is limited to safe one-way hashing semantics.

### `keep`
- Explicitly preserves the original content.
- Must remain visible in safe explain and audit as an intentional policy outcome.

## Application Rules

- Operators apply only to resolved hits after policy resolution.
- Application order is derived from resolved hit ordering and must be deterministic.
- The same resolved hits must always yield the same transform result.
- Overlap-aware application must not re-open suppressed spans.

## Blocking Rules

- If final action is `block`, no sanitized payload is returned to downstream execution.
- Blocked outcomes must still produce safe explain and audit sections.
- Blocked outcomes are not runtime failures.

## JSON Validity Rules

- Object and array structure must remain intact for non-blocking results.
- Only string leaves may be transformed directly.
- Non-string leaves remain unchanged.

## Acceptance Criteria

- Supported operators and their semantics are explicit.
- Overlap-aware operator application is defined.
- Blocking versus transformed outcomes are distinct.
- JSON validity preservation is part of the transform contract.
- Unsupported reversible and runtime-code operators remain excluded from core.
