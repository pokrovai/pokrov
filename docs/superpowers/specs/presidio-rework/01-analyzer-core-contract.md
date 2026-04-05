# Analyzer Core Contract

Date: 2026-04-05
Status: Draft

## Context

This specification defines the runtime contract for the Pokrov analyzer pipeline after the architectural foundation is frozen.
It exists to make LLM proxy, MCP mediation, structured JSON processing, evaluation runs, and future remote recognizers consume one stable analyzer result model.

## Goals

- Define analyzer inputs and outputs precisely.
- Fix the lifecycle from normalized input to resolved decision, transform result, explain payload, and audit summary.
- Separate policy-block errors from internal runtime errors.
- Ensure analyzer results are reusable by runtime paths and evaluation paths.

## Non-Goals

- Family-specific recognizer logic.
- Operator implementation details beyond the boundary between policy and transform.
- Remote service wire protocol details.

## Inputs

### Required inputs
- payload as raw text or `serde_json::Value`
- active profile id
- execution mode
- language
- path class
- request id or correlation id

### Optional inputs
- entity scope filter
- evaluation or dry-run flags
- recognizer-family include or exclude gates
- explicit allowlist additions allowed by policy

## Processing Stages

### Normalization
Output:
- normalized traversal view
- field path metadata
- stable language and path-class context

### Recognition
Output:
- list of `NormalizedHit`

### Analysis
Output:
- list of `ResolvedHit`
- suppression trace
- overlap resolution trace

### Policy resolution
Output:
- final action
- operator mapping
- `TransformPlan`

### Transformation
Output:
- `TransformResult`

### Explain and audit
Output:
- `ExplainSummary`
- `AuditSummary`

## Analyzer Result Shape

The analyzer contract must expose one top-level result containing:
- request and profile identity
- mode and path class
- resolved policy decision
- transform result
- safe explain payload
- safe audit summary
- execution and degradation metadata

Minimum top-level sections:
- `decision`
- `transform`
- `explain`
- `audit`
- `executed`
- `degraded`

## Decision Contract

The decision section must include:
- final action
- total hit count
- counts by entity family or category
- resolved spans or field locations
- deterministic signature or equivalent stable replay identity

The decision section must not include:
- raw matched values
- raw excerpts
- debug-only internal objects

## Error Contract

### Policy-block outcome
- Represented as a successful analyzer result with final action `block`.
- Includes safe explain and audit.
- Not treated as an internal error.

### Invalid input or invalid profile
- Represented as analyzer errors.
- Must not be conflated with policy blocks.

### Runtime failure
- Represented as analyzer errors with enough metadata for operational debugging.
- Must remain compatible with metadata-only safety rules.

## Determinism Rules

- Identical input, profile, language, mode, and recognizer set must produce identical resolved hits and final action.
- Deterministic ordering must be explicit for same-span and same-score collisions.
- The same analyzer result shape must be emitted for LLM, MCP, structured, and evaluation flows.

## Compatibility Rules

- Native and remote recognizers feed the same normalized hit model.
- Structured JSON uses the same analyzer result shape as plain text after normalization.
- Evaluation runners consume the same top-level result sections used by runtime flows.

## Acceptance Criteria

- Analyzer input requirements are explicit and sufficient for all planned runtime consumers.
- Top-level result sections are stable and reusable.
- Policy-block semantics are separate from internal failures.
- Deterministic replay behavior is defined.
- Explain and audit sections remain metadata-only by contract.
