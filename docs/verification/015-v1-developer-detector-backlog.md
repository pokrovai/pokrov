# Verification: 015 V1 Developer Detector Backlog

Date: 2026-04-06
Status: Drafted for implementation planning
Primary audience: developers and coding-agent users

## Purpose

This document freezes the detector backlog for the first Pokrov release aimed at developer traffic.
It defines what must be implemented natively in the deterministic runtime, what may ship as constrained deterministic coverage, and what should stay outside the first release unless a remote recognizer path is introduced.

## Product Boundary For V1

V1 must prioritize developer-facing leak paths that are common in prompts, tool arguments, logs, shell output, config files, and copied debug snippets.

V1 guarantees should focus on:
- secrets, tokens, and key material
- email addresses
- URLs and domains with validation and allowlist suppression
- IPv4 addresses
- phone numbers
- person-name fields only in structured payloads or explicit identity contexts

V1 must not claim broad person-name detection in arbitrary prose or source code without strong context.
That capability belongs to a later remote-recognizer or ML-backed path.

## Delivery Tiers

### Tier 1: Must-Have Deterministic

These detectors should be implemented directly in `pokrov-core` and covered by exact-output replay assertions.

| Priority | Detector family | Why it matters for developer traffic | Implementation mode | Expected v1 action |
|---:|---|---|---|---|
| 1 | `secret_token_family` | Secrets are the highest-severity leak path in prompts and config snippets | pattern + validation + context + allowlist suppression | `block` for high-risk secrets, `redact` where profile requires |
| 2 | `email` | Common PII and corporate identity leak in prompts, commits, and configs | pattern + validation + allowlist suppression | `redact` |
| 3 | `url_or_domain` | Internal dashboards, repos, endpoints, and artifact links leak often in agent workflows | pattern + validation + allowlist suppression | `redact` |
| 4 | `ip_address` | Internal service addresses and logs often expose infrastructure details | pattern + validation | `redact` |
| 5 | `phone_number` | Moderate-cost deterministic detector with clear user value | pattern + context | `redact` |
| 6 | `card_like_number` | Existing baseline family that must stay stable | pattern + checksum + context | `block` or `redact` by profile |

`secret_token_family` in v1 should cover at least:
- OpenAI-style keys
- generic API key assignments such as `api_key=...` and `api-key: ...`
- bearer token headers
- GitHub PAT-like tokens
- JWT-like tokens when high-confidence shape is present
- `.env`, YAML, JSON, and shell assignment contexts

### Tier 2: Constrained Deterministic

These detectors are acceptable in v1 only when the signal is strongly structured or explicitly contextual.
They should not claim broad free-text parity.

| Priority | Detector family | Safe v1 boundary | Implementation mode | Expected v1 action |
|---:|---|---|---|---|
| 7 | `person_first_name_field` | JSON, YAML, TOML, form, or tool-arg fields named `first_name` | key-aware field detection | `redact` |
| 8 | `person_last_name_field` | Structured fields named `last_name` | key-aware field detection | `redact` |
| 9 | `person_middle_name_field` | Structured fields named `middle_name` | key-aware field detection | `redact` |
| 10 | `person_identity_context` | Phrases like `my name is`, `signed by`, `author:`, `from:` | phrase context + constrained candidate pattern | `redact` |
| 11 | `en_address_like_high_risk` | English high-risk address fragments in explicit address contexts | pattern + context + validation | `redact` |
| 12 | `customer_id_contextual` | `customer id`, `client id`, or equivalent explicit markers | pattern + context | `redact` |
| 13 | `account_number_contextual` | Explicit finance or account context only | pattern + context | `redact` or `block` by profile |
| 14 | `swift_bic` | Structured or clearly delimited BIC/SWIFT values | pattern + validation | `redact` |
| 15 | `medical_record_number_contextual` | `MRN` and close variants only | pattern + context | `redact` |
| 16 | `license_plate_contextual` | Vehicle or shipment context only | pattern + context | `redact` |

Structured person-name fields should cover keys such as:
- `first_name`
- `last_name`
- `middle_name`
- `full_name`
- `author`
- `committer`
- `contact_name`
- `user.name`

Explicit identity contexts should cover phrases such as:
- `my name is`
- `I am`
- `signed by`
- `author:`
- `from:`
- `contact:`

### Tier 3: Deferred To Remote NER Or Later Extensions

These capabilities should not be promised in native v1.

| Family | Deferral reason |
|---|---|
| Broad free-text `first_name` / `last_name` / `middle_name` / `name` | High false-positive risk in developer prose and source code |
| `company_name` | High collision rate with package names, repos, domains, and project identifiers |
| `city`, `state`, `country`, `county` | Too generic without NER or rich context |
| `occupation`, `employment_status`, `education_level` | Lower v1 value and weak deterministic signal |
| `race_ethnicity`, `religious_belief`, `sexuality`, `political_view` | Requires more careful semantic handling than v1 deterministic scope allows |
| Broad medical PHI ontology | Already outside native phase-one scope |

## Recommended Implementation Order

Before any detector implementation starts, complete an architecture-analysis phase for the detector layer and decide whether the current runtime model is sufficient for the planned expansion.

1. `secret_token_family`
2. `email`
3. `url_or_domain`
4. `ip_address`
5. `phone_number`
6. `person_first_name_field`
7. `person_last_name_field`
8. `person_middle_name_field`
9. `person_identity_context`
10. `en_address_like_high_risk`
11. `customer_id_contextual`
12. `account_number_contextual`
13. `swift_bic`
14. `medical_record_number_contextual`
15. `license_plate_contextual`

## Implementation Description

### Phase 0: Detector Architecture Analysis

Before implementation, review whether the current detector architecture is suitable for the next wave of deterministic families.
This phase must answer:
- whether built-in rules and deterministic recognizers should remain split or move behind one extensible registry path
- whether validation and context hooks are sufficient for URL, IP, phone, address, and structured-name coverage
- whether structured-field name detection belongs in the current traversal and detection pipeline or needs a dedicated field-aware preclassification layer
- whether the test and report layer already reflects real runtime coverage without overstating taxonomy-only support

This phase should end with one of two outcomes:
- `proceed with current architecture`
- `perform a narrow detector-architecture refactor before adding new families`

### Phase 0 Outcome: 2026-04-06 Architecture Analysis

Decision: `perform a narrow detector-architecture refactor before adding new families`

Findings:
- The current runtime is split between hard-coded built-in rules and the extensible deterministic recognizer path.
- Built-in rules do not use the same deterministic metadata pipeline used by recognizers generated from profile configuration.
- The existing deterministic pipeline is strong enough for URL, IPv4, phone, and most secret detectors because it already supports normalization, validation, context scoring, and allowlist suppression.
- Structured person-name coverage does not fit cleanly into the current recognizer model because recognizers currently match only leaf text, while constrained name coverage needs field-aware gating based on JSON pointer or logical field path.
- The detector file layout is still small enough that a narrow refactor is preferable to a broad module redesign, but `crates/pokrov-core/src/detection/mod.rs` should not absorb the entire new detector wave unchanged.

Required narrow refactor goals:
- Unify built-in detector execution with the deterministic recognizer path so new native families do not require a second implementation style.
- Introduce a narrow field-aware gating mechanism for constrained structured-name detectors.
- Keep policy ownership, overlap resolution, JSON-safe traversal, and metadata-only audit behavior unchanged.
- Preserve the existing deterministic candidate path so validator and context behavior stay reusable for the new families.

Refactor boundaries:
- In scope:
  - detector-path unification
  - field-aware gating for structured names
  - small file split inside `crates/pokrov-core/src/detection/` if needed for maintainability
- Out of scope:
  - remote recognizers
  - ML or NER inference
  - policy-flow changes
  - audit contract changes
  - transform semantics changes

### Workstream A: Native Deterministic Recognizers

Implement new deterministic recognizers in `pokrov-core` using the existing pattern, validation, context-policy, and allowlist machinery.
The runtime already supports deterministic metadata for custom-style rule compilation, so each new detector should fit the current analyzer path rather than introduce a parallel detection subsystem.

Implementation tasks:
- add built-in recognizers where the pattern is global and stable enough for all profiles
- add deterministic validator support where shape validation is required
- add context terms for detectors that are too broad without lexical evidence
- preserve current overlap resolution and policy ownership
- avoid new blocking or heavy dependencies inside the hot path

### Workstream B: Structured Name Coverage

Implement structured-field coverage for person names without broad free-text name matching.
This work should be field-aware and limited to known key paths or explicit identity phrases.

Implementation tasks:
- classify name-like structured keys in JSON leaves before pattern evaluation
- apply constrained redaction only when the field name or local phrase context is explicit
- add adversarial negatives to ensure code identifiers or package names are not redacted as people

### Workstream C: Dataset And Contract Expansion

Every detector introduced above must move at least one replayable dataset label from report-only backlog into exact runtime assertions.
Use cached rows from open snapshots where possible and add starter-corpus fixtures when open rows are too noisy or ambiguous.

Implementation tasks:
- expand `replay_assertable_dataset_labels()` only after runtime support exists
- add one or more exact-output replay assertions per detector family
- keep the report generator aligned with real runtime coverage, not taxonomy-only mappings

## Test Strategy

For every detector family, implementation is incomplete unless all three levels below exist.

### 1. Unit-level detector tests

Add detector-focused tests for:
- positive matches
- hard negatives
- allowlist suppression
- validator rejection of malformed lookalikes
- context gating where applicable

### 2. Contract-level runtime assertions

Add exact-output runtime replay assertions in `tests/contract` for:
- expected `block` decisions where profile semantics require blocking
- exact redacted payload text where profile semantics require redaction
- absence of source values in sanitized payloads

### 3. Dataset-backed gap tracking

Keep `014-dataset-detector-gap-report.md` aligned with:
- mapped labels
- runtime-covered labels
- detector implementation priority for mapped labels
- backlog labels still deferred to later work

## Acceptance Criteria For V1

V1 is ready for the target audience only when the following are true:
- developer-secret families are covered by deterministic recognizers and exact replay tests
- email, URL, IPv4, and phone coverage exist in the runtime and in dataset-backed assertions
- structured person-name fields are redacted in safe constrained contexts
- arbitrary free-text name coverage is not falsely claimed by product or verification docs
- no detector requires heavy NLP or a remote service in the default hot path

## Non-Goals For This Backlog

The following items are explicitly outside this implementation backlog:
- in-process ML NER inside `pokrov-core`
- generic free-text person-name parity
- organization or location NER parity
- broad medical PHI extraction
- OCR, image, or DICOM recognizers

## Notes For Future Remote NER Extension

If free-text person-name coverage becomes mandatory, the next step should be a remote recognizer sidecar rather than expanding the deterministic core into broad NER behavior.
That path already has a draft architectural contract and should stay isolated behind explicit timeout and fail-closed rules.

## Residual Gaps Requiring Remote NER (2026-04-06)

- Broad free-text person names in arbitrary prose or code comments (`name`, `first_name`, `last_name`) remain deferred to avoid deterministic false positives.
- Location and organization disambiguation (`city`, `state`, `country`, `county`, `company_name`) requires semantic context not available in deterministic v1 rules.
- Semantic-sensitive labels (`race_ethnicity`, `religious_belief`, `sexuality`, `political_view`) remain explicitly outside deterministic v1 and should be handled only by an isolated remote recognizer path with strict policy controls.
