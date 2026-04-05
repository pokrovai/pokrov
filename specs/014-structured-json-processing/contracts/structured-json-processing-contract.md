# Contract: Structured JSON Processing

## 1. Scope

This contract defines inline structured JSON behavior for deterministic traversal, path-aware policy resolution, transformation constraints, and metadata-only explain/audit summaries.

## 2. Input Contract

- Input is a valid JSON object or array with nested values.
- Only string leaves are valid recognizer inputs.
- Each string leaf is evaluated with path-context metadata.

## 3. Traversal Contract

- Traversal MUST be deterministic for identical payload and config.
- Objects and arrays MUST be traversed in stable order.
- Empty strings are valid leaves and MUST be processed.
- Non-string leaves MUST remain structurally and semantically unchanged.

## 4. Binding and Precedence Contract

- Binding targets: exact pointer, logical field alias, subtree default.
- Precedence MUST be applied in fixed order:
  1. exact pointer override
  2. logical alias override
  3. subtree default
  4. profile default
  5. global default
- Equal precedence conflicts MUST resolve deterministically.

## 5. Transformation Contract

- Only string leaves MAY be transformed.
- Output JSON MUST preserve input structure (objects/arrays/position semantics).
- Identical string values at different paths MAY resolve to different actions when bindings differ.

## 6. Size and Failure Policy Contract

- Payload <=1 MB: processing is in SLA mode (p95 sanitization+proxy overhead <=50 ms target).
- Payload >1 MB: processing continues in best-effort mode; latency SLA is not guaranteed.
- High-risk processing failures MUST produce fail-closed block decisions.

## 7. Explain/Audit Safety Contract

- Explain/audit summaries MUST be metadata-only.
- Summaries MUST include safe counts/categories/path classes only.
- Summaries MUST NOT include raw values, raw snippets, or exact JSON pointer.

## 8. Shared Contract Reuse

- Structured mode MUST reuse shared normalized hit/decision/transform contract family used by plain-text flows.
- Any extension MUST preserve backward compatibility for existing runtime consumers.

## 9. Verification Contract

Minimum evidence required:
- unit tests for traversal determinism and precedence ordering
- integration tests for nested payloads with path-specific overrides
- security tests verifying zero raw leakage and no exact pointer in summaries
- performance checks for <=1 MB SLA-mode payloads
