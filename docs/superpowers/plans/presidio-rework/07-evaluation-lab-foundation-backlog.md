# Backlog: Evaluation Lab Foundation

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/07-evaluation-lab-foundation.md`
Status: Draft

## Summary

This backlog establishes the first-class evaluation subsystem for the Presidio rework.
Its purpose is to turn parity, quality, safety, and rollout readiness into measurable artifacts instead of informal judgment.

## Scope

In scope:
- evaluation case schema
- report and scoreboard schema
- synthetic, curated, and adversarial corpus roles
- metric groups
- progressive quality-gate model

Out of scope:
- full dataset population
- final per-entity thresholds
- clinical and image benchmark execution details

## Deliverables

- stable `EvaluationCase`, `EvaluationResult`, `EvaluationReport`, `ParityReport`, and `ReadinessScoreboard` shapes
- common runner assumptions for runtime-compatible evaluation
- metric definitions for detection, parity, security, runtime, and transformation quality
- progressive quality-gate stages

## Tasks

### Phase 1: Case and report model
- `V0701` Define the shared evaluation case schema with stable required fields and allowed modes.
- `V0702` Define report schemas for evaluation summaries, parity reports, and readiness scoreboards.
- `V0703` Ensure evaluation results can consume runtime-compatible analyzer and transform outputs without per-family adapters.

### Phase 2: Corpus model and responsibilities
- `V0704` Freeze the purpose and minimum required contents of the synthetic corpus.
- `V0705` Freeze the purpose and minimum required contents of the curated gold corpus.
- `V0706` Freeze the purpose and minimum required contents of the adversarial corpus.

### Phase 3: Metrics and gates
- `V0707` Encode the detection, parity, security, runtime, and transformation metric groups as stable report dimensions.
- `V0708` Define the progressive quality-gate levels and which report outputs they depend on.
- `V0709` Ensure gates can evolve in thresholds without changing report schemas.

### Phase 4: Verification and evidence
- `V0710` Add tests or fixtures proving evaluation cases can be replayed against runtime-compatible contracts.
- `V0711` Add schema checks for report generation and scoreboard generation.
- `V0712` Record evaluation-foundation verification evidence and known deferred gaps.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md`, `01-analyzer-core-contract-backlog.md`, and `04-safe-explainability-and-audit-backlog.md`.
- Consumes results from `02-06` but does not require their full feature completion to define the evaluation model.
- Supports `08-baseline-and-dataset-inventory` and `09-remote-recognizer-contract`.

## Acceptance Evidence

Implementation is complete when:
- evaluation and report schemas are explicit and stable
- the three corpus types and their responsibilities are fixed
- metric groups are frozen for later thresholding
- progressive gates can be applied without redefining the schema model

## Suggested Verification

- schema tests for evaluation cases and reports
- replay tests using analyzer-compatible outputs
- dry-run scoreboard generation for a minimal synthetic sample
