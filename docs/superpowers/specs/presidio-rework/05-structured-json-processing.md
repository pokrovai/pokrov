# Structured JSON Processing

Date: 2026-04-05
Status: Draft

## Context

Pokrov already operates on JSON payloads, but future Presidio-style parity cannot treat structured inputs as a generic bag of string leaves.
This specification defines inline processing of structured and semi-structured JSON payloads with field-aware semantics while preserving the shared analyzer and transform contracts.

## Goals

- Define deterministic traversal over nested JSON.
- Preserve field and path semantics required for policy and evaluation.
- Reuse the same hit, decision, and transform contracts used by plain text flows.
- Keep inline structured processing distinct from future batch tabular processing.

## Non-Goals

- CSV or SQL-style batch processors.
- Column inference for tabular data outside JSON.
- Image, OCR, or document-layout extraction.

## Input Model

Structured input is any `serde_json::Value` containing strings embedded in objects or arrays.
Each string leaf must be processed together with:
- JSON pointer
- logical field path if configured
- parent structural context
- path class
- active profile and language

## Traversal Rules

- Objects are traversed in deterministic key order.
- Arrays are traversed in index order.
- Only string leaves are direct recognizer inputs.
- Non-string leaves remain unchanged and still contribute to field context when needed.
- Empty strings are valid leaves and must not break traversal.

## Field-Aware Semantics

Structured processing must support:
- path-specific recognizer-family inclusion
- path-specific recognizer-family exclusion
- path-specific operator defaults
- high-risk field overrides
- safe field classification metadata for evaluation and reporting

Field bindings may target:
- exact JSON pointer
- logical field aliases
- object subtrees where explicitly configured

## Required Behavior

### Inline detection
- Each string leaf is analyzed as text, but results retain field path metadata.
- Two identical string leaves in different paths may produce different outcomes if field bindings differ.

### Inline transformation
- Transform only string leaves.
- Preserve object and array shape exactly.
- Preserve ordering semantics used by the runtime serializer.

### Structured summaries
- Explain and audit may include path-safe counts and field-safe categories.
- They must not include raw values or field contents.

## Path Binding Precedence

Path-level rules must apply in this order:
1. exact pointer overrides
2. logical field alias overrides
3. subtree defaults
4. profile defaults
5. global engine defaults

## Relationship To Batch Structured Work

Inline structured JSON processing is in scope now.
Future batch processing for tabular or SQL-style datasets remains a separate execution mode but must reuse:
- the same normalized hit contract
- the same transform result concepts
- the same explain and audit safety model

## Acceptance Criteria

- Traversal over nested objects and arrays is deterministic.
- Path binding precedence is explicit.
- Non-string leaves are preserved.
- Field-aware summaries can be emitted without leaking values.
- Structured JSON uses the same core hit and transform contracts as plain text.
