use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{Tokenizer, TruncationParams};
use tracing::{debug, info};

use crate::decode::{argmax_labels, decode_bio_tags};
use crate::error::NerError;
use crate::label::{load_id2label, span_token_confidence};
use crate::lang::detect_language;
use crate::model::{NerConfig, NerEntityType, NerExecutionMode, NerHit, NerMergeStrategy};

struct LoadedModel {
    language: String,
    priority: u16,
    // `ort::Session::run` requires `&mut self` in 2.0.0-rc.12.
    // Wrapping in `Mutex` allows concurrent access from parallel multi-model threads
    // (each thread locks a different model — no cross-model contention).
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    id2label: HashMap<usize, String>,
    has_token_type_ids: bool,
}

pub struct NerEngine {
    models: Vec<LoadedModel>,
    default_language: String,
    fallback_language: String,
    confidence_threshold: f32,
    execution: NerExecutionMode,
    merge_strategy: NerMergeStrategy,
}

impl NerEngine {
    pub fn new(config: NerConfig) -> Result<Self, NerError> {
        if config.models.is_empty() {
            return Err(NerError::NoModelsConfigured);
        }

        let mut models: Vec<LoadedModel> = Vec::with_capacity(config.models.len());

        for binding in &config.models {
            let (session, tokenizer, id2label, has_token_type_ids) = Self::load_model(
                &binding.model_path,
                &binding.tokenizer_path,
                &binding.language,
                config.max_seq_length,
            )?;
            models.push(LoadedModel {
                language: binding.language.clone(),
                priority: binding.priority,
                session,
                tokenizer,
                id2label,
                has_token_type_ids,
            });
        }

        models.sort_by(|a, b| b.priority.cmp(&a.priority));

        info!(
            "NerEngine initialized: {} models, execution={:?}, default_language={}, fallback={}",
            models.len(),
            config.execution,
            if config.default_language.is_empty() { "(auto)" } else { &config.default_language },
            config.fallback_language
        );

        Ok(Self {
            models,
            default_language: config.default_language,
            fallback_language: config.fallback_language,
            confidence_threshold: config.confidence_threshold,
            execution: config.execution,
            merge_strategy: config.merge_strategy,
        })
    }

    pub fn execution_mode(&self) -> NerExecutionMode {
        self.execution
    }

    fn default_threshold(&self) -> f32 {
        self.confidence_threshold
    }

    fn load_model(
        model_path: &Path,
        tokenizer_path: &Path,
        language: &str,
        max_seq_length: usize,
    ) -> Result<(Mutex<Session>, Tokenizer, HashMap<usize, String>, bool), NerError> {
        if !model_path.exists() {
            return Err(NerError::ModelNotFound { path: model_path.to_path_buf() });
        }
        if !tokenizer_path.exists() {
            return Err(NerError::TokenizerNotFound { path: tokenizer_path.to_path_buf() });
        }

        let mut builder = Session::builder().map_err(|e| NerError::SessionInit(e.to_string()))?;
        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| NerError::SessionInit(e.to_string()))?;
        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| NerError::TokenizationFailed(e.to_string()))?;
        if max_seq_length > 0 {
            let mut truncation = tokenizer.get_truncation().cloned().unwrap_or_default();
            truncation.max_length = max_seq_length;
            tokenizer
                .with_truncation(Some(TruncationParams {
                    max_length: max_seq_length,
                    ..truncation
                }))
                .map_err(|e| NerError::TokenizationFailed(e.to_string()))?;
        }
        let id2label = load_id2label(model_path);
        let has_token_type_ids = session.inputs().iter().any(|inp| inp.name() == "token_type_ids");

        info!(
            "NER model [{}]: {}, labels={}, token_type_ids={}",
            language,
            model_path.display(),
            id2label.len(),
            has_token_type_ids
        );

        Ok((Mutex::new(session), tokenizer, id2label, has_token_type_ids))
    }

    fn select_model_index(&self, detected: &str) -> usize {
        let mut best: Option<usize> = None;
        let mut best_priority: u16 = 0;

        for (i, m) in self.models.iter().enumerate() {
            if m.language == detected && m.priority > best_priority {
                best_priority = m.priority;
                best = Some(i);
            }
        }

        best.unwrap_or_else(|| {
            let fallback_idx =
                self.models.iter().position(|m| m.language == self.fallback_language);
            match fallback_idx {
                Some(idx) => idx,
                None => {
                    tracing::warn!(
                        "NER fallback language '{}' matches no loaded model, using model at index 0",
                        self.fallback_language
                    );
                    0
                }
            }
        })
    }

    pub fn recognize(
        &mut self,
        text: &str,
        entity_types: &[NerEntityType],
    ) -> Result<Vec<NerHit>, NerError> {
        self.recognize_with_threshold(text, entity_types, self.default_threshold())
    }

    /// Recognizes entities with a custom confidence threshold override.
    pub fn recognize_with_threshold(
        &mut self,
        text: &str,
        entity_types: &[NerEntityType],
        confidence_threshold: f32,
    ) -> Result<Vec<NerHit>, NerError> {
        let items = [text.to_string()];
        let results =
            self.recognize_batch_with_threshold(&items, entity_types, confidence_threshold)?;
        Ok(results.into_iter().next().unwrap_or_default().1)
    }

    pub fn recognize_batch(
        &mut self,
        texts: &[String],
        entity_types: &[NerEntityType],
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        self.recognize_batch_with_threshold(texts, entity_types, self.default_threshold())
    }

    pub fn recognize_batch_with_threshold(
        &mut self,
        texts: &[String],
        entity_types: &[NerEntityType],
        confidence_threshold: f32,
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results: Vec<(String, Vec<NerHit>)> = Vec::with_capacity(texts.len());

        for chunk_start in (0..texts.len()).step_by(32) {
            let chunk_end = (chunk_start + 32).min(texts.len());
            let chunk = &texts[chunk_start..chunk_end];
            let chunk_results = match self.execution {
                NerExecutionMode::Auto => {
                    self.recognize_batch_inner_auto(chunk, entity_types, confidence_threshold)?
                }
                NerExecutionMode::Sequential | NerExecutionMode::Parallel => {
                    self.recognize_batch_multi(chunk, entity_types, confidence_threshold)?
                }
            };
            all_results.extend(chunk_results);
        }

        debug!(
            "Batch recognized {} entities across {} texts",
            all_results.iter().map(|(_, h)| h.len()).sum::<usize>(),
            texts.len()
        );
        Ok(all_results)
    }

    // Auto mode: group texts by detected language, select one model per group.
    fn recognize_batch_inner_auto(
        &self,
        texts: &[String],
        entity_types: &[NerEntityType],
        confidence_threshold: f32,
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        let mut all_results: Vec<(String, Vec<NerHit>)> =
            vec![(String::new(), Vec::new()); texts.len()];

        let mut by_language: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, text) in texts.iter().enumerate() {
            let lang = if self.default_language.is_empty() {
                detect_language(text)
            } else {
                self.default_language.clone()
            };
            by_language.entry(lang).or_default().push(i);
        }

        for (lang, indices) in &by_language {
            let model_idx = self.select_model_index(lang);
            let lang_texts: Vec<String> = indices.iter().map(|&i| texts[i].clone()).collect();
            let model_hits = self.run_model_on_texts(
                model_idx,
                &lang_texts,
                entity_types,
                confidence_threshold,
            )?;

            for (local_i, &original_idx) in indices.iter().enumerate() {
                all_results[original_idx] =
                    (texts[original_idx].clone(), model_hits[local_i].clone());
            }
        }

        debug!(
            "Auto: {} entities across {} texts",
            all_results.iter().map(|(_, h)| h.len()).sum::<usize>(),
            texts.len()
        );
        Ok(all_results)
    }

    /// Core inference: run a single loaded model on a batch of texts.
    /// Returns per-text hit vectors aligned with the input order.
    fn run_model_on_texts(
        &self,
        model_idx: usize,
        texts: &[String],
        entity_types: &[NerEntityType],
        confidence_threshold: f32,
    ) -> Result<Vec<Vec<NerHit>>, NerError> {
        let model = &self.models[model_idx];
        let language = model.language.clone();

        let encodings: Vec<_> = texts
            .iter()
            .map(|text| {
                model
                    .tokenizer
                    .encode(text.as_str(), true)
                    .map_err(|e| NerError::TokenizationFailed(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let max_seq_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);
        let batch_size = encodings.len();
        let num_labels = model.id2label.len();

        let pad_id = model.tokenizer.get_padding().map(|p| p.pad_id).unwrap_or(0);
        let pad_type_id = model.tokenizer.get_padding().map(|p| p.pad_type_id).unwrap_or(0);

        let mut input_ids = vec![pad_id as i64; batch_size * max_seq_len];
        let mut attention_mask = vec![0i64; batch_size * max_seq_len];

        for (i, enc) in encodings.iter().enumerate() {
            let ids = enc.get_ids();
            let attn = enc.get_attention_mask();
            let offset = i * max_seq_len;
            for j in 0..ids.len() {
                input_ids[offset + j] = ids[j] as i64;
                attention_mask[offset + j] = attn[j] as i64;
            }
        }

        let input_ids_tensor = Tensor::from_array(([batch_size, max_seq_len], input_ids))
            .map_err(|e| NerError::InferenceFailed(e.to_string()))?;
        let attention_mask_tensor = Tensor::from_array(([batch_size, max_seq_len], attention_mask))
            .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let mut session_guard = model
            .session
            .lock()
            .map_err(|_| NerError::InferenceFailed("session lock poisoned".to_string()))?;

        let outputs = if model.has_token_type_ids {
            let mut token_type_ids = vec![pad_type_id as i64; batch_size * max_seq_len];
            for (i, enc) in encodings.iter().enumerate() {
                let ttids = enc.get_type_ids();
                let offset = i * max_seq_len;
                for j in 0..ttids.len() {
                    token_type_ids[offset + j] = ttids[j] as i64;
                }
            }
            let token_type_ids_tensor =
                Tensor::from_array(([batch_size, max_seq_len], token_type_ids))
                    .map_err(|e| NerError::InferenceFailed(e.to_string()))?;
            session_guard
                .run(ort::inputs![
                    "input_ids" => input_ids_tensor,
                    "attention_mask" => attention_mask_tensor,
                    "token_type_ids" => token_type_ids_tensor
                ])
                .map_err(|e| NerError::InferenceFailed(e.to_string()))?
        } else {
            session_guard
                .run(ort::inputs![
                    "input_ids" => input_ids_tensor,
                    "attention_mask" => attention_mask_tensor
                ])
                .map_err(|e| NerError::InferenceFailed(e.to_string()))?
        };

        let logits_tensor = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let logits_array: ndarray::Array3<f32> = ndarray::Array3::from_shape_vec(
            (batch_size, max_seq_len, num_labels),
            logits_tensor.1.to_vec(),
        )
        .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

        let allowed_labels: HashMap<String, NerEntityType> = entity_types
            .iter()
            .map(|et| {
                let label = match et {
                    NerEntityType::Person => "PER".to_string(),
                    NerEntityType::Organization => "ORG".to_string(),
                };
                (label, *et)
            })
            .collect();

        let mut per_text_hits: Vec<Vec<NerHit>> = Vec::with_capacity(texts.len());
        for (i, enc) in encodings.iter().enumerate() {
            let seq_len = enc.get_ids().len();
            let text = &texts[i];

            let seq_view = logits_array.index_axis(ndarray::Axis(0), i);
            let needed = seq_len * num_labels;
            let seq_data: Vec<f32> =
                seq_view.to_slice().unwrap_or(&[]).get(0..needed).unwrap_or(&[]).to_vec();
            let seq_logits: ndarray::Array3<f32> =
                ndarray::Array3::from_shape_vec((1, seq_len, num_labels), seq_data)
                    .map_err(|e| NerError::InferenceFailed(e.to_string()))?;
            let label_indices = argmax_labels(&seq_logits);

            let labels: Vec<String> = label_indices
                .iter()
                .map(|&idx| model.id2label.get(&idx).cloned().unwrap_or_else(|| "O".to_string()))
                .collect();

            let offsets: Vec<(usize, usize)> =
                enc.get_offsets().iter().map(|&(start, end)| (start, end)).collect();

            let raw_spans = decode_bio_tags(&labels, &offsets, text);

            let mut hits = Vec::new();
            for span in raw_spans {
                if span.text.is_empty() {
                    continue;
                }
                if !allowed_labels.is_empty() && !allowed_labels.contains_key(&span.label) {
                    continue;
                }

                let span_confidence = span_token_confidence(
                    &seq_logits,
                    &label_indices,
                    span.token_start,
                    span.token_end,
                );
                if span_confidence < confidence_threshold {
                    continue;
                }

                let entity =
                    allowed_labels.get(&span.label).copied().unwrap_or(NerEntityType::Person);
                hits.push(NerHit {
                    entity,
                    text: span.text,
                    start: span.byte_start,
                    end: span.byte_end,
                    score: span_confidence,
                    language: language.clone(),
                });
            }
            per_text_hits.push(hits);
        }

        Ok(per_text_hits)
    }

    /// Multi-model mode: run all loaded models on every text, then merge per-text results.
    fn recognize_batch_multi(
        &self,
        texts: &[String],
        entity_types: &[NerEntityType],
        confidence_threshold: f32,
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        let num_models = self.models.len();

        let model_results: Vec<Vec<Vec<NerHit>>> = match self.execution {
            NerExecutionMode::Parallel => {
                let this = &*self;
                std::thread::scope(|s| {
                    (0..num_models)
                        .map(|model_idx| {
                            s.spawn(move || {
                                this.run_model_on_texts(
                                    model_idx,
                                    texts,
                                    entity_types,
                                    confidence_threshold,
                                )
                            })
                            .join()
                            .unwrap_or_else(|_| {
                                Err(NerError::InferenceFailed(
                                    "parallel model thread panicked".to_string(),
                                ))
                            })
                            .unwrap_or_default()
                        })
                        .collect()
                })
            }
            NerExecutionMode::Sequential => (0..num_models)
                .map(|model_idx| {
                    self.run_model_on_texts(model_idx, texts, entity_types, confidence_threshold)
                })
                .collect::<Result<Vec<_>, _>>()?,
            NerExecutionMode::Auto => unreachable!(),
        };

        let all_results: Vec<(String, Vec<NerHit>)> = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let per_model: Vec<Vec<NerHit>> =
                    model_results.iter().map(|mr| mr.get(i).cloned().unwrap_or_default()).collect();
                (text.clone(), merge_hits(&per_model, self.merge_strategy))
            })
            .collect();

        debug!(
            "Multi-model ({:?}): {} entities across {} texts",
            self.execution,
            all_results.iter().map(|(_, h)| h.len()).sum::<usize>(),
            texts.len()
        );
        Ok(all_results)
    }
}

/// Merge hits from multiple models for a single text.
///
/// - `Union`: deduplicate by exact byte range, keep highest score per range.
/// - `HighestScore`: greedy non-overlapping selection, prefer highest score.
fn merge_hits(hits_per_model: &[Vec<NerHit>], strategy: NerMergeStrategy) -> Vec<NerHit> {
    let mut all: Vec<NerHit> = hits_per_model.iter().flatten().cloned().collect();
    if all.is_empty() {
        return all;
    }

    match strategy {
        NerMergeStrategy::Union => {
            // Deduplicate by exact byte range. On collision, keep highest score.
            all.sort_by(|a, b| match a.start.cmp(&b.start) {
                std::cmp::Ordering::Equal => match a.end.cmp(&b.end) {
                    std::cmp::Ordering::Equal => {
                        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    ord => ord,
                },
                ord => ord,
            });
            let mut result = Vec::new();
            for hit in all {
                let is_dup = result
                    .last()
                    .map_or(false, |last: &NerHit| last.start == hit.start && last.end == hit.end);
                if !is_dup {
                    result.push(hit);
                }
            }
            result
        }
        NerMergeStrategy::HighestScore => {
            all.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.start.cmp(&b.start))
            });
            let mut result = Vec::new();
            for hit in all {
                let overlaps =
                    result.iter().any(|k: &NerHit| hit.start < k.end && k.start < hit.end);
                if !overlaps {
                    result.push(hit);
                }
            }
            result.sort_by_key(|h| h.start);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn new_fails_on_missing_model() {
        let config = NerConfig {
            models: vec![crate::model::NerModelBinding {
                language: "en".to_string(),
                model_path: PathBuf::from("/nonexistent/model.onnx"),
                tokenizer_path: PathBuf::from("/nonexistent/tokenizer.json"),
                priority: 100,
            }],
            ..Default::default()
        };
        let result = NerEngine::new(config);
        assert!(matches!(result, Err(NerError::ModelNotFound { .. })));
    }

    #[test]
    fn new_fails_on_empty_models() {
        let config = NerConfig { models: vec![], ..Default::default() };
        let result = NerEngine::new(config);
        assert!(matches!(result, Err(NerError::NoModelsConfigured)));
    }

    #[test]
    fn merge_hits_union_deduplicates_by_range() {
        let hits_a = vec![NerHit {
            entity: NerEntityType::Person,
            text: "Alice".to_string(),
            start: 0,
            end: 5,
            score: 0.8,
            language: "en".to_string(),
        }];
        let hits_b = vec![NerHit {
            entity: NerEntityType::Person,
            text: "Alice".to_string(),
            start: 0,
            end: 5,
            score: 0.95,
            language: "en".to_string(),
        }];
        let merged = merge_hits(&[hits_a, hits_b], NerMergeStrategy::Union);
        assert_eq!(merged.len(), 1);
        assert!((merged[0].score - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn merge_hits_union_keeps_non_overlapping() {
        let hits_a = vec![NerHit {
            entity: NerEntityType::Person,
            text: "Alice".to_string(),
            start: 0,
            end: 5,
            score: 0.9,
            language: "en".to_string(),
        }];
        let hits_b = vec![NerHit {
            entity: NerEntityType::Person,
            text: "Bob".to_string(),
            start: 10,
            end: 13,
            score: 0.85,
            language: "ru".to_string(),
        }];
        let merged = merge_hits(&[hits_a, hits_b], NerMergeStrategy::Union);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_hits_highest_score_resolves_overlap() {
        let hits_a = vec![NerHit {
            entity: NerEntityType::Person,
            text: "Alice Bob".to_string(),
            start: 0,
            end: 9,
            score: 0.7,
            language: "en".to_string(),
        }];
        let hits_b = vec![
            NerHit {
                entity: NerEntityType::Person,
                text: "Alice".to_string(),
                start: 0,
                end: 5,
                score: 0.95,
                language: "en".to_string(),
            },
            NerHit {
                entity: NerEntityType::Person,
                text: "Bob".to_string(),
                start: 6,
                end: 9,
                score: 0.8,
                language: "en".to_string(),
            },
        ];
        let merged = merge_hits(&[hits_a, hits_b], NerMergeStrategy::HighestScore);
        // "Alice" (0.95) and "Bob" (0.8) both survive; "Alice Bob" (0.7) overlaps with both
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].text, "Alice");
        assert_eq!(merged[1].text, "Bob");
    }

    #[test]
    fn merge_hits_empty() {
        let merged = merge_hits(&[], NerMergeStrategy::Union);
        assert!(merged.is_empty());
    }
}
