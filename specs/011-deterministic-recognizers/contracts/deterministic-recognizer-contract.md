# Contract: Deterministic Recognizer Pipeline

## 1. Scope

This contract defines the configuration and analyzer-surface behavior for Pokrov's native deterministic recognizer families. It is limited to pattern, validation, contextual, allowlist, and denylist behavior inside the existing analyzer foundation.

Out of scope:

- remote recognizer wire protocols
- machine-learning recognizers
- broader entity-pack rollout beyond phase-one priorities

## 2. Configuration Contract

Deterministic recognizers extend the existing sanitization profile model in `pokrov-config`.

### Required profile capabilities

- multiple deterministic recognizer definitions per profile
- recognizer-specific family priority
- optional validator configuration
- optional context dictionaries for EN and RU
- profile-scoped allowlist and denylist entries
- optional tenant and language restrictions for list controls where profile policy allows them

### Validation requirements

- invalid recognizer definitions fail startup validation
- duplicate recognizer identifiers inside one profile are rejected
- invalid patterns or invalid validator configuration are rejected
- exact-match allowlist values must be normalizable before acceptance
- denylist and allowlist entity scopes must map to approved recognizer categories or families

## 3. Runtime Candidate Contract

Every deterministic family emits the same metadata-only candidate shape before overlap resolution.

### Required candidate fields

- normalized category
- location kind
- field path metadata (`json_pointer` and/or logical field path)
- start and end offsets when span-based
- final candidate score before overlap resolution
- recognizer identifier
- evidence class
- validation status
- reason codes
- suppression status
- family priority
- language

### Prohibited candidate fields

- raw matched substrings
- surrounding source snippets
- raw normalized secrets or PII values

## 4. Deterministic Execution Order

The deterministic pipeline order is fixed:

1. initial pattern or list match creation
2. normalization
3. validator or checksum evaluation
4. context scoring
5. allowlist suppression
6. overlap resolution
7. policy evaluation

## 5. Default Semantics

### Validation

- failed validation rejects a candidate by default
- a family may keep a downgraded candidate only when that exception is explicitly documented in configuration and explain metadata

### Negative context

- negative context downscores a candidate by default
- outright suppression requires an explicit family-level opt-in

### Allowlist

- allowlists suppress only exact normalized matches inside the configured entity scope
- embedded or substring matches are not suppressed by default

### Denylist

- denylist positives become first-class candidates
- they still pass through overlap resolution and policy evaluation

### Same-span precedence

- highest final score wins first
- equal scores are broken by family priority
- remaining ties are broken by stable recognizer ordering

## 6. Structured Payload Compatibility

- deterministic recognizers operate on normalized string leaves from JSON traversal
- structured-field origin must be preserved in metadata
- plain-text and structured-field inputs with equivalent content must produce equivalent candidate semantics except for field-context metadata

## 7. Consumer Expectations

Runtime and evaluation consumers may rely on:

- deterministic replay identity remaining stable for identical input and configuration
- metadata-only explain and audit outputs
- stable candidate and resolved-hit contracts across LLM, MCP, direct evaluation, and future baseline comparison flows
