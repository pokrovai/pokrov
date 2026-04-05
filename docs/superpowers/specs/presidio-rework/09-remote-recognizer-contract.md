# Remote Recognizer Contract

Date: 2026-04-05
Status: Draft

## Context

Some Presidio families depend on heavy NLP, PHI, or cloud services that should not live inside the Pokrov hot path as native runtime dependencies.
This specification defines the adapter contract for remote recognizers so they can participate in the analyzer flow without changing Pokrov's ownership of policy and safety.

## Goals

- Define the adapter boundary for external recognizers.
- Keep remote recognizers compatible with the same normalized hit model as native families.
- Freeze timeout, degradation, and fail-closed rules.
- Ensure audit and explain remain metadata-only even when remote services are involved.

## Non-Goals

- A concrete HTTP or gRPC wire protocol.
- Vendor-specific cloud connector details.
- Remote operator contracts such as surrogate generation.

## Inputs

Remote recognizers receive:
- text or JSON leaf payload
- language
- entity scope
- timeout budget
- request or correlation metadata needed for observability
- profile-derived routing metadata if allowed by policy

## Outputs

Remote recognizers must return enough data to populate normalized hits:
- entity or category
- location
- score or confidence
- recognizer id
- evidence class
- validation or model status if safe to expose
- degradation metadata when applicable
- explicit failure mode information

## Core Rules

- Final policy decision always remains in Pokrov.
- Remote hits must normalize into the same hit model as native recognizers.
- Remote recognizers may propose evidence, not final action.
- Default degradation mode is fail-closed.
- Timeout and service failures must be visible in safe audit and explain metadata.

## Degradation Semantics

### Success
- normal normalized hits are returned
- degradation flag is false

### Empty but valid response
- valid no-hit result
- not treated as degradation

### Timeout or transport failure
- treated as degradation
- fail-closed unless profile explicitly allows a more permissive mode in future revisions

### Partial or malformed response
- treated as degradation
- do not forward malformed evidence into analysis

## Safety Requirements

- Raw remote payload echoes must never appear in explain or audit outputs.
- Remote adapters must not bypass metadata-only restrictions.
- Remote service identity may appear only as safe provenance metadata.

## Evaluation Compatibility

Remote recognizer runs must be reportable by the same evaluation framework used for native recognizers.
The evaluation system must be able to distinguish:
- remote quality issues
- remote degradation issues
- policy outcomes caused by fail-closed behavior

## Acceptance Criteria

- Remote recognizers can be integrated without changing core result contracts.
- Degradation behavior is deterministic and testable.
- Fail-closed is explicit as the default.
- Remote adapters preserve the metadata-only safety model.
