use std::path::PathBuf;

use serde_json::Value;
use serde_yaml;

use pokrov_ner::{model::NerModelBinding, NerConfig, NerEngine, NerEntityType, NerHit};

const DATASET_DIR: &str = "tests/fixtures/eval/datasets/open-cache";
const DATASET_FILES: &[&str] = &[
    "open_gretel_pii_masking_en_v1.json",
    "open_nvidia_nemotron_pii.json",
    "open_ai4privacy_pii_masking_200k.json",
];

const PERSON_LABELS: &[&str] = &[
    "name",
    "first_name",
    "last_name",
    "middle_name",
    "user_name",
    "LASTNAME",
    "FIRSTNAME",
    "MIDDLENAME",
];

const ORG_LABELS: &[&str] = &["company_name"];

#[derive(Debug, Clone)]
struct GroundTruthSpan {
    char_start: usize,
    char_end: usize,
    category: SpanCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanCategory {
    Person,
    Organization,
}

#[derive(Debug, Clone, Default)]
struct EvalMetrics {
    tp: usize,
    fp: usize,
    fn_count: usize,
}

fn engine_for_languages(required_languages: &[&str]) -> Option<(NerEngine, Vec<NerModelBinding>)> {
    let default_config = NerConfig::default();
    let mut selected_models: Vec<NerModelBinding> = Vec::new();

    for &language in required_languages {
        let Some(binding) = default_config.models.iter().find(|binding| binding.language == language)
        else {
            eprintln!("Skipping NER dataset evaluation: language '{language}' is not configured");
            return None;
        };
        if !binding.model_path.exists() || !binding.tokenizer_path.exists() {
            eprintln!(
                "Skipping NER dataset evaluation: model assets for '{language}' are missing (model='{}', tokenizer='{}')",
                binding.model_path.display(),
                binding.tokenizer_path.display()
            );
            return None;
        }
        selected_models.push(binding.clone());
    }

    let config = NerConfig { models: selected_models.clone(), ..default_config };
    match NerEngine::new(config) {
        Ok(engine) => Some((engine, selected_models)),
        Err(error) => {
            eprintln!("Skipping NER dataset evaluation: NER engine init failed: {error}");
            None
        }
    }
}

fn dataset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join(DATASET_DIR)
}

fn read_dataset(file_name: &str) -> Value {
    let path = dataset_dir().join(file_name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

fn extract_ground_truth_spans(row: &Value) -> Vec<GroundTruthSpan> {
    let mut spans = Vec::new();

    if let Some(entities_raw) = row.get("entities").and_then(|v| v.as_str()) {
        if let Ok(items) = serde_yaml::from_str::<Vec<serde_yaml::Value>>(entities_raw) {
            for item in items {
                let entity = match item.get("entity").and_then(|v| v.as_str()) {
                    Some(e) => e.to_string(),
                    None => continue,
                };
                let types = item
                    .get("types")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter().filter_map(|t| t.as_str().map(String::from)).collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let category = if types.iter().any(|t| PERSON_LABELS.contains(&t.as_str())) {
                    Some(SpanCategory::Person)
                } else if types.iter().any(|t| ORG_LABELS.contains(&t.as_str())) {
                    Some(SpanCategory::Organization)
                } else {
                    None
                };

                if let Some(cat) = category {
                    if let Some(start) = row.get("text").and_then(|v| v.as_str()) {
                        if let Some(pos) = start.find(&entity) {
                            spans.push(GroundTruthSpan {
                                char_start: pos,
                                char_end: pos + entity.len(),
                                category: cat,
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(privacy_mask) = row.get("privacy_mask").and_then(|v| v.as_array()) {
        for item in privacy_mask {
            let label = match item.get("label").and_then(|v| v.as_str()) {
                Some(l) => l.to_string(),
                None => continue,
            };
            let value = match item.get("value").and_then(|v| v.as_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            let start = item.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let end = item.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            let category = if PERSON_LABELS.contains(&label.as_str()) {
                Some(SpanCategory::Person)
            } else if ORG_LABELS.contains(&label.as_str()) {
                Some(SpanCategory::Organization)
            } else {
                None
            };

            if let Some(cat) = category {
                let effective_end = if end > start { end } else { start + value.len() };
                spans.push(GroundTruthSpan {
                    char_start: start,
                    char_end: effective_end,
                    category: cat,
                });
            }
        }
    }

    if let Some(spans_raw) = row.get("spans").and_then(|v| v.as_str()) {
        if let Ok(items) = serde_yaml::from_str::<Vec<serde_yaml::Value>>(spans_raw) {
            for item in items {
                let label = match item.get("label").and_then(|v| v.as_str()) {
                    Some(l) => l.to_string(),
                    None => continue,
                };
                let text = match item.get("text").and_then(|v| v.as_str()) {
                    Some(t) => t.to_string(),
                    None => continue,
                };
                let start = item.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let end = item.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                let category = if PERSON_LABELS.contains(&label.as_str()) {
                    Some(SpanCategory::Person)
                } else if ORG_LABELS.contains(&label.as_str()) {
                    Some(SpanCategory::Organization)
                } else {
                    None
                };

                if let Some(cat) = category {
                    let effective_end = if end > start { end } else { start + text.len() };
                    spans.push(GroundTruthSpan {
                        char_start: start,
                        char_end: effective_end,
                        category: cat,
                    });
                }
            }
        }
    }

    spans
}

fn row_text(row: &Value) -> String {
    for key in &["text", "source_text", "content", "prompt", "sentence"] {
        if let Some(text) = row.get(key).and_then(|v| v.as_str()) {
            if !text.trim().is_empty() {
                return text.to_string();
            }
        }
    }
    String::new()
}

fn spans_overlap(a: &GroundTruthSpan, b: &NerHit) -> bool {
    if a.category == SpanCategory::Person && b.entity != NerEntityType::Person {
        return false;
    }
    if a.category == SpanCategory::Organization && b.entity != NerEntityType::Organization {
        return false;
    }
    let overlap_start = a.char_start.max(b.start);
    let overlap_end = a.char_end.min(b.end);
    let overlap = overlap_end.saturating_sub(overlap_start);
    if overlap == 0 {
        return false;
    }
    let gt_len = a.char_end.saturating_sub(a.char_start);
    let ner_len = b.end.saturating_sub(b.start);
    let min_len = gt_len.min(ner_len);
    min_len > 0 && overlap * 2 >= min_len
}

fn evaluate_rows(
    engine: &mut NerEngine,
    rows: &[Value],
    dataset_name: &str,
) -> (EvalMetrics, EvalMetrics, f64) {
    let mut person_metrics = EvalMetrics::default();
    let mut org_metrics = EvalMetrics::default();
    let mut total_latency_ms = 0.0;
    let mut total_rows = 0usize;

    for entry in rows.iter() {
        let row = entry.get("row").unwrap_or(entry);
        let text = row_text(row);
        if text.trim().is_empty() {
            continue;
        }

        let gt_spans = extract_ground_truth_spans(row);
        let gt_person: Vec<_> =
            gt_spans.iter().filter(|s| s.category == SpanCategory::Person).collect();
        let gt_org: Vec<_> =
            gt_spans.iter().filter(|s| s.category == SpanCategory::Organization).collect();

        let start = std::time::Instant::now();
        let ner_hits = engine
            .recognize(&text, &[NerEntityType::Person, NerEntityType::Organization])
            .unwrap_or_default();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        total_latency_ms += elapsed;
        total_rows += 1;

        for gt in &gt_person {
            let matched = ner_hits.iter().any(|hit| spans_overlap(gt, hit));
            if matched {
                person_metrics.tp += 1;
            } else {
                person_metrics.fn_count += 1;
            }
        }
        for gt in &gt_org {
            let matched = ner_hits.iter().any(|hit| spans_overlap(gt, hit));
            if matched {
                org_metrics.tp += 1;
            } else {
                org_metrics.fn_count += 1;
            }
        }

        for hit in &ner_hits {
            let person_match = gt_person.iter().any(|gt| spans_overlap(gt, hit));
            let org_match = gt_org.iter().any(|gt| spans_overlap(gt, hit));
            if !person_match && !org_match {
                if hit.entity == NerEntityType::Person {
                    person_metrics.fp += 1;
                } else {
                    org_metrics.fp += 1;
                }
            }
        }
    }

    let avg_latency = if total_rows > 0 { total_latency_ms / total_rows as f64 } else { 0.0 };

    println!("\n=== {dataset_name} ({total_rows} rows evaluated) ===");
    println!("  Avg latency: {:.1}ms/row", avg_latency);
    print_metrics("Person (PER)", &person_metrics);
    print_metrics("Organization (ORG)", &org_metrics);

    (person_metrics, org_metrics, avg_latency)
}

fn print_metrics(label: &str, m: &EvalMetrics) {
    let precision = if m.tp + m.fp > 0 { m.tp as f64 / (m.tp + m.fp) as f64 } else { f64::NAN };
    let recall =
        if m.tp + m.fn_count > 0 { m.tp as f64 / (m.tp + m.fn_count) as f64 } else { f64::NAN };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        f64::NAN
    };

    println!("  {label}:");
    println!(
        "    TP={:<4} FP={:<4} FN={:<4}  P={:.3}  R={:.3}  F1={:.3}",
        m.tp, m.fp, m.fn_count, precision, recall, f1
    );
}

#[test]
fn ner_dataset_evaluation() {
    let Some((mut engine, models)) = engine_for_languages(&["en"]) else {
        return;
    };

    println!("=== NER Dataset Evaluation ===");
    let model_list = models
        .iter()
        .map(|binding| {
            format!(
                "{}:{}",
                binding.language,
                binding.model_path.display()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    println!("Models: {model_list}");

    let mut total_person = EvalMetrics::default();
    let mut total_org = EvalMetrics::default();
    let mut total_latency = 0.0;
    let mut total_rows = 0usize;

    for file in DATASET_FILES {
        let dataset = read_dataset(file);
        let rows = dataset
            .get("rows")
            .and_then(|v| v.as_array())
            .map(|arr| arr.clone())
            .unwrap_or_default();

        let source_id = dataset.get("source_id").and_then(|v| v.as_str()).unwrap_or(file);

        let (p, o, avg) = evaluate_rows(&mut engine, &rows, source_id);
        total_person.tp += p.tp;
        total_person.fp += p.fp;
        total_person.fn_count += p.fn_count;
        total_org.tp += o.tp;
        total_org.fp += o.fp;
        total_org.fn_count += o.fn_count;
        total_latency += avg;
        total_rows += 1;
    }

    println!("\n=== AGGREGATE ({total_rows} datasets) ===");
    println!("  Avg dataset latency: {:.1}ms", total_latency);
    print_metrics("Person (PER) — total", &total_person);
    print_metrics("Organization (ORG) — total", &total_org);

    let min_recall = 0.50;
    let person_recall = if total_person.tp + total_person.fn_count > 0 {
        total_person.tp as f64 / (total_person.tp + total_person.fn_count) as f64
    } else {
        1.0
    };
    let org_recall = if total_org.tp + total_org.fn_count > 0 {
        total_org.tp as f64 / (total_org.tp + total_org.fn_count) as f64
    } else {
        1.0
    };

    assert!(
        person_recall >= min_recall,
        "Person recall {:.3} below minimum threshold {min_recall}",
        person_recall
    );
    assert!(
        org_recall >= min_recall,
        "Organization recall {:.3} below minimum threshold {min_recall}",
        org_recall
    );
}
