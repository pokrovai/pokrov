# Verification: 008 Baseline And Dataset Inventory

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/08-baseline-and-dataset-inventory.md`
Backlog source: `docs/superpowers/plans/presidio-rework/08-baseline-and-dataset-inventory-backlog.md`

## Implemented checkpoints

- Added canonical dataset inventory schema in `pokrov-core` with explicit access, license constraints, language/entity coverage, handler-family scope, repository safety, and execution scope.
- Registered baseline dataset sources:
  - `presidio-research`
  - `ai4privacy/pii-masking-200k`
  - `nvidia/Nemotron-PII`
  - `gretelai/gretel-pii-masking-en-v1`
  - `n2c2/i2b2` de-identification datasets
  - `Pseudo-PHI-DICOM-Data` (TCIA)
  - Pokrov internal de-identified corpus
  - optional hard-negative corpora
- Added baseline system catalog with requirement tiers:
  - mandatory deterministic: `Vanilla Presidio`, `Tuned Presidio`, Pokrov current native, Pokrov updated native
  - optional future workstreams: `NLM Scrubber`
- Added Phase 1A starter corpus definition with mandatory case groups and target volume:
  - 25-40 per priority deterministic family
  - 100 shared hard negatives
  - 50 structured JSON cases
  - 30 adversarial smoke cases
- Added baseline run matrix with reproducibility metadata requirements for parity stability.
- Added validation helpers and contract tests for:
  - dataset metadata completeness
  - starter corpus mandatory group coverage
  - baseline matrix reproducibility metadata and mandatory/optional split
- Added dataset-driven runtime replay test using repo-safe fixture:
  - `tests/fixtures/eval/datasets/phase_1a_starter.jsonl`
  - `starter_dataset_fixture_replays_through_runtime_engine`
- Added optional restricted-dataset manifest validation test (ignored in CI) for secured local environments:
  - `restricted_dataset_manifest_is_valid_when_provided`
  - manifest template: `tests/fixtures/eval/datasets/restricted-dataset-manifest.example.yaml`
- Added open-dataset autoload pipeline tests for four public sources (ignored in CI due network dependency):
  - `open_dataset_pipeline_autoloads_snapshots_for_all_sources`
  - `open_dataset_pipeline_replays_samples_through_runtime_engine`
- Added explicit downloader script used by tests and manual runs:
  - `scripts/eval/download_open_datasets.sh`

## Verification commands

- `cargo test -p pokrov-core`
- `cargo test --test contract sanitization_evaluation_lab_contract -- --nocapture`
- `cargo test --test contract sanitization_evaluation_lab_contract restricted_dataset_manifest_is_valid_when_provided -- --ignored --nocapture` (local secured env)
- `cargo test --test contract sanitization_open_dataset_pipeline_contract -- --ignored --nocapture`
- `bash scripts/eval/download_open_datasets.sh`

## Verification results (2026-04-05)

- `cargo test -p pokrov-core` -> PASS
- `cargo test --test contract sanitization_evaluation_lab_contract -- --nocapture` -> PASS
- `cargo test --test contract sanitization_open_dataset_pipeline_contract -- --ignored --nocapture` -> PASS
- `bash scripts/eval/download_open_datasets.sh` -> PASS

## Unresolved access constraints

- `n2c2/i2b2` access authorization and redistribution boundaries remain external governance tasks.
- TCIA `Pseudo-PHI-DICOM-Data` download and local storage controls remain outside repository fixture scope.
- Internal corpus handling remains limited to secured environments and metadata-only references in repository artifacts.
