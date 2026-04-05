# Presidio Rework Backlog Pack

Date: 2026-04-05
Status: Draft

## Purpose

This directory contains implementation backlogs derived from the numbered Presidio rework specs.
Backlog decomposition starts with the foundation layer only and expands outward by dependency order.

## Current Scope

- `00-architecture-foundation-backlog.md`
- `01-analyzer-core-contract-backlog.md`
- `02-deterministic-recognizers-backlog.md`
- `03-operator-semantics-backlog.md`
- `04-safe-explainability-and-audit-backlog.md`
- `05-structured-json-processing-backlog.md`
- `06-en-ru-entity-packs-backlog.md`
- `07-evaluation-lab-foundation-backlog.md`
- `08-baseline-and-dataset-inventory-backlog.md`
- `09-remote-recognizer-contract-backlog.md`

## Sequencing Rule

- Finish `00` before starting implementation work for `01`.
- Start `02-04` only after `00` and `01` contracts are implemented or at least frozen strongly enough that downstream tasks do not redefine them.
- Start `05-06` only after `02-04` behavior is implemented or frozen strongly enough for structured and language coverage work.
- Start `07-09` only after `05-06` scope is implemented or frozen strongly enough for evaluation and extension work.

## Acceptance Rule

Each backlog document must be implementable independently and must reference only already-frozen upstream specs.
