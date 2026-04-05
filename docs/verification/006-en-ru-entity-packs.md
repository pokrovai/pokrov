# Verification Runbook: 006 EN/RU Entity Packs

## Scope

This runbook verifies the phase-one EN/RU entity-pack contract used by deterministic recognizer planning and parity preparation.
It covers:

- explicit supported entity set
- explicit unsupported/deferred entity set with rationale
- entity-to-recognizer-family mapping
- entity-to-language mapping
- entity-to-default-risk-class mapping
- EN and RU language-sensitive context/list requirements for supported entities

## Automated Verification Commands

```bash
cargo test --test contract -- sanitization_entity_pack
cargo test -p pokrov-core phase_one_en_ru_entity_pack
```

## Acceptance Checklist

- every supported entity maps to at least one recognizer family
- coverage projection exposes all required reporting sections
- EN-specific and RU-specific entities are explicit in the pack
- RU language-sensitive entities require the expected list/context constraints
- unsupported/deferred entities are listed with non-empty rationale

## Deferred Coverage Notes

The following remain explicitly out of first-phase native scope:

- broad person-name ML parity
- location and organization NER families requiring heavy NLP
- medical and full PHI ontology families
- global long-tail national identifier coverage beyond selected deterministic validators
