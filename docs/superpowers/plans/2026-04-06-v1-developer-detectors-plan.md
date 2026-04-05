# V1 Developer Detectors Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first developer-focused detector wave to Pokrov while first proving that the detector architecture is extensible enough for URL, IP, phone, secret, and constrained name coverage.

**Architecture:** Start with a dedicated architecture-analysis gate for the detector layer before implementing any new recognizers. If the current split between built-in rules, deterministic metadata, validation, context scoring, and structured traversal is sufficient, proceed with narrow incremental detector additions; otherwise, perform one bounded refactor before adding families.

**Tech Stack:** Rust 1.85+, `pokrov-core`, `pokrov-config`, existing deterministic detection pipeline, contract tests in `tests/contract`, dataset-backed verification in `tests/common` and `docs/verification`

---

## Scope Source

- Product backlog: `docs/verification/015-v1-developer-detector-backlog.md`
- Gap report: `docs/verification/014-dataset-detector-gap-report.md`
- Deterministic baseline backlog: `docs/superpowers/plans/presidio-rework/02-deterministic-recognizers-backlog.md`

## File Map

**Detector architecture and runtime**
- Inspect: `crates/pokrov-core/src/detection/mod.rs`
- Inspect: `crates/pokrov-core/src/detection/deterministic/`
- Inspect: `crates/pokrov-core/src/lib.rs`
- Inspect: `crates/pokrov-core/src/traversal/mod.rs`
- Inspect: `crates/pokrov-core/src/types/foundation/entity_packs.rs`
- Inspect: `crates/pokrov-core/src/types/foundation/hit_families.rs`
- Possible modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible create: focused modules under `crates/pokrov-core/src/detection/` if the architecture-analysis phase finds the current file too rigid for expansion

**Configuration and recognizer metadata**
- Inspect: `crates/pokrov-config/src/validate.rs`
- Inspect: `tests/common/sanitization_deterministic_test_support.rs`

**Contract and dataset verification**
- Modify: `tests/common/sanitization_dataset_test_support.rs`
- Modify: `tests/common/sanitization_dataset_report_test_support.rs`
- Modify: `tests/contract/sanitization_open_dataset_pipeline_contract.rs`
- Modify: `tests/contract/sanitization_evaluation_lab_contract.rs`
- Possible create: focused detector-specific test helpers under `tests/common/`

**Documentation and evidence**
- Modify: `docs/verification/014-dataset-detector-gap-report.md`
- Modify: `docs/verification/015-v1-developer-detector-backlog.md`
- Create or modify: implementation evidence notes only if the detector architecture changes materially

## Chunk 1: Architecture Analysis Gate

Progress snapshot (implemented on branch `feature/v1-developer-detectors`):
- `4496dd7` secret token family expansion
- `bf9339d` URL + IPv4 runtime coverage
- `b1290e9` + `bf4ccc8` phone detection and RU phone format coverage
- `dd54670` constrained structured name-field coverage
- `59ea869` synthetic nested identity coverage + stable `id` hash replacement
- `e4c1cff` constrained `customer_id` / `account_number` / `swift_bic`

### Task 1: Audit detector extensibility before adding new families

**Files:**
- Inspect: `crates/pokrov-core/src/detection/mod.rs`
- Inspect: `crates/pokrov-core/src/detection/deterministic/`
- Inspect: `crates/pokrov-core/src/traversal/mod.rs`
- Inspect: `crates/pokrov-core/src/types/foundation/entity_packs.rs`
- Inspect: `docs/verification/014-dataset-detector-gap-report.md`
- Inspect: `docs/verification/015-v1-developer-detector-backlog.md`
- Output note in: `docs/verification/015-v1-developer-detector-backlog.md`

- [x] Review how built-in rules and deterministic recognizers are currently split.
- [x] Verify whether URL, IP, phone, and secret families can be expressed through the existing `pattern -> normalize -> validate -> context -> allowlist` pipeline without special cases.
- [x] Verify whether structured-field name coverage can be added without violating JSON-safe traversal invariants.
- [x] Check whether `crates/pokrov-core/src/detection/mod.rs` should stay as one file or be split before new families are added.
- [x] Record a decision: `proceed with current architecture` or `perform narrow refactor first`.

Run:
- `sed -n '1,260p' crates/pokrov-core/src/detection/mod.rs`
- `find crates/pokrov-core/src/detection -maxdepth 2 -type f | sort`
- `cargo test starter_dataset_fixture_replays_through_runtime_engine -- --nocapture`

Expected:
- A written architectural decision with explicit risks and file targets.

### Task 2: If needed, do the narrow architecture refactor first

**Files:**
- Modify only if Task 1 requires it: `crates/pokrov-core/src/detection/mod.rs`
- Possible create: focused submodules under `crates/pokrov-core/src/detection/`
- Test: existing unit and contract tests touching detection flow

- [x] Write a failing or missing-coverage test that proves the current structure blocks safe detector growth.
- [x] Apply the smallest refactor that improves extensibility without changing policy ownership or audit semantics.
- [x] Re-run detection unit tests and existing contract tests.
- [x] Update the architecture note in `docs/verification/015-v1-developer-detector-backlog.md` if the refactor changed the implementation path.

Run:
- `cargo test detect_payload --lib -- --nocapture`
- `cargo test starter_dataset_fixture_replays_through_runtime_engine -- --nocapture`

Expected:
- Existing detection behavior stays green and the architecture is ready for the detector wave.

## Chunk 2: Tier 1 Deterministic Wave

### Task 3: Secret and token family

**Files:**
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible modify: `crates/pokrov-core/src/detection/deterministic/validation.rs`
- Test: unit tests in `crates/pokrov-core/src/detection/mod.rs` or focused detector test modules
- Test: `tests/contract/sanitization_evaluation_lab_contract.rs`

- [x] Add failing tests for high-confidence secret and token patterns.
- [x] Implement the minimal recognizers and validators for secret assignment and token/header forms.
- [x] Add starter or contract cases proving `block` or `redact` behavior as required by the active profile.
- [x] Re-run focused unit and contract tests.

### Task 4: URL and IP family

**Files:**
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible modify: `crates/pokrov-core/src/detection/deterministic/validation.rs`
- Modify: `tests/common/sanitization_dataset_test_support.rs`
- Modify: `tests/contract/sanitization_open_dataset_pipeline_contract.rs`
- Modify: `tests/common/sanitization_dataset_report_test_support.rs`

- [x] Add failing detector tests for valid and invalid URL/IP candidates.
- [x] Implement validation-backed URL and IPv4 recognizers.
- [x] Promote dataset labels from mapped-only to runtime-covered only after the runtime behavior is real.
- [x] Add exact-output replay assertions for at least one cached URL row and one cached IPv4 row.
- [x] Re-render and verify `014-dataset-detector-gap-report.md`.

Run:
- `cargo test rows_match_expected --test contract -- --ignored --nocapture`
- `scripts/eval/render_dataset_detector_gap_report.sh 2026-04-06`
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture`

### Task 5: Phone family

**Files:**
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible modify: `crates/pokrov-core/src/detection/deterministic/context.rs`
- Modify: `tests/common/sanitization_dataset_test_support.rs`
- Modify: `tests/contract/sanitization_open_dataset_pipeline_contract.rs`

- [x] Add failing phone positives and adversarial negatives.
- [x] Implement a context-gated phone recognizer to limit accidental numeric matches.
- [x] Add exact-output replay assertions for one or more cached `phone_number` rows.
- [x] Verify no regression in card-like overlap behavior.

Run:
- `cargo test detect_payload --lib phone -- --nocapture`
- `cargo test rows_match_expected --test contract -- --ignored --nocapture`

## Chunk 3: Constrained Name Coverage

### Task 6: Structured name-field detection

**Files:**
- Inspect or modify: `crates/pokrov-core/src/traversal/mod.rs`
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible create: a focused field-aware helper under `crates/pokrov-core/src/detection/`
- Modify: `tests/contract/sanitization_evaluation_lab_contract.rs`
- Possible modify: `tests/common/sanitization_dataset_test_support.rs`

- [x] Add failing tests for `first_name`, `last_name`, and `middle_name` in structured JSON or tool-arg payloads.
- [x] Implement constrained field-aware detection without broad free-text name matching.
- [x] Add adversarial negatives proving package names, class names, or code identifiers are not redacted as person names.
- [x] Add starter-corpus fixtures if cached open datasets are too noisy for exact replay.

Run:
- `cargo test sanitization_evaluation_lab_contract -- --nocapture`

### Task 7: Explicit identity-context names

**Files:**
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible modify: `crates/pokrov-core/src/detection/deterministic/context.rs`
- Modify: `tests/contract/sanitization_evaluation_lab_contract.rs`

- [x] Add failing tests for explicit identity phrases such as `my name is`, `signed by`, and `author:`.
- [x] Implement the smallest context-bound name recognizer that satisfies those cases.
- [x] Add hard negatives for ordinary prose and code-like text.
- [x] Re-run focused unit and contract tests.

## Chunk 4: Remaining Constrained Deterministic Families

### Task 8: Address and contextual identifiers

**Files:**
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible modify: `crates/pokrov-core/src/detection/deterministic/validation.rs`
- Modify: `tests/common/sanitization_dataset_test_support.rs`
- Modify: `tests/contract/sanitization_open_dataset_pipeline_contract.rs`

- [x] Implement `en_address_like_high_risk` with explicit context and validation.
- [x] Implement `customer_id_contextual` and `account_number_contextual` with strict lexical context.
- [x] Add exact-output replay assertions only where dataset rows are stable enough.
- [x] Verify these detectors do not accidentally fire on common developer identifiers.

### Task 9: Structured financial and document identifiers

**Files:**
- Modify: `crates/pokrov-core/src/detection/mod.rs`
- Possible modify: `crates/pokrov-core/src/detection/deterministic/validation.rs`
- Modify: `tests/common/sanitization_dataset_test_support.rs`
- Modify: `tests/contract/sanitization_open_dataset_pipeline_contract.rs`

- [x] Implement `swift_bic` validation-backed detection.
- [x] Implement `medical_record_number_contextual` and `license_plate_contextual` with strict context.
- [x] Add unit and contract verification for positives and negatives.
- [x] Update dataset-backed report coverage accordingly.

## Chunk 5: Finish And Evidence

### Task 10: Align the report, backlog, and runtime evidence

**Files:**
- Modify: `tests/common/sanitization_dataset_test_support.rs`
- Modify: `tests/common/sanitization_dataset_report_test_support.rs`
- Modify: `docs/verification/014-dataset-detector-gap-report.md`
- Modify: `docs/verification/015-v1-developer-detector-backlog.md`

- [x] Ensure `supported_dataset_label_mapping()` only expresses taxonomy normalization.
- [x] Ensure `replay_assertable_dataset_labels()` reflects real runtime coverage only.
- [x] Re-render the gap report and confirm no section overstates support.
- [x] Record residual gaps that still require remote NER rather than deterministic growth.

Run:
- `scripts/eval/render_dataset_detector_gap_report.sh 2026-04-06`
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture`
- `cargo test partitioned_into_supported_or_backlog --test contract -- --ignored --nocapture`

### Task 11: Full verification pass for the completed wave

**Files:**
- No new files; verify all touched runtime, test, and doc files

- [x] Run focused unit tests for detection and validators.
- [x] Run starter-corpus contract tests.
- [x] Run ignored local dataset replay tests that cover the new families.
- [x] Only then claim the detector wave is complete.

Run:
- `cargo test detect_payload --lib -- --nocapture`
- `cargo test sanitization_evaluation_lab_contract -- --nocapture`
- `cargo test rows_match_expected --test contract -- --ignored --nocapture`
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture`

Expected:
- New deterministic coverage is implemented, verified, and honestly reflected in the report and backlog docs.
