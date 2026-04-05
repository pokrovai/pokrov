# Backlog: Baseline And Dataset Inventory

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/08-baseline-and-dataset-inventory.md`
Status: Draft

## Summary

This backlog turns the dataset and baseline inventory into a usable operational package for the Evaluation Lab.
Its purpose is to make every parity and quality report traceable to explicit corpus sources, access constraints, and required baseline runs.

## Scope

In scope:
- external dataset inventory
- internal corpus inventory
- metadata requirements per dataset
- Phase 1A starter corpus definition
- required baseline run list

Out of scope:
- full ingestion or mirroring of restricted datasets
- clinical benchmark execution details
- image benchmark implementation details

## Deliverables

- explicit dataset inventory records
- clear repo-safe versus restricted-only separation
- starter corpus definition concrete enough for first parity runs
- baseline run matrix covering Presidio and Pokrov comparisons

## Tasks

### Phase 1: Inventory records
- `B0801` Create the canonical inventory record format for datasets and baselines, including access, license, language, entity coverage, handler-family scope, and CI-safety.
- `B0802` Register the core sources: `presidio-research`, `n2c2/i2b2`, `Pseudo-PHI-DICOM-Data`, Pokrov internal corpus, and optional hard-negative corpora.
- `B0803` Register the comparative baseline systems: `Vanilla Presidio`, `Tuned Presidio`, Pokrov current native baseline, Pokrov updated native baseline, and optional `NLM Scrubber`.

### Phase 2: Starter corpus definition
- `B0804` Define the mandatory contents of the Phase 1A starter corpus: deterministic positives, deterministic negatives, context pairs, overlap and operator cases, structured JSON cases, and adversarial smoke cases.
- `B0805` Define target starter-corpus volume for first parity and regression runs.
- `B0806` Separate repo-safe starter cases from external or restricted benchmark references.

### Phase 3: Baseline run requirements
- `B0807` Define the minimum baseline run matrix required before parity reporting is treated as stable.
- `B0808` Define which baselines are mandatory for deterministic families and which remain optional for future PHI or image workstreams.
- `B0809` Ensure baseline run metadata is sufficient for reproducibility and comparison over time.

### Phase 4: Verification and evidence
- `B0810` Add checks or fixtures proving every dataset entry contains required metadata.
- `B0811` Add checks proving the starter corpus definition references all required case groups.
- `B0812` Record dataset-inventory verification evidence and unresolved access constraints.

## Dependencies

- Depends on `07-evaluation-lab-foundation-backlog.md`.
- Consumes entity scope from `06-en-ru-entity-packs-backlog.md`.
- Supports future parity reporting and release gating.

## Acceptance Evidence

Implementation is complete when:
- every dataset and baseline source is recorded with explicit constraints
- the starter corpus is concrete enough to seed first parity runs
- required baseline runs are explicit and reproducible
- repo-safe and restricted datasets are clearly separated

## Suggested Verification

- inventory-schema checks
- starter-corpus checklist verification
- review of baseline-run matrix against deterministic family scope
