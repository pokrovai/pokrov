# Verification: 014 Dataset Detector Gap Report

Date: 2026-04-06
Data source: `tests/fixtures/eval/datasets/open-cache/*.json`
Current mapping source: `tests/common/sanitization_dataset_test_support.rs`
Current enforcement source: `tests/contract/sanitization_open_dataset_pipeline_contract.rs`

## Where labels are fixed today

- Dataset-label-to-entity normalization is fixed in `supported_dataset_label_mapping()`.
- Dataset labels with proven current runtime coverage are fixed in `replay_assertable_dataset_labels()`.
- Labels intentionally left outside current runtime coverage are fixed in `known_unsupported_dataset_labels()`.
- The contract `open_dataset_labels_are_explicitly_partitioned_into_supported_or_backlog` fails when a new label appears in cached snapshots without being added to one of those two lists.

This keeps one explicit source of truth for:

- what dataset labels are normalized to Pokrov entities
- what the current runtime can already replay with exact assertions
- what is tracked as backlog for future detector work

## Current mapped label variants

These are the dataset-label variants currently normalized to Pokrov entities in the test layer. Mapping here does not imply that the runtime already ships a detector for every listed entity.

| Dataset label | Pokrov entity | Hits in cached snapshots | Datasets |
|---|---|---:|---|
| `CREDITCARDNUMBER` | `card_like_number` | 2 | `open_ai4privacy_pii_masking_200k.json` |
| `EMAIL` | `email` | 2 | `open_ai4privacy_pii_masking_200k.json` |
| `IPV4` | `ip_address` | 3 | `open_ai4privacy_pii_masking_200k.json` |
| `STREET` | `en_address_like_high_risk` | 2 | `open_ai4privacy_pii_masking_200k.json` |
| `credit_card_number` | `card_like_number` | 2 | `open_gretel_pii_masking_en_v1.json` |
| `credit_debit_card` | `card_like_number` | 1 | `open_nvidia_nemotron_pii.json` |
| `email` | `email` | 11 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `ipv4` | `ip_address` | 2 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `phone` | `phone_number` | 0 | none in current cached rows |
| `phone_number` | `phone_number` | 5 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `street_address` | `en_address_like_high_risk` | 10 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `url` | `url_or_domain` | 6 | `open_nvidia_nemotron_pii.json` |

## Current runtime-covered label variants

These mapped labels already have stable runtime behavior and exact-output replay assertions today.

| Dataset label | Pokrov entity | Hits in cached snapshots | Datasets |
|---|---|---:|---|
| `CREDITCARDNUMBER` | `card_like_number` | 2 | `open_ai4privacy_pii_masking_200k.json` |
| `EMAIL` | `email` | 2 | `open_ai4privacy_pii_masking_200k.json` |
| `IPV4` | `ip_address` | 3 | `open_ai4privacy_pii_masking_200k.json` |
| `credit_card_number` | `card_like_number` | 2 | `open_gretel_pii_masking_en_v1.json` |
| `credit_debit_card` | `card_like_number` | 1 | `open_nvidia_nemotron_pii.json` |
| `email` | `email` | 11 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `ipv4` | `ip_address` | 2 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `url` | `url_or_domain` | 6 | `open_nvidia_nemotron_pii.json` |

## Current replay coverage

The current format-specific runtime assertions use rows that produce deterministic, already-supported behavior:

- `ai4privacy` row `15`: `CREDITCARDNUMBER` -> expected `block`
- `ai4privacy` row `20`: `EMAIL` -> expected exact redaction
- `Nemotron` row `4`: `credit_debit_card` -> expected `block`
- `Nemotron` row `23`: `email` -> expected exact redaction
- `Gretel` row `12`: `credit_card_number` -> expected `block`
- `Gretel` row `3`: `email` -> expected exact redaction

These rows are intentionally narrow. They verify runtime behavior only where current detector coverage is explicit and stable.

## Detector implementation priority for mapped labels

These labels are already mapped to Pokrov entities, but the current runtime does not yet expose stable detector coverage for them in exact-output replay assertions.

| Priority | Dataset label | Pokrov entity | Hits | Suggested replay row |
|---:|---|---|---:|---|
| 1 | `street_address` | `en_address_like_high_risk` | 10 | `open_nvidia_nemotron_pii.json` row `0` |
| 2 | `phone_number` | `phone_number` | 5 | `open_nvidia_nemotron_pii.json` row `3` |
| 3 | `STREET` | `en_address_like_high_risk` | 2 | `open_ai4privacy_pii_masking_200k.json` row `12` |

## Detector backlog from dataset analysis

The following labels are present in cached open datasets but are not yet mapped to current Pokrov detector coverage in the test layer.

### High-frequency backlog candidates

These labels appear often enough to justify detector-design discussion first.

| Dataset label | Hits | Datasets |
|---|---:|---|
| `FIRSTNAME` | 10 | `open_ai4privacy_pii_masking_200k.json` |
| `company_name` | 8 | `open_nvidia_nemotron_pii.json` |
| `customer_id` | 8 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `date` | 15 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `date_of_birth` | 13 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `first_name` | 17 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `last_name` | 13 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `medical_record_number` | 21 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `ssn` | 15 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |

### Medium-frequency backlog candidates

These labels have enough evidence to justify detector design after the highest-frequency privacy identifiers.

| Dataset label | Hits | Datasets |
|---|---:|---|
| `LASTNAME` | 4 | `open_ai4privacy_pii_masking_200k.json` |
| `NEARBYGPSCOORDINATE` | 4 | `open_ai4privacy_pii_masking_200k.json` |
| `account_number` | 4 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `biometric_identifier` | 4 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `blood_type` | 4 | `open_nvidia_nemotron_pii.json` |
| `county` | 4 | `open_nvidia_nemotron_pii.json` |
| `employment_status` | 4 | `open_nvidia_nemotron_pii.json` |
| `license_plate` | 5 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |
| `name` | 5 | `open_gretel_pii_masking_en_v1.json` |
| `occupation` | 4 | `open_nvidia_nemotron_pii.json` |
| `religious_belief` | 4 | `open_nvidia_nemotron_pii.json` |
| `swift_bic` | 4 | `open_nvidia_nemotron_pii.json` |
| `time` | 4 | `open_gretel_pii_masking_en_v1.json`, `open_nvidia_nemotron_pii.json` |

### Long-tail backlog candidates

These labels are present, but each currently has low evidence volume in cached snapshots:

- `ACCOUNTNUMBER`
- `AGE`
- `BUILDINGNUMBER`
- `CITY`
- `COUNTY`
- `CREDITCARDISSUER`
- `DATE`
- `DOB`
- `EYECOLOR`
- `GENDER`
- `HEIGHT`
- `IPV6`
- `JOBAREA`
- `JOBTITLE`
- `MASKEDNUMBER`
- `MIDDLENAME`
- `PASSWORD`
- `PHONEIMEI`
- `PIN`
- `PREFIX`
- `STATE`
- `TIME`
- `USERAGENT`
- `VEHICLEVIN`
- `VEHICLEVRM`
- `address`
- `age`
- `bank_routing_number`
- `certificate_license_number`
- `city`
- `coordinate`
- `country`
- `cvv`
- `date_time`
- `device_identifier`
- `education_level`
- `employee_id`
- `gender`
- `health_plan_beneficiary_number`
- `mac_address`
- `password`
- `pin`
- `political_view`
- `race_ethnicity`
- `sexuality`
- `state`
- `unique_identifier`
- `user_name`
- `vehicle_identifier`

## Recommended detector priority

The next detector candidates should be prioritized as:

1. `ssn`
2. `medical_record_number`
3. `first_name`
4. `last_name`
5. `date_of_birth`
6. `account_number`
7. `license_plate`
8. `swift_bic`
9. `customer_id`

Rationale:

- Higher-frequency labels should move first when they represent clear privacy or secret-bearing identifiers.
- Name-like and customer-like fields should stay behind stronger context constraints to avoid false positives.
- Mapped-but-not-yet-runtime-covered labels still require detector implementation before they can move into exact replay coverage.

## Current limitations

- The current report is derived from cached open snapshots, not from full upstream datasets.
- The current runtime assertion set intentionally covers card, email, IPv4, and URL behavior.
- The report includes a dedicated detector-gap priority section for mapped labels that still lack current runtime recognizers or stable exact-output replay assertions.
- `open_presidio_research_repo.json` is metadata-only and is not part of replay coverage.

## Verification commands

- `cargo test starter_dataset_fixture_replays_through_runtime_engine -- --nocapture`
- `cargo test rows_match_expected --test contract -- --ignored --nocapture`
- `cargo test partitioned_into_supported_or_backlog --test contract -- --ignored --nocapture`
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture`

## Verification results (2026-04-06)

- `cargo test starter_dataset_fixture_replays_through_runtime_engine -- --nocapture` -> PASS
- `cargo test rows_match_expected --test contract -- --ignored --nocapture` -> PASS
- `cargo test partitioned_into_supported_or_backlog --test contract -- --ignored --nocapture` -> PASS
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture` -> PASS
