# Internal Contract Surface: Analyzer Core Contract

## Purpose

This document defines the shared analyzer request/result surface that runtime adapters and evaluation consumers must reuse.
It is not a new public external API specification. It is the internal contract boundary for the analyzer milestone.

## Canonical Request Contract

The shared analyzer request surface must carry:

- payload as raw text or structured `serde_json::Value`
- request identity and optional correlation identity
- profile identity
- execution mode
- path class
- effective language
- optional entity-scope filters
- optional recognizer-family gates
- optional policy-allowed allowlist additions

Rules:

- adapter layers may inject a deterministic configured default for effective language when the caller does not provide one, but they must not omit effective language from the shared request contract;
- adapters may provide defaults for optional fields, but they must not create local request-only variants;
- request identity is always required;
- the request contract is owned by `pokrov-core` and reused across evaluate, LLM, MCP, structured, and evaluation flows.

## Canonical Result Surface

Every successful analyzer completion must expose the same top-level sections:

- `decision`
- `transform`
- `explain`
- `audit`
- `executed`
- `degraded`

Rules:

- the same section family is used for allow, mask, replace, redact, and block outcomes;
- degraded outcomes remain successful analyzer results when a safe policy outcome exists, and fail-closed handling is recorded in `degraded` metadata when required evidence is missing;
- consumers must not introduce private top-level result wrappers to reinterpret policy-block outcomes;
- serialized reuse is allowed, but it must come from this shared surface rather than a competing contract family.

## Decision Contract

The decision section must provide:

- final action
- total hit count
- family/category counts
- one unified resolved-location record family for text or structured inputs
- stable replay identity

The decision section must not provide:

- raw matched values
- raw excerpts
- debug-only internal objects
- copied leaf values from the payload

## Transform Contract

The transform section owns the only payload-bearing portion of a successful analyzer result.

Rules:

- sanitized payload is present only for non-blocking outcomes;
- block outcomes expose `blocked=true` and no sanitized payload;
- transform metadata may describe what happened, but not by copying raw source content.

## Explain, Audit, Executed, And Degraded Safety Rules

These sections are metadata-only.

They may include:

- counts
- enums
- identifiers
- path class
- profile identity
- effective language only where the shared contract requires it
- timing
- degradation reasons
- execution-stage facts

They must not include:

- raw payload text
- matched substrings
- nearby source snippets
- copied structured leaf values

Additional rule:

- degraded sections must record fail-closed handling in metadata-safe form when required analyzer evidence is unavailable.

## Ownership And Export Boundary

The shared analyzer contract is expected to remain compile-visible from `pokrov-core` and reused by:

- `pokrov-api` evaluate handling
- `pokrov-proxy-llm`
- `pokrov-proxy-mcp`
- structured JSON and evaluation consumers

Current related surfaces already live in:

- `crates/pokrov-core/src/types.rs`
- `crates/pokrov-core/src/types/foundation/`
- `crates/pokrov-core/src/lib.rs`
- adapter-local wrappers in `crates/pokrov-api` and proxy crates
