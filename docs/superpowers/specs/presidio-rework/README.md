# Presidio Rework Spec Pack

Date: 2026-04-05
Status: Draft index

## Purpose

This directory contains the base specification decomposition for the Presidio rework in Pokrov.
The package is intentionally split into small decision areas so implementation backlog can be derived from stable contracts instead of broad feature ideas.

## Spec Order

1. `00-architecture-foundation.md`
2. `01-analyzer-core-contract.md`
3. `02-deterministic-recognizers.md`
4. `03-operator-semantics.md`
5. `04-safe-explainability-and-audit.md`
6. `05-structured-json-processing.md`
7. `06-en-ru-entity-packs.md`
8. `07-evaluation-lab-foundation.md`
9. `08-baseline-and-dataset-inventory.md`
10. `09-remote-recognizer-contract.md`

## Dependency Order

- `00` is the foundation and freezes shared contracts.
- `01` depends on `00`.
- `02`, `03`, `04`, and `05` depend on `00` and `01`.
- `06` depends on `02` and `03`.
- `07` depends on `00`, `01`, `02`, `03`, `04`, and `05`.
- `08` depends on `07`.
- `09` depends on `00`, `01`, `04`, and `07`.

## Readiness Criteria

A spec is ready for backlog decomposition when all of the following are true:
- its scope is a single decision area rather than a mixed feature bundle
- goals and non-goals are explicit
- contracts, invariants, and boundaries are written down
- downstream specs can reference it instead of redefining the same decision
- acceptance criteria are concrete enough to derive implementation and verification tasks

## Recommended Backlog Derivation Order

1. Foundation and shared contracts
   - `00`
   - `01`
2. Deterministic text behavior
   - `02`
   - `03`
   - `04`
3. Structured behavior and language coverage
   - `05`
   - `06`
4. Evaluation and evidence
   - `07`
   - `08`
5. External extension points
   - `09`

## Implementation Notes

- Do not build backlog directly from this README.
- Build backlog from the numbered specs, using this file only for ordering and dependency checks.
- If a later spec needs to change a frozen contract from `00`, treat that as a foundation revision rather than a normal downstream refinement.
