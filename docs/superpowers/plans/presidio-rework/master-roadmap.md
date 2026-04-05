# Presidio Rework Master Roadmap

Date: 2026-04-05
Status: In Progress

## Purpose

This document turns the numbered Presidio rework specs and backlogs into one management roadmap.
Its purpose is to make execution order, dependency gates, readiness criteria, and milestone boundaries explicit before implementation starts.

## Planning Inputs

Specs:
- `docs/superpowers/specs/presidio-rework/00-architecture-foundation.md`
- `docs/superpowers/specs/presidio-rework/01-analyzer-core-contract.md`
- `docs/superpowers/specs/presidio-rework/02-deterministic-recognizers.md`
- `docs/superpowers/specs/presidio-rework/03-operator-semantics.md`
- `docs/superpowers/specs/presidio-rework/04-safe-explainability-and-audit.md`
- `docs/superpowers/specs/presidio-rework/05-structured-json-processing.md`
- `docs/superpowers/specs/presidio-rework/06-en-ru-entity-packs.md`
- `docs/superpowers/specs/presidio-rework/07-evaluation-lab-foundation.md`
- `docs/superpowers/specs/presidio-rework/08-baseline-and-dataset-inventory.md`
- `docs/superpowers/specs/presidio-rework/09-remote-recognizer-contract.md`

Backlogs:
- `docs/superpowers/plans/presidio-rework/00-architecture-foundation-backlog.md`
- `docs/superpowers/plans/presidio-rework/01-analyzer-core-contract-backlog.md`
- `docs/superpowers/plans/presidio-rework/02-deterministic-recognizers-backlog.md`
- `docs/superpowers/plans/presidio-rework/03-operator-semantics-backlog.md`
- `docs/superpowers/plans/presidio-rework/04-safe-explainability-and-audit-backlog.md`
- `docs/superpowers/plans/presidio-rework/05-structured-json-processing-backlog.md`
- `docs/superpowers/plans/presidio-rework/06-en-ru-entity-packs-backlog.md`
- `docs/superpowers/plans/presidio-rework/07-evaluation-lab-foundation-backlog.md`
- `docs/superpowers/plans/presidio-rework/08-baseline-and-dataset-inventory-backlog.md`
- `docs/superpowers/plans/presidio-rework/09-remote-recognizer-contract-backlog.md`

## Delivery Principles

- Preserve Pokrov ownership of policy, transformation, safe explain, and metadata-only audit.
- Build contracts before behavior, behavior before coverage, and coverage before extensions.
- Treat evaluation as a release-control subsystem, not as optional post-factum testing.
- Avoid starting downstream work before upstream contracts are implemented or frozen strongly enough that they will not be redefined.
- Keep native deterministic paths first-class; do not let remote recognizers drive core design.

## Workstreams

- `WS0 Foundation`: architecture skeleton and frozen internal contracts.
- `WS1 Analyzer Core`: analyzer-facing input/output contracts and top-level result model.
- `WS2 Deterministic Families`: pattern, checksum, context, denylist, allowlist behavior.
- `WS3 Operators`: replace, redact, mask, hash, keep, and overlap-aware application order.
- `WS4 Safe Explain And Audit`: reason codes, provenance, confidence buckets, metadata-only outputs.
- `WS5 Structured JSON`: field-aware traversal and path-bound behavior for inline JSON payloads.
- `WS6 EN/RU Entity Packs`: initial language/entity coverage and default operator mapping.
- `WS7 Evaluation Lab`: evaluation schema, reports, corpora roles, and quality gates.
- `WS8 Dataset And Baseline Inventory`: corpus sources, starter corpus, and parity baseline matrix.
- `WS9 Remote Recognizer Contract`: remote adapter boundary, degradation model, and normalization rules.

## Milestones

### Milestone M0: Foundation Frozen

Scope:
- `WS0`
- `WS1`

Primary goal:
- Freeze the shared architecture and analyzer contracts so later work adds capability without redefining core types or stage ownership.

Entry criteria:
- current Pokrov sanitization flow is understood well enough to map onto the target pipeline
- no unresolved question remains about crate boundaries for `pokrov-core`, proxy crates, and config ownership

Exit criteria:
- stage boundaries are explicit
- shared internal result shapes exist or are frozen strongly enough for implementation
- no-raw-data constraints are encoded in explain/audit-facing contracts
- downstream deterministic, structured, evaluation, and remote specs can reference the same contracts without redefining them

Deliverables:
- stable architecture foundation
- stable analyzer core contract
- verification note for shared contract viability

Implementation status:
- `009-architecture-foundation` now exports the frozen stage map and shared contract scaffolding from `pokrov-core`
- one executable runtime/evaluation proof is implemented and must remain green before `WS1-WS9` changes land

Blocking dependencies:
- none beyond the current workspace

### Milestone M1: Deterministic Text Engine Ready

Scope:
- `WS2`
- `WS3`
- `WS4`

Primary goal:
- Make the native analyzer/anonymizer path behaviorally explicit and safe for deterministic families.

Entry criteria:
- `M0` complete or frozen strongly enough that no downstream work will redefine hit, transform, explain, or audit contracts

Exit criteria:
- deterministic recognizer scoring and precedence rules are frozen
- supported operator semantics are frozen
- explain and audit outputs can represent deterministic decisions without raw data leakage
- deterministic-family implementation can begin without unresolved contract questions

Deliverables:
- deterministic recognizer behavior model
- operator semantics model
- safe explain and audit model
- verification approach for deterministic-family correctness and safety

Blocking dependencies:
- `M0`

### Milestone M2: Structured And Coverage Layer Ready

Scope:
- `WS5`
- `WS6`

Primary goal:
- Bind the deterministic engine to real JSON payload semantics and to the initial EN/RU entity surface.

Entry criteria:
- `M1` complete or frozen strongly enough that structured traversal and entity mapping do not need to redefine recognizer/operator behavior

Exit criteria:
- field-aware inline JSON semantics are frozen
- path binding and traversal precedence are frozen
- Phase 1 EN/RU entity coverage is explicit
- entity-to-recognizer and entity-to-operator defaults are explicit enough to drive implementation and evaluation

Deliverables:
- structured JSON processing model
- EN/RU entity-pack definition
- initial coverage boundary for implementation and evaluation

Blocking dependencies:
- `M1`

### Milestone M3: Evaluation Readiness Frozen

Scope:
- `WS7`
- `WS8`

Primary goal:
- Convert parity and quality goals into measurable artifacts before larger-scale implementation and release claims.

Entry criteria:
- `M2` complete or frozen strongly enough that evaluation schemas can target stable entities, payload modes, and operator outcomes

Exit criteria:
- evaluation case and report schemas are frozen
- synthetic, curated, and adversarial corpus roles are frozen
- starter corpus definition exists
- baseline inventory and required parity runs are explicit
- quality gates are defined well enough to support later release decisions

Deliverables:
- evaluation lab foundation
- dataset and baseline inventory
- starter corpus and baseline-run management rules

Blocking dependencies:
- `M0`
- `M1`
- `M2`

### Milestone M4: Extension Boundary Frozen

Scope:
- `WS9`

Primary goal:
- Open a safe extension boundary for remote recognizers without allowing them to redefine core runtime behavior.

Entry criteria:
- `M0` complete
- `M1` complete
- `M3` complete or frozen strongly enough that remote degradation and remote quality can be evaluated through the common evidence model

Exit criteria:
- remote adapter contract is explicit
- remote degradation semantics are explicit and fail-closed by default
- remote hits normalize into the shared hit model
- evaluation can distinguish remote quality failures from remote availability failures

Deliverables:
- remote recognizer contract
- degradation and observability model for remote execution
- evaluation compatibility for remote runs

Blocking dependencies:
- `M0`
- `M1`
- `M3`

## Recommended Execution Order

### Stage A: Contract Foundation
- implement or freeze `00`
- implement or freeze `01`
- stop only when downstream specs no longer need to redefine core types or stage ownership

### Stage B: Native Deterministic Behavior
- implement or freeze `02`
- implement or freeze `03`
- implement or freeze `04`
- stop only when deterministic detection, transformation, and safe explain paths are behaviorally explicit

### Stage C: Structured Payloads And Coverage
- implement or freeze `05`
- implement or freeze `06`
- stop only when inline JSON behavior and EN/RU Phase 1 coverage are explicit enough for evaluation and rollout planning

### Stage D: Evidence And Baselines
- implement or freeze `07`
- implement or freeze `08`
- stop only when parity, quality, and leakage claims can be backed by stable artifacts and dataset inventory

### Stage E: Extension Boundary
- implement or freeze `09`
- stop only when remote recognizers can be introduced without weakening policy ownership or audit safety

## Dependency Gates

### Gate G0: Foundation Gate
Required before starting implementation-heavy work in `02-09`.

Gate conditions:
- shared hit, transform, explain, and audit contracts are frozen
- stage ownership is explicit
- metadata-only audit and explain invariants are encoded in contract design

### Gate G1: Deterministic Behavior Gate
Required before structured and coverage work in `05-06`.

Gate conditions:
- deterministic scoring and suppression precedence are explicit
- supported operator behaviors are explicit
- no unresolved conflict remains between recognizer resolution and transform application order
- explain and audit can represent deterministic outcomes safely

### Gate G2: Coverage Gate
Required before full evaluation and baseline planning in `07-08`.

Gate conditions:
- inline JSON semantics are explicit
- EN/RU Phase 1 entity scope is explicit
- expected operator outcomes for covered entities are explicit enough for corpus labeling

### Gate G3: Evidence Gate
Required before remote-contract rollout and before release-style quality claims.

Gate conditions:
- evaluation case schema and report schema are frozen
- corpus roles are explicit
- starter corpus exists conceptually and operationally
- baseline run matrix is explicit

## Parallelization Guidance

Safe parallel work after `M0`:
- `WS2`, `WS3`, and `WS4` can advance in parallel if they do not redefine shared contracts

Safe parallel work after `M1`:
- `WS5` and `WS6` can advance in parallel with coordination on entity/path semantics

Safe parallel work after `M2`:
- `WS7` and `WS8` can advance in parallel because one defines evidence structure and the other populates evidence sources

`WS9` should remain late-bound:
- it may begin contract drafting earlier
- it should not drive implementation sequencing ahead of native deterministic and evaluation readiness work

## Readiness Model

### Spec Readiness
A spec area is ready for implementation when:
- scope and non-goals are explicit
- upstream dependencies are frozen strongly enough
- acceptance evidence is measurable
- no hidden contract changes remain

### Implementation Readiness
A milestone is ready for active coding when:
- entry criteria are satisfied
- upstream gate is passed
- verification method is known
- affected crates and ownership boundaries are explicit

### Release Readiness For The Rework
The Presidio rework is ready for release-style claims only when:
- native deterministic families are implemented against frozen contracts
- structured JSON semantics are implemented for covered modes
- EN/RU Phase 1 coverage is implemented and measured
- evaluation lab artifacts produce repeatable reports
- baseline and dataset inventory support parity tracking
- any remote-recognizer claims are backed by the explicit remote contract and degradation model

## Risks And Management Notes

- The main delivery risk is starting entity or recognizer implementation before `M0-M1` are stable enough; that would force repeated contract churn.
- The main quality risk is treating Presidio parity as ground truth instead of using it as comparative evidence beside gold datasets.
- The main scope risk is pulling remote recognizers or PHI/image work forward before native deterministic and evidence layers are stable.
- The main security risk is allowing explain, audit, evaluation fixtures, or remote adapters to carry raw payload fragments.
- The main execution risk is mixing backlog refinement with implementation without milestone-level stop conditions.

## Management Recommendation

Use this roadmap as the top-level execution controller.
For implementation planning and issue generation:
- refine only `M0` first
- then refine `M1`
- do not open `M2-M4` implementation work until the preceding gate is explicitly passed or consciously frozen

## Update 2026-04-05: 010 Analyzer Core Contract

- Canonical analyzer request/result contracts were implemented in `pokrov-core` for shared runtime and evaluation consumers.
- The shared result sections include `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded`.
- Adapter integrations in evaluate/LLM/MCP paths now consume the same core request contract shape.
