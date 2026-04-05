# Baseline And Dataset Inventory

Date: 2026-04-05
Status: Draft

## Context

The Evaluation Lab requires a concrete inventory of external datasets, internal corpora, and baseline systems so parity and quality evidence are traceable and reproducible.
This specification records those sources and the constraints on how they may be used.

## Goals

- Record baseline datasets and systems explicitly.
- Separate repo-safe artifacts from restricted external datasets.
- Define the minimum starter corpus for Phase 1A.
- Define the baseline runs that must exist before parity reporting is treated as meaningful.

## Dataset Inventory

### `presidio-research`
- role: workflow scaffold for synthetic generation, split strategy, exploratory analysis, and parity reporting
- access: open tooling
- main language: EN first, extensible
- intended families: deterministic text families, operators, evaluation workflows

### `n2c2 / i2b2` de-identification datasets
- role: restricted clinical-text benchmark
- access: restricted
- main language: EN
- intended families: future PHI and remote-recognizer evaluation
- repository rule: never committed as plain fixtures

### `Pseudo-PHI-DICOM-Data` from TCIA
- role: future DICOM and image benchmark
- access: external dataset download
- main language: medical-image text and metadata contexts
- intended families: future OCR, image, and DICOM workstreams

### `Pokrov internal de-identified corpus`
- role: proxy-specific benchmark
- access: internal only
- main languages: EN and RU
- intended families: prompts, tools, outputs, structured JSON, adversarial cases

### Optional hard-negative corpora
- role: false-positive evaluation for deterministic families
- access: varies by source
- main languages: EN and RU if curated internally
- intended families: deterministic families and allowlist behavior

## Required Metadata Per Dataset

Every dataset record must include:
- access model
- license and redistribution limits
- language coverage
- entity coverage
- intended handler families
- repo-safe versus restricted-only status
- CI-safe versus local-only status

## Baseline Systems

### Required comparative baselines
- `Vanilla Presidio`
- `Tuned Presidio`
- Pokrov current native pipeline
- Pokrov updated native pipeline

### Optional comparative baseline
- `NLM Scrubber` for PHI-oriented workstreams

## Phase 1A Starter Corpus

The starter corpus must include:
- deterministic positives for email, phone, card-like numbers, IBAN, IP, URL, secret tokens, and corporate markers
- deterministic negatives for invalid lookalikes and allowlist scenarios
- context boost and suppression pairs
- overlap and operator cases
- nested structured JSON cases
- adversarial smoke cases for spacing, punctuation, mixed-language, and unicode-obfuscation patterns

Recommended minimum volume:
- 25 to 40 cases per priority deterministic family
- 100 shared hard negatives
- 50 structured JSON cases
- 30 adversarial smoke cases

## Baseline Run Requirements

Before parity reporting is treated as stable, the following runs must exist:
- starter corpus through `Vanilla Presidio`
- starter corpus through `Tuned Presidio`
- starter corpus through Pokrov current baseline
- starter corpus through Pokrov updated baseline

Clinical or image baselines may be added later per workstream.

## Acceptance Criteria

- Every baseline source has usage constraints recorded.
- Restricted datasets are clearly separated from repo-safe fixtures.
- The Phase 1A starter corpus is concrete enough to run first parity reports.
- Required baseline runs are explicit.
