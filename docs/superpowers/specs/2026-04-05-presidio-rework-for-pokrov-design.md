# Presidio Rework For Pokrov Design

Date: 2026-04-05
Status: Updated after evaluation-focused planning

## Summary

This design defines a parity-driven rework of Presidio capabilities for Pokrov.
The goal is not a literal full port of Presidio, but a `Pokrov-compatible full` implementation of the selected handler families without losing the concrete mechanisms inside each family.

The selected scope focuses on text core plus structured processing, with a separate evaluation and evidence workstream:
- text recognizers and analyzer orchestration
- anonymizer operators
- registry and explainability
- JSON and semi-structured processing
- batch structured processors as a separate execution mode
- evaluation corpora, parity checks, and quality gates

Heavy ML, OCR, image, and DICOM capabilities remain extension families behind explicit service adapters.

## 1. Product Boundary

Presidio rework for Pokrov is treated as a three-level program rather than a single crate-level port.

| Level | Includes | Lives in |
|---|---|---|
| `L1 Native Pokrov Core` | text recognizers, analyzer orchestration, anonymizer operators, JSON traversal, policy binding, audit-safe explainability | Rust workspace |
| `L2 Pokrov Extensions` | heavy recognizers via unified adapters, local ML sidecars, OCR services, PHI services, batch structured workers | separate self-hosted or cloud services |
| `L3 Out Of Pokrov Product Scope` | UI, notebooks, demos, Spark and Fabric recipes, Python SDK parity, deployment samples | not part of the product core |

Hard boundaries:
- Only capabilities compatible with `sanitization-first`, `metadata-only audit`, `JSON-safe traversal`, and latency constraints belong in core.
- Heavy NLP and OCR runtimes must not become embedded hot-path dependencies of the Rust proxy process.
- Reversible transforms and deanonymization are not part of the baseline core because they expand leak surface and conflict with Pokrov security posture.

## 2. Capability Matrix

| Capability | Presidio | Pokrov status | Solution |
|---|---|---|---|
| Pattern recognizers | regex, checksums, deny lists | `Native` | extend in `pokrov-core` |
| Context-aware text detection | context words, scoring, validation | `Native` | add scorer and validator layers |
| Recognizer registry | pluggable recognizers | `Native` | trait-based Rust registry |
| Ad-hoc and custom recognizers | per request or config | `Native` with limits | allow only policy-bound, audit-safe sources |
| Decision trace | rich decision process | `Native`, reduced | metadata-safe explain only |
| Anonymizer operators | replace, mask, redact, hash, keep, encrypt | `Native` partial | native operators except reversible core paths |
| Reversible deanonymization | decrypt, deanonymize | `Unsupported in core` | only as separate tightly controlled service if ever needed |
| Structured JSON | semi-structured JSON | `Native` | extend current traversal and analysis model |
| Tables, DataFrames, SQL batch | structured package | `Batch/offline only` | separate worker or job mode |
| Image OCR redaction | image plus bounding boxes | `Adapter to service` | separate OCR and redaction service |
| DICOM pixel redaction | medical imaging | `Adapter to service` | specialized service |
| ML NER models | spaCy, stanza, transformers | `Adapter to service` | self-hosted ML recognizer or cloud connector |
| Remote recognizers | Azure, AHDS, external detectors | `Adapter to service` | standard remote recognizer contract |
| Multi-language entity packs | wide global coverage | `Hybrid` | EN and RU native first, other languages via packs or services |
| Notebook, demo, deployment recipes | ecosystem content | `Out of scope` | do not port into product |

Key conclusion:
- Pokrov native implementation can cover the majority of Presidio value for proxy use cases.
- The remaining value sits mostly in external recognizer and redaction orchestration rather than in-process Rust parity.

## 3. Target Architecture

Pokrov should evolve from a narrow `detect -> resolve -> transform` flow into a fuller analyzer pipeline.

### Core logical layers
- `recognizer`
  - common recognizer traits
  - built-in pattern recognizers
  - checksum validators
  - context scorers
  - config-defined custom recognizers
- `registry`
  - active recognizer assembly by profile, language, and path class
- `analysis`
  - recognizer orchestration
  - merge, scoring, suppression, overlap resolution
  - safe decision summary
- `operator`
  - replace, mask, redact, hash, keep
  - deterministic operator selection
- `adapter`
  - remote recognizer clients
  - external ML and OCR connectors

Target flow:
`AnalyzerEngine -> PolicyResolver -> TransformEngine -> Audit/Explain`

### Recognizer classes
- `Native recognizers`
  - regex and pattern recognizers
  - checksum-backed validators
  - keyword and denylist recognizers
  - context-enhanced recognizers
  - corporate marker detectors
  - EN and RU packs
- `Remote recognizers`
  - ML NER services
  - PHI services
  - cloud PII services
  - OCR extraction services

Rules for remote recognizers:
- They are explicitly policy-gated.
- They have time budgets and degradation semantics.
- They never own final policy outcome.
- Pokrov must not silently allow unsafe payloads when a remote recognizer degrades.

### Execution modes
- `inline_proxy_mode`
  - only lightweight native recognizers
  - strict latency budget
- `extended_inline_mode`
  - native plus short-timeout remote recognizers
- `batch_mode`
  - structured tables
  - heavy ML
  - OCR and image processing
  - offline redaction jobs

### Stable contract between Pokrov and extensions
Normalized recognizer hit contract:
- entity type or category
- location
  - JSON pointer
  - string-leaf start and end when applicable
  - logical field path for structured batch flows
- confidence score
- source recognizer id
- evidence class
  - pattern
  - checksum
  - context
  - model
  - OCR
- safe explain metadata
- optional suggested action

External services only return detection results. Final policy decisions remain in Pokrov.

### Crate responsibility boundaries
- `pokrov-core`
  - recognizers
  - registry
  - analyzer pipeline
  - operators
  - overlap, policy, and transform logic
- `pokrov-config`
  - recognizer packs
  - remote connector config
  - language and profile binding
- `pokrov-proxy-llm` and `pokrov-proxy-mcp`
  - engine invocation and application of outcomes only
- external services
  - ML NER
  - OCR and image redaction
  - PHI and DICOM
  - structured batch workers

## 4. Phased Roadmap

### Phase 1A
Directly inside Pokrov:
- native recognizer framework
- pattern, checksum, context, allowlist, and denylist families
- Presidio-like text entity packs for EN and RU
- operator expansion
- evaluation case format
- synthetic corpus
- first Presidio parity runner

### Phase 1B
Stabilization and evidence:
- explainability upgrade
- deterministic family scoreboards
- regression reports
- latency evidence

### Phase 2
As platform extensions:
- remote recognizer contract
- self-hosted ML recognizer service
- cloud recognizer connectors
- curated gold corpus
- adversarial corpus
- degradation metrics

### Phase 3
Structured parity:
- field-aware JSON and semi-structured processing
- batch processors for tabular and SQL-style workflows
- structured evaluation scorecards

### Phase 4
Only if strong product pull appears:
- broad country-specific packs
- medical entity families
- OCR and image redaction service families
- DICOM pixel redaction
- extended evaluation harness
- recognizer marketplace or external pack loading

### Never or not Pokrov
- Python SDK parity as a product goal
- notebooks, demos, Streamlit, and deployment recipe parity
- runtime custom lambda anonymizers
- built-in deanonymization in proxy core
- heavy NLP or OCR runtimes inside the hot-path Rust process

## 5. Handler Family Parity Principle

The roadmap is driven by handler families, not by broad capability labels.
For each chosen family the design captures the internal Presidio mechanisms so they are not flattened into a generic "supported" marker.

Selected families for detailed parity:
- pattern recognizers
- checksum and validation recognizers
- context-aware recognizers
- deny and allow list recognizers
- recognizer registry and loading
- decision process and explainability
- anonymizer operators
- remote recognizer contract
- structured and semi-structured processors

Parity rule:
- target `Pokrov-compatible full` coverage inside each selected family
- exclude only mechanisms that violate Pokrov invariants or require heavy external runtime

## 6. Handler Family Catalog

### 6.1 Pattern recognizers
Parity target: almost full native parity.

Required mechanisms:
- regex-based detection
- multiple patterns per entity
- per-pattern score and priority
- validation and invalidation hooks
- entity-specific normalization before validation
- ad-hoc recognizers from trusted config
- YAML and file-based loading

Pokrov direction:
- implement natively
- represent recognizers with traits carrying pattern set, base score, priority, validator, and invalidator
- preserve deterministic scoring and suppression

Not in core:
- Python class loading
- runtime code execution hooks

### 6.2 Checksum and validation recognizers
Parity target: full native parity for deterministic validators.

Required mechanisms:
- checksum validation for IDs such as card numbers and IBAN
- combined context and checksum scoring
- post-match validation after regex detection
- invalidation of syntactically valid but semantically invalid candidates

Pokrov direction:
- implement natively
- compose validation pipeline as `regex -> normalize -> checksum -> context score`
- validators can reject or downscore candidates deterministically

Initial priority groups:
- card-like numbers
- IBAN
- IP and URL basic validation
- selected deterministic national identifiers

### 6.3 Context-aware recognizers
Parity target: near-full native parity without heavy NLP dependencies.

Required mechanisms:
- nearby context words around a detected span
- positive and negative context
- score boosting and suppression
- recognizer-specific context lists

Pokrov direction:
- implement natively using lexical context windows
- maintain language-specific context dictionaries for EN and RU

Not in core:
- lemma-based, POS-based, or deep NLP context logic

### 6.4 Deny and allow list recognizers
Parity target: full native parity.

Required mechanisms:
- exact or high-confidence deny lists
- allow lists for false-positive suppression
- entity-specific allow lists
- scope binding by request, profile, tenant, or language

Pokrov direction:
- implement natively
- model deny lists as recognizers and allow lists as suppression layers
- bind them by profile, tenant, language, and path class

### 6.5 Recognizer registry and loading
Parity target: almost full native parity.

Required mechanisms:
- active recognizer registry
- predefined recognizers
- custom recognizers
- config-driven loading
- language-aware registry composition
- ad-hoc recognizers on request

Pokrov direction:
- implement natively
- build deterministic registry assembly by profile, language, and path class
- keep configuration in `pokrov-config`
- allow ad-hoc recognizers only through explicit policy and trusted source constraints

### 6.6 Decision process and explainability
Parity target: Pokrov-safe parity.

Required mechanisms:
- recognizer provenance
- reason for score assignment
- accepted and rejected hit reasoning
- overlap and conflict traces
- decision logging

Pokrov direction:
- implement natively in metadata-only form
- use safe reason codes such as:
  - `pattern_match`
  - `checksum_valid`
  - `context_boost`
  - `allowlist_suppressed`
  - `overlap_lost`
  - `policy_escalated`
- use confidence buckets rather than raw evidence dumps

Not allowed:
- raw snippets
- nearby source text
- payload fragments

### 6.7 Anonymizer operators
Parity target: maximum parity within Pokrov invariants.

Required native operators:
- replace
- redact
- mask
- hash
- keep

Required semantics:
- operator mapping by entity, category, and profile
- deterministic overlap-aware application order

Not in core parity target:
- runtime custom lambda operators
- reversible encrypt and decrypt paths
- deanonymization in the proxy core

### 6.8 Remote recognizer contract
Parity target: full parity in integration shape.

Required mechanisms:
- external recognizers
- cloud recognizers
- remote entity mapping
- normalization into common analyzer flow

Pokrov direction:
- implement as adapter contract
- send input text or JSON leaf payload, language, entity scope, and timeout budget
- normalize remote spans, scores, and provenance into internal hit model
- keep final policy in Pokrov
- make degradation explicit with fail-closed default and degraded-mode metadata

### 6.9 Structured and semi-structured processors
Parity target: full parity in processor semantics, split by execution mode.

Required mechanisms:
- JSON and semi-structured traversal
- tabular analysis
- field-aware anonymization
- batch processing
- processor-specific field mapping

Pokrov direction:
- implement JSON and semi-structured support natively
- implement CSV, tabular, and SQL-style processing as batch or offline workers
- preserve field semantics rather than reducing structured handling to generic string-leaf traversal

Needed semantics:
- field classifier
- column and field policy binding
- selective analyzer families per field type
- metadata-only batch summaries

## 7. Evaluation And Evidence Architecture

Evaluation is a first-class workstream, not just a test appendix.
It exists to prove that Pokrov preserves handler-family semantics, to measure quality on gold corpora, to detect regressions, and to support rollout decisions with evidence rather than intuition.

### Core artifacts
- `Reference corpus`
  - synthetic cases
  - curated gold cases
  - adversarial cases
- `Gold labels`
  - entity spans
  - entity types and categories
  - expected operator outcome
  - expected policy outcome
  - expected JSON validity and field-level effects
- `Presidio baseline report`
  - coverage map
  - parity delta by family, entity, and operator
- `Pokrov evaluation report`
  - precision
  - recall
  - F2
  - family and entity breakdown
  - latency
  - degradation behavior
  - leakage checks
- `Readiness scoreboard`
  - `experimental`
  - `baseline measured`
  - `regression gated`
  - `release gated`

### Evaluation case model
Each evaluation case should include:
- `case_id`
- `language`
- `mode`
  - `text`
  - `structured_json`
  - `batch_structured`
  - later `image_ocr`
- `input`
- `expected_entities`
- `expected_operator_outcome`
- `expected_policy_outcome`
- `tags`
  - handler family
  - entity group
  - difficulty
  - adversarial flag
  - standards mapping
- `source`
  - synthetic
  - curated
  - imported baseline
- `notes`

### Corpora
- `Synthetic corpus`
  - regex variants
  - checksum valid and invalid pairs
  - context boost and no-context cases
  - allowlist suppression
  - denylist positives
  - overlap cases
  - nested JSON
  - operator edge cases
- `Curated gold corpus`
  - de-identified realistic prompts, tool calls, and outputs
  - EN and RU cases
  - structured JSON with field semantics
  - hard negatives that resemble PII
- `Adversarial corpus`
  - whitespace and punctuation splitting
  - unicode confusables
  - mixed RU and EN content
  - fragmented and nested JSON
  - bypass-oriented obfuscation patterns

### Recommended external datasets and baselines
- `Presidio Research`
  - use the `presidio-research` notebooks and tooling as the default evaluation workflow scaffold
  - use it for synthetic generation, exploratory analysis, split strategy, and parity reporting
- `n2c2 / i2b2 de-identification datasets`
  - use the 2006 and 2014 de-identification challenge datasets as the primary external clinical-text benchmark where access is permitted
  - treat them as restricted-access clinical gold sets, not as redistributable fixtures inside the Pokrov repository
- `Pseudo-PHI-DICOM-Data`
  - use the TCIA `Pseudo-PHI-DICOM-Data` collection as the default external benchmark for future DICOM and image-redaction evaluation
  - keep it out of the core text roadmap, but reserve the dataset in the evaluation design now
- `Internal curated de-identified corpus`
  - build a Pokrov-owned gold corpus from de-identified prompts, tool arguments, model outputs, and structured JSON examples
  - this should become the main benchmark for proxy-specific adversarial and mixed-language cases that public datasets do not cover well

Recommended baseline implementations:
- `Vanilla Presidio`
  - default Presidio analyzer and anonymizer configuration
- `Tuned Presidio`
  - Presidio configured and tuned through `presidio-research` style workflows
- `NLM Scrubber`
  - optional clinical-text baseline for PHI-oriented evaluation where direct comparison is useful
- `Remote-service baselines`
  - Azure AI Language PII or AHDS only for the handler families that are intentionally delegated to remote recognizers or surrogate operators

Baseline usage rules:
- do not treat any external implementation as ground truth
- compare Pokrov against the baseline only within matching handler families
- report access restrictions, licensing constraints, and redistribution limits for every external dataset
- keep open synthetic corpora and repository-shippable gold cases separate from restricted clinical corpora

### External dataset inventory

| Dataset or source | Access model | Main languages | Main entity focus | Intended handler families |
|---|---|---|---|---|
| `presidio-research` generated datasets and notebooks | Open workflow tooling | EN first, extensible | synthetic PII and recognizer evaluation flows | pattern, checksum, context, operators, parity reporting |
| `n2c2 / i2b2 2006/2014 de-identification` | Restricted access | EN | clinical PHI entities in narrative text | remote recognizers, PHI families, curated clinical gold sets |
| `Pseudo-PHI-DICOM-Data` from TCIA | External dataset, separate download | EN-oriented medical imaging metadata and pixels | DICOM pixel PHI and OCR-oriented image redaction | future OCR/image/DICOM families |
| `Pokrov internal de-identified corpus` | Internal only | EN and RU | prompts, tool arguments, outputs, structured JSON | proxy-specific text, structured, adversarial, mixed-language cases |
| `Optional public hard-negative corpora` | Depends on source | EN and RU if curated internally | non-PII strings resembling IDs, URLs, emails, or names | false-positive measurement for deterministic families |

Required metadata for every dataset entry:
- access model
- license and redistribution limits
- language coverage
- supported entity classes
- intended handler families
- whether the dataset is allowed inside the repository, CI, or only in local or secured evaluation environments

### Oracles and standards mapping
- `Gold truth`
  - main oracle for precision, recall, and F2
- `Presidio baseline`
  - comparative reference for parity, not a source of truth
- `Standards mapping`
  - HIPAA Safe Harbor for PHI coverage accountability
  - NIST PII vocabulary and privacy-risk framing
  - GDPR personal-data taxonomy anchor

### Metrics
- Detection quality
  - precision
  - recall
  - F2
  - per-entity breakdown
  - per-family breakdown
  - per-language breakdown
  - false-positive and false-negative hotspots
- Parity quality
  - coverage delta versus Presidio
  - detection delta versus Presidio
  - operator delta versus Presidio
  - structured semantic delta
- Security quality
  - leakage count in explain, audit, and logs
  - fail-closed correctness
  - remote recognizer degradation behavior
  - adversarial bypass rate
- Runtime quality
  - p50 and p95 latency
  - latency by family
  - native versus remote cost split
- Transformation quality
  - operator correctness
  - overlap resolution correctness
  - JSON validity preservation
  - field-preservation correctness for structured data

### Evaluation modes
- `fast_local`
  - synthetic subset
  - smoke parity
  - deterministic regression
- `full_corpus`
  - synthetic, curated, and adversarial datasets
  - quality reports
  - family and entity breakdown
- `baseline_comparison`
  - Pokrov versus Presidio diff report

### Quality gate strategy
- `Level 0`
  - collect metrics only
- `Level 1`
  - regression gate for deterministic families
- `Level 2`
  - thresholds for priority entities and families
- `Level 3`
  - rollout gates for structured and remote recognizer modes

### Phase 1A starter corpus
- `Deterministic positives`
  - email, phone, card-like number, IBAN, IP, URL, secret token, corporate marker
- `Deterministic negatives`
  - invalid card-like and IBAN variants
  - URLs and domains that should be allowlisted
  - numeric strings that resemble identifiers but should not match
- `Context cases`
  - positive context boost
  - negative context suppression
  - same pattern with and without nearby evidence words
- `List-based cases`
  - denylist positive
  - allowlist suppression
  - entity-specific allowlist conflict
- `Overlap and operator cases`
  - nested detections
  - competing recognizers on one span
  - replace, redact, mask, hash, and keep outcomes
- `Structured JSON cases`
  - nested objects and arrays
  - path-sensitive fields
  - mixed string and non-string leaves
- `Adversarial smoke cases`
  - spacing and punctuation splits
  - mixed RU and EN strings
  - simple unicode obfuscation

Recommended minimum starter volume:
- at least 25 to 40 cases per priority deterministic family
- at least 100 hard negatives shared across families
- at least 50 structured JSON cases
- at least 30 adversarial smoke cases before enabling regression gates

### Initial baseline runs
- `Vanilla Presidio text baseline`
  - run the starter corpus through default Presidio analyzer and anonymizer settings
  - record detection and operator deltas family-by-family
- `Tuned Presidio baseline`
  - run the same corpus through a tuned Presidio configuration derived through `presidio-research`
  - use this as the stronger parity reference for deterministic families
- `Pokrov native baseline`
  - run the same corpus through the current and then updated Pokrov pipeline
  - store the first release-quality report as the regression anchor
- `Clinical optional baseline`
  - only for PHI workstreams, compare against `NLM Scrubber` or the approved remote PHI service where licensing and access allow it
