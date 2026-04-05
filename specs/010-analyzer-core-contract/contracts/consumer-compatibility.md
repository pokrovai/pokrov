# Consumer Compatibility Contract: Analyzer Core

## Purpose

This document defines how downstream consumers must reuse the shared analyzer contract and what behavior is forbidden.

## Consumer Reuse Rules

### Evaluate HTTP Consumer

- May wrap the shared analyzer result in an HTTP response model.
- Must preserve policy-block versus analyzer-error semantics.
- Must not reinterpret block as transport failure.

### LLM Proxy Consumer

- Must reuse the shared analyzer request/result families before upstream proxy behavior.
- Must not fork a private decision model for chat-completions or streaming-specific enforcement.
- May expose adapter-specific response envelopes, but analyzer semantics stay shared.

### MCP Proxy Consumer

- Must reuse the same analyzer result family for tool argument and output sanitization decisions.
- Must not create a tool-specific block/error reinterpretation layer.

### Structured JSON Consumer

- Must reuse the same analyzer result surface after normalization.
- May enrich field-aware behavior through the shared unified location metadata, not through a private top-level result model.

### Evaluation Consumer

- Must reuse the same top-level analyzer result sections for replay and reporting.
- Must not require an evaluation-only result family to distinguish block, error, or degradation semantics.

## Policy Block Versus Analyzer Error

Required behavior:

- `block` is a valid analyzer result with full shared result sections.
- invalid input, invalid profile, and runtime failures are analyzer errors.
- degraded outcomes remain successful analyzer results when the analyzer can still return a safe policy outcome.
- missing required analyzer evidence under degradation defaults to fail-closed policy behavior recorded in metadata-safe output.
- no consumer may collapse those categories into one generic failure path.

## Executable Proof Requirement

The analyzer-core milestone is incomplete until executable evidence shows that:

- one runtime-oriented consumer path reuses the shared analyzer result family;
- one evaluation-oriented consumer path reuses the same family;
- policy-block outcomes do not require local special casing;
- degraded fail-closed outcomes do not require local special casing;
- metadata-only safety holds across those proofs.

Expected proof surface:

- contract tests for shared request/result construction;
- integration tests spanning evaluate and at least one runtime adapter;
- security tests that verify metadata-only boundaries;
- performance checks confirming the shared contract does not require duplicate conversion layers.

## Forbidden Consumer Behavior

Downstream work must not:

- introduce a private top-level analyzer result model for convenience;
- reinterpret policy block as analyzer failure;
- reinterpret degraded fail-closed outcomes as analyzer failure when a safe result exists;
- bypass the shared unified resolved-location model with consumer-specific text-only or field-only records;
- move degradation details into payload-bearing sections;
- expose raw payload fragments through explain, audit, executed, or degraded outputs;
- add consumer-local request variants that bypass the shared analyzer request contract.
