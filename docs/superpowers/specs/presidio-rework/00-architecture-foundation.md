# Architecture Foundation

Date: 2026-04-05
Status: Draft

## Context

Presidio rework touches recognizers, operators, structured processing, explainability, evaluation, and future remote adapters.
Without a frozen architectural base, each later specification would redefine the analyzer pipeline, internal hit model, and safety boundaries differently.
This document exists to lock the minimum shared architecture before family-specific work begins.

## Goals

- Freeze the analyzer pipeline stages and their responsibilities.
- Freeze the shared internal contracts used by deterministic, structured, and future remote families.
- Define extension points for recognizers, processors, and evaluation runners.
- Define repository layout and artifact placement rules.
- Prevent later specs from changing no-raw-data and metadata-only invariants implicitly.

## Non-Goals

- Full recognizer family implementation.
- Full entity coverage.
- Remote recognizer implementations.
- OCR, image, DICOM, or PHI service implementations.
- Final corpus population or quality thresholds.
- Broad refactoring outside the contracts required for this roadmap.

## Core Invariants

- Detection, policy resolution, and transformation are separate stages.
- Final policy action is always owned by Pokrov core.
- Raw payload fragments must not be carried by explain or audit types.
- Non-blocking transformations must preserve valid JSON structure.
- Native and remote recognizers must converge into the same normalized hit model.
- Evaluation artifacts must be able to consume the same result contracts used by runtime flows.

## Pipeline Skeleton

### 1. Input normalization
Responsibility:
- Accept raw text or `serde_json::Value`.
- Derive language, path class, execution mode, and profile context.
- Produce a stable traversal view over text leaves and structured fields.

Must not:
- Apply policy decisions.
- Mutate payload content.

### 2. Recognizer execution
Responsibility:
- Execute active native recognizers.
- Optionally call active remote recognizer adapters.
- Emit candidate `NormalizedHit` values only.

Must not:
- Resolve overlaps.
- Choose final operators.
- Write explain or audit payloads.

### 3. Analysis, merge, and suppression
Responsibility:
- Merge candidate hits.
- Apply validation, suppression, allowlist effects, and overlap resolution.
- Produce stable `ResolvedHit` results.

Must not:
- Decide final policy profile action.
- Mutate payload content.

### 4. Policy resolution
Responsibility:
- Combine resolved hits with active profile rules.
- Decide final action and operator mapping.
- Produce a `TransformPlan`.

Must not:
- Directly mutate payload content.

### 5. Transform planning and application
Responsibility:
- Apply the approved transform plan.
- Preserve JSON validity for non-blocking outcomes.
- Produce `TransformResult`.

Must not:
- Re-run recognition logic.
- Recompute policy decisions.

### 6. Safe explain
Responsibility:
- Emit metadata-only reasons, provenance, and confidence buckets.
- Reuse resolved hits and policy outcome.

Must not:
- Carry raw snippets, matched fragments, or nearby source text.

### 7. Audit summary
Responsibility:
- Emit metadata-only audit facts.
- Record timing, path class, profile, and counts.

Must not:
- Log payload content or detection substrings.

## Frozen Internal Contracts

### `NormalizedHit`
Minimum fields:
- `entity_type` or `category`
- `location_kind`
- `start`
- `end`
- `json_pointer` or logical field path
- `score`
- `recognizer_id`
- `evidence_class`
- `reason_codes`
- `validation_status`
- `suppressed`
- `language`

Purpose:
- Single candidate detection shape for native and remote recognizers.

### `ResolvedHit`
Minimum fields:
- winning hit identity
- surviving span and field location
- effective score
- effective operator action hint
- suppressed competing recognizer ids
- stable precedence trace

Purpose:
- Single post-analysis hit shape for policy and explain.

### `TransformPlan`
Minimum fields:
- final policy action
- per-hit operator mapping
- transform order
- block versus transform mode

Purpose:
- Bridge between policy resolution and transformation.

### `TransformResult`
Minimum fields:
- final action
- blocked flag
- transformed payload or `None`
- transformed field count
- transform application metadata safe for evaluation

### `ExplainSummary`
Minimum fields:
- final action
- family and entity counts
- reason codes
- confidence buckets
- provenance summary
- degradation markers

### `AuditSummary`
Minimum fields:
- request id
- profile id
- mode
- final action
- category and family hit counts
- path class
- duration
- degradation metadata

### Evaluation placeholders
- `EvaluationCase`
- `EvaluationResult`
- `EvaluationReport`

These remain placeholders here but their existence is frozen so later specs can reuse runtime contracts directly.

## Extension Points

### `NativeRecognizer`
- Runs inside Pokrov process.
- Must obey deterministic ordering and latency constraints.
- Must emit only normalized hits.

### `RemoteRecognizer`
- Runs behind an adapter contract.
- Must return data normalized into the same hit model.
- Must never own final policy decisions.

### `StructuredProcessor`
- Reuses normalized hits and transform results.
- Adds field-aware semantics, not a separate detection model.

### `EvaluationRunner`
- Replays cases against runtime-compatible contracts.
- Must not require custom adapters per handler family.

### `BaselineRunner`
- Executes Presidio or another baseline and normalizes output for parity comparison.

## Allowed Dependencies

- Input normalization may be used by recognizer execution and structured processing only.
- Analysis depends on normalized hits, not raw recognizer internals.
- Policy depends on resolved hits, profile config, and execution mode.
- Transform depends on transform plan and original payload.
- Explain and audit depend on resolved hits, policy outcome, transform result, and degradation facts.
- Evaluation depends on the same public result contracts emitted by runtime flows.

## Repository Layout Rules

- Base decomposition specs live in `docs/superpowers/specs/presidio-rework/`.
- Public, repo-safe evaluation fixtures may live in a future `docs/superpowers/eval/` or `tests/fixtures/eval/` tree.
- Restricted datasets must never be committed to the repository.
- External dataset references must record access, license, redistribution constraints, and whether they are CI-safe.
- Future implementation plans and backlog documents should reference these specs by number.

## Acceptance Criteria

- Later specs can reference this document instead of redefining pipeline stages or shared types.
- The boundary between detection, analysis, policy, transform, explain, and audit is explicit.
- The metadata-only rule is enforced at the contract level.
- Evaluation work can target the same contracts used by runtime flows.
- Any later spec that changes these contracts must explicitly declare itself a foundation revision.
