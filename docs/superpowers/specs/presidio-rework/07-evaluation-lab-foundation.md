# Evaluation Lab Foundation

Date: 2026-04-05
Status: Draft

## Context

Parity by handler family is not useful without measurable evidence.
This specification defines the first-class evaluation subsystem used to measure quality, parity, safety, and rollout readiness across Pokrov and comparative baselines.

## Goals

- Define the common evaluation case and report model.
- Define the three required corpora: synthetic, curated, and adversarial.
- Freeze the metric groups used by parity and readiness reporting.
- Define progressive quality gates instead of immediate hard global thresholds.

## Non-Goals

- Final dataset population.
- Final threshold values for every entity family.
- Clinical or image benchmark execution details.

## Core Artifacts

- `EvaluationCase`
- `EvaluationResult`
- `EvaluationReport`
- `ParityReport`
- `ReadinessScoreboard`

## Evaluation Case Schema

Each case must include:
- `case_id`
- `language`
- `mode`
- `input`
- `expected_entities`
- `expected_operator_outcome`
- `expected_policy_outcome`
- `tags`
- `source`
- `notes`

Supported initial modes:
- `text`
- `structured_json`
- `batch_structured` placeholder
- future `image_ocr` placeholder

## Corpora

### Synthetic corpus
Purpose:
- cover deterministic families cheaply and completely

Must include:
- regex variants
- checksum valid and invalid pairs
- context boost and suppression pairs
- allowlist and denylist cases
- overlap cases
- operator cases
- nested JSON cases

### Curated gold corpus
Purpose:
- measure realistic behavior on de-identified examples

Must include:
- EN and RU prompts
- tool arguments and outputs
- structured JSON cases
- hard negatives

### Adversarial corpus
Purpose:
- measure bypass resistance

Must include:
- spacing and punctuation obfuscation
- unicode confusables
- mixed-language strings
- fragmented JSON patterns
- simple exfiltration-oriented disguises

## Metrics

### Detection metrics
- precision
- recall
- F2
- per-entity breakdown
- per-family breakdown
- per-language breakdown

### Parity metrics
- detection delta versus Presidio
- operator delta versus Presidio
- coverage delta versus Presidio

### Security metrics
- leakage checks
- fail-closed correctness
- adversarial bypass rate

### Runtime metrics
- p50 latency
- p95 latency
- native versus remote recognizer cost split

### Transformation metrics
- operator correctness
- overlap correctness
- JSON validity preservation

## Quality Gate Strategy

### Level 0
- baseline collection only
- no blocking gates

### Level 1
- deterministic family regression gates
- no regression on stable baseline reports

### Level 2
- thresholds for priority entities and families
- rollout gates for selected deterministic families

### Level 3
- structured and remote-flow rollout gates
- readiness scoreboards become release-significant

## Output Reports

Every evaluation cycle should be able to emit:
- family summary report
- entity breakdown report
- parity report against baseline
- safety and leakage report
- readiness scoreboard

## Acceptance Criteria

- Evaluation cases can be replayed against runtime-compatible contracts.
- The three corpus types and their purposes are explicit.
- Metrics are frozen well enough for later threshold setting.
- Quality gates can evolve without redefining the report model.
