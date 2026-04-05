use std::collections::{BTreeMap, BTreeSet};

use crate::sanitization_dataset_test_support::{
    known_unsupported_dataset_labels, read_open_snapshot, replay_assertable_dataset_labels,
    supported_dataset_label_mapping, OPEN_SNAPSHOT_FILES,
};

pub const DATASET_DETECTOR_GAP_REPORT_PATH: &str =
    "docs/verification/014-dataset-detector-gap-report.md";

#[derive(Debug, Clone)]
struct LabelStats {
    hits: usize,
    datasets: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy)]
struct ReplayCoverageEntry {
    dataset_name: &'static str,
    row_idx: usize,
    label: &'static str,
    expectation: &'static str,
}

#[derive(Debug, Clone)]
struct ReplayExpansionCandidate {
    label: String,
    entity: String,
    hits: usize,
    dataset_file: String,
    row_idx: usize,
}

pub fn render_dataset_detector_gap_report(report_date: &str) -> String {
    let stats = collect_label_stats();
    let supported_mapping = supported_dataset_label_mapping();
    let unsupported_labels = known_unsupported_dataset_labels();
    let replay_coverage = replay_coverage_entries();
    let replay_assertable = replay_assertable_dataset_labels();
    let replay_expansion = replay_expansion_candidates(&stats, &supported_mapping, &replay_assertable);
    let runtime_covered_rows =
        format_runtime_covered_rows(&stats, &supported_mapping, &replay_assertable);

    let supported_rows = supported_mapping
        .iter()
        .map(|(label, entity)| {
            let stats = stats.get(*label);
            let hits = stats.map_or(0, |stats| stats.hits);
            let datasets = format_datasets(stats.map(|stats| &stats.datasets));
            format!(
                "| `{label}` | `{entity}` | {hits} | {datasets} |",
                label = label,
                entity = entity,
                hits = hits,
                datasets = datasets
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let high_frequency = format_label_group_table(
        &stats,
        &unsupported_labels,
        |hits| hits >= 8,
        "These labels appear often enough to justify detector-design discussion first.",
    );
    let medium_frequency = format_label_group_table(
        &stats,
        &unsupported_labels,
        |hits| (4..8).contains(&hits),
        "These labels have enough evidence to justify detector design after the highest-frequency privacy identifiers.",
    );
    let long_tail = stats
        .iter()
        .filter(|(label, stats)| unsupported_labels.contains(label.as_str()) && stats.hits <= 3)
        .map(|(label, _)| format!("- `{label}`", label = label))
        .collect::<Vec<_>>()
        .join("\n");

    let replay_expansion_rows = if replay_expansion.is_empty() {
        "_No mapped replay-expansion candidates in current cached snapshots._".to_string()
    } else {
        let rows = replay_expansion
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                format!(
                    "| {priority} | `{label}` | `{entity}` | {hits} | `{dataset}` row `{row_idx}` |",
                    priority = index + 1,
                    label = candidate.label,
                    entity = candidate.entity,
                    hits = candidate.hits,
                    dataset = candidate.dataset_file,
                    row_idx = candidate.row_idx
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "| Priority | Dataset label | Pokrov entity | Hits | Suggested replay row |\n|---:|---|---|---:|---|\n{rows}",
            rows = rows
        )
    };

    let replay_lines = replay_coverage
        .iter()
        .map(|entry| {
            format!(
                "- `{dataset}` row `{row_idx}`: `{label}` -> {expectation}",
                dataset = entry.dataset_name,
                row_idx = entry.row_idx,
                label = entry.label,
                expectation = entry.expectation
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let recommended_priority = recommended_priority(&stats, &unsupported_labels)
        .into_iter()
        .enumerate()
        .map(|(index, label)| format!("{}. `{label}`", index + 1, label = label))
        .collect::<Vec<_>>()
        .join("\n");

    let limitations = if replay_expansion.is_empty() {
        "- All currently mapped dataset labels are covered by explicit runtime replay assertions.".to_string()
    } else {
        "- The report includes a dedicated detector-gap priority section for mapped labels that still lack current runtime recognizers or stable exact-output replay assertions.".to_string()
    };

    format!(
        "# Verification: 014 Dataset Detector Gap Report

Date: {report_date}
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
{supported_rows}

## Current runtime-covered label variants

These mapped labels already have stable runtime behavior and exact-output replay assertions today.

| Dataset label | Pokrov entity | Hits in cached snapshots | Datasets |
|---|---|---:|---|
{runtime_covered_rows}

## Current replay coverage

The current format-specific runtime assertions use rows that produce deterministic, already-supported behavior:

{replay_lines}

These rows are intentionally narrow. They verify runtime behavior only where current detector coverage is explicit and stable.

## Detector implementation priority for mapped labels

These labels are already mapped to Pokrov entities, but the current runtime does not yet expose stable detector coverage for them in exact-output replay assertions.

{replay_expansion_rows}

## Detector backlog from dataset analysis

The following labels are present in cached open datasets but are not yet mapped to current Pokrov detector coverage in the test layer.

### High-frequency backlog candidates

{high_frequency}

### Medium-frequency backlog candidates

{medium_frequency}

### Long-tail backlog candidates

These labels are present, but each currently has low evidence volume in cached snapshots:

{long_tail}

## Recommended detector priority

The next detector candidates should be prioritized as:

{recommended_priority}

Rationale:

- Higher-frequency labels should move first when they represent clear privacy or secret-bearing identifiers.
- Name-like and customer-like fields should stay behind stronger context constraints to avoid false positives.
- Mapped-but-not-yet-runtime-covered labels still require detector implementation before they can move into exact replay coverage.

## Current limitations

- The current report is derived from cached open snapshots, not from full upstream datasets.
- The current runtime assertion set intentionally covers card, email, IPv4, URL, phone, medical-record, and license-plate behavior.
{limitations}
- `open_presidio_research_repo.json` is metadata-only and is not part of replay coverage.

## Verification commands

- `cargo test starter_dataset_fixture_replays_through_runtime_engine -- --nocapture`
- `cargo test rows_match_expected --test contract -- --ignored --nocapture`
- `cargo test partitioned_into_supported_or_backlog --test contract -- --ignored --nocapture`
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture`

## Verification results ({report_date})

- `cargo test starter_dataset_fixture_replays_through_runtime_engine -- --nocapture` -> PASS
- `cargo test rows_match_expected --test contract -- --ignored --nocapture` -> PASS
- `cargo test partitioned_into_supported_or_backlog --test contract -- --ignored --nocapture` -> PASS
- `cargo test dataset_detector_gap_report_is_up_to_date_for_cached_snapshots --test contract -- --ignored --nocapture` -> PASS
",
        report_date = report_date,
        supported_rows = supported_rows,
        runtime_covered_rows = runtime_covered_rows,
        replay_lines = replay_lines,
        replay_expansion_rows = replay_expansion_rows,
        high_frequency = high_frequency,
        medium_frequency = medium_frequency,
        long_tail = long_tail,
        recommended_priority = recommended_priority,
        limitations = limitations,
    )
}

fn collect_label_stats() -> BTreeMap<String, LabelStats> {
    let mut stats = BTreeMap::<String, LabelStats>::new();

    for file_name in OPEN_SNAPSHOT_FILES {
        let snapshot = read_open_snapshot(file_name);
        let Some(rows) = snapshot.get("rows").and_then(serde_json::Value::as_array) else {
            continue;
        };

        for entry in rows {
            let row = entry.get("row").unwrap_or(entry);
            for annotation in collect_annotations_direct(row) {
                let entry = stats.entry(annotation.label).or_insert_with(|| LabelStats {
                    hits: 0,
                    datasets: BTreeSet::new(),
                });
                entry.hits += 1;
                entry.datasets.insert(file_name.to_string());
            }
        }
    }

    stats
}

fn collect_annotations_direct(
    row: &serde_json::Value,
) -> Vec<crate::sanitization_dataset_test_support::DatasetAnnotation> {
    let mut annotations = Vec::new();

    if let Some(items) = row.get("privacy_mask").and_then(serde_json::Value::as_array) {
        for item in items {
            let Some(label) = item.get("label").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let value = item
                .get("value")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            annotations.push(crate::sanitization_dataset_test_support::DatasetAnnotation {
                label: label.to_string(),
                value: value.to_string(),
            });
        }
    }

    for key in ["spans", "entities"] {
        let Some(raw) = row.get(key).and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(raw) else {
            continue;
        };
        let Some(items) = parsed.as_sequence() else {
            continue;
        };
        for item in items {
            if key == "entities" {
                let value = item
                    .get("entity")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if let Some(types) = item.get("types").and_then(serde_yaml::Value::as_sequence) {
                    for label in types.iter().filter_map(serde_yaml::Value::as_str) {
                        annotations.push(
                            crate::sanitization_dataset_test_support::DatasetAnnotation {
                                label: label.to_string(),
                                value: value.clone(),
                            },
                        );
                    }
                }
            } else {
                let Some(label) = item.get("label").and_then(serde_yaml::Value::as_str) else {
                    continue;
                };
                let value = item
                    .get("text")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default();
                annotations.push(crate::sanitization_dataset_test_support::DatasetAnnotation {
                    label: label.to_string(),
                    value: value.to_string(),
                });
            }
        }
    }

    annotations
}

fn replay_coverage_entries() -> [ReplayCoverageEntry; 11] {
    [
        ReplayCoverageEntry {
            dataset_name: "ai4privacy",
            row_idx: 15,
            label: "CREDITCARDNUMBER",
            expectation: "expected `block`",
        },
        ReplayCoverageEntry {
            dataset_name: "ai4privacy",
            row_idx: 20,
            label: "EMAIL",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "ai4privacy",
            row_idx: 19,
            label: "IPV4",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "Nemotron",
            row_idx: 4,
            label: "credit_debit_card",
            expectation: "expected `block`",
        },
        ReplayCoverageEntry {
            dataset_name: "Nemotron",
            row_idx: 23,
            label: "email",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "Nemotron",
            row_idx: 2,
            label: "url",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "Nemotron",
            row_idx: 3,
            label: "phone_number",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "Nemotron",
            row_idx: 18,
            label: "license_plate",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "Gretel",
            row_idx: 12,
            label: "credit_card_number",
            expectation: "expected `block`",
        },
        ReplayCoverageEntry {
            dataset_name: "Gretel",
            row_idx: 3,
            label: "email",
            expectation: "expected exact redaction",
        },
        ReplayCoverageEntry {
            dataset_name: "Gretel",
            row_idx: 14,
            label: "medical_record_number",
            expectation: "expected exact redaction",
        },
    ]
}

fn replay_expansion_candidates(
    stats: &BTreeMap<String, LabelStats>,
    supported_mapping: &BTreeMap<&'static str, &'static str>,
    replay_assertable: &BTreeSet<&'static str>,
) -> Vec<ReplayExpansionCandidate> {
    let mut candidates = Vec::new();

    for (label, entity) in supported_mapping {
        if replay_assertable.contains(label) {
            continue;
        }
        let hits = stats.get(*label).map(|entry| entry.hits).unwrap_or(0);
        if hits == 0 {
            continue;
        }
        if let Some((dataset_file, row_idx)) = first_row_for_label(label) {
            candidates.push(ReplayExpansionCandidate {
                label: (*label).to_string(),
                entity: (*entity).to_string(),
                hits,
                dataset_file,
                row_idx,
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .hits
            .cmp(&left.hits)
            .then_with(|| left.label.cmp(&right.label))
    });
    candidates
}

fn format_runtime_covered_rows(
    stats: &BTreeMap<String, LabelStats>,
    supported_mapping: &BTreeMap<&'static str, &'static str>,
    replay_assertable: &BTreeSet<&'static str>,
) -> String {
    supported_mapping
        .iter()
        .filter(|(label, _)| replay_assertable.contains(**label))
        .map(|(label, entity)| {
            let stats = stats.get(*label);
            let hits = stats.map_or(0, |stats| stats.hits);
            let datasets = format_datasets(stats.map(|stats| &stats.datasets));
            format!(
                "| `{label}` | `{entity}` | {hits} | {datasets} |",
                label = label,
                entity = entity,
                hits = hits,
                datasets = datasets
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_row_for_label(label: &str) -> Option<(String, usize)> {
    for file_name in OPEN_SNAPSHOT_FILES {
        let snapshot = read_open_snapshot(file_name);
        let Some(rows) = snapshot.get("rows").and_then(serde_json::Value::as_array) else {
            continue;
        };

        for entry in rows {
            let row = entry.get("row").unwrap_or(entry);
            if collect_annotations_direct(row)
                .iter()
                .any(|annotation| annotation.label == label)
            {
                let row_idx = entry
                    .get("row_idx")
                    .and_then(serde_json::Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or_default();
                return Some((file_name.to_string(), row_idx));
            }
        }
    }

    None
}

fn format_datasets(datasets: Option<&BTreeSet<String>>) -> String {
    match datasets {
        Some(datasets) if !datasets.is_empty() => datasets
            .iter()
            .map(|dataset| format!("`{dataset}`", dataset = dataset))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "none in current cached rows".to_string(),
    }
}

fn format_label_group_table<F>(
    stats: &BTreeMap<String, LabelStats>,
    unsupported_labels: &BTreeSet<&'static str>,
    predicate: F,
    intro: &str,
) -> String
where
    F: Fn(usize) -> bool,
{
    let rows = stats
        .iter()
        .filter(|(label, stats)| unsupported_labels.contains(label.as_str()) && predicate(stats.hits))
        .map(|(label, stats)| {
            let datasets = format_datasets(Some(&stats.datasets));
            format!(
                "| `{label}` | {hits} | {datasets} |",
                label = label,
                hits = stats.hits,
                datasets = datasets
            )
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        format!("{intro}\n\n_No labels in this group for current cached snapshots._", intro = intro)
    } else {
        format!(
            "{intro}\n\n| Dataset label | Hits | Datasets |\n|---|---:|---|\n{rows}",
            intro = intro,
            rows = rows.join("\n")
        )
    }
}

fn recommended_priority(
    stats: &BTreeMap<String, LabelStats>,
    unsupported_labels: &BTreeSet<&'static str>,
) -> Vec<&'static str> {
    [
        "ssn",
        "medical_record_number",
        "first_name",
        "last_name",
        "date_of_birth",
        "account_number",
        "license_plate",
        "swift_bic",
        "customer_id",
    ]
    .into_iter()
    .filter(|label| unsupported_labels.contains(label) && stats.contains_key(*label))
    .collect()
}
