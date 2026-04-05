# Quickstart: Structured JSON Processing

## 1. Preconditions

- Checkout branch `014-structured-json-processing`.
- Ensure workspace config is valid and test environment is available.
- Use representative nested JSON fixtures (<=1 MB and >1 MB variants).

## 2. Implement in Small Steps

1. Add deterministic traversal and leaf context propagation for nested JSON.
2. Add path-binding precedence resolution according to contract order.
3. Apply transform only on string leaves while preserving payload shape.
4. Add size-mode handling (SLA mode <=1 MB, best-effort >1 MB).
5. Add fail-closed behavior for high-risk processing errors.
6. Add metadata-only structured explain/audit summaries with path-safe categories only.

## 3. Validate Locally

Run core checks:

```bash
cargo test
```

Run focused suites as available:

```bash
cargo test --test contract
cargo test --test integration
cargo test --test security
cargo test --test performance
```

## 4. Verify Acceptance Evidence

- Deterministic traversal across repeated runs for same payload+config.
- Precedence behavior proven for conflicting bindings.
- Structural preservation proven (only string leaves may change).
- No raw value leakage and no exact JSON pointer in explain/audit summaries.
- <=1 MB SLA-mode overhead target validated.
- >1 MB best-effort behavior validated with preserved safety invariants.

## 5. Prepare Next Step

After implementation-plan approval, generate executable tasks:

```bash
/speckit.tasks 014-structured-json-processing
```
