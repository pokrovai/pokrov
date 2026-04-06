use std::collections::HashMap;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{Tokenizer, TruncationParams};
use tracing::{debug, info};

use crate::decode::{argmax_labels, decode_bio_tags};
use crate::error::NerError;
use crate::label::{load_id2label, span_token_confidence};
use crate::lang::detect_language;
use crate::model::{NerConfig, NerEntityType, NerHit};

struct LoadedModel {
    language: String,
    priority: u16,
    session: Session,
    tokenizer: Tokenizer,
    id2label: HashMap<usize, String>,
    has_token_type_ids: bool,
}

pub struct NerEngine {
    models: Vec<LoadedModel>,
    fallback_language: String,
    confidence_threshold: f32,
}

impl NerEngine {
    pub fn new(config: NerConfig) -> Result<Self, NerError> {
        if config.models.is_empty() {
            return Err(NerError::NoModelsConfigured);
        }

        let mut models: Vec<LoadedModel> = Vec::with_capacity(config.models.len());

        for binding in &config.models {
            let (session, tokenizer, id2label, has_token_type_ids) =
                Self::load_model(
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
            "NerEngine initialized: {} models, fallback={}",
            models.len(),
            config.fallback_language
        );

        Ok(Self {
            models,
            fallback_language: config.fallback_language,
            confidence_threshold: config.confidence_threshold,
        })
    }

    fn default_threshold(&self) -> f32 {
        self.confidence_threshold
    }

    fn load_model(
        model_path: &Path,
        tokenizer_path: &Path,
        language: &str,
        max_seq_length: usize,
    ) -> Result<(Session, Tokenizer, HashMap<usize, String>, bool), NerError> {
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
                .with_truncation(Some(TruncationParams { max_length: max_seq_length, ..truncation }))
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

        Ok((session, tokenizer, id2label, has_token_type_ids))
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
            all_results.extend(self.recognize_batch_inner(
                chunk,
                entity_types,
                confidence_threshold,
            )?);
        }

        debug!(
            "Batch recognized {} entities across {} texts",
            all_results.iter().map(|(_, h)| h.len()).sum::<usize>(),
            texts.len()
        );
        Ok(all_results)
    }

    fn recognize_batch_inner(
        &mut self,
        texts: &[String],
        entity_types: &[NerEntityType],
        confidence_threshold: f32,
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        let mut all_results: Vec<(String, Vec<NerHit>)> = Vec::with_capacity(texts.len());

        let mut by_language: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, text) in texts.iter().enumerate() {
            let lang = detect_language(text);
            by_language.entry(lang).or_default().push(i);
        }

        for (lang, indices) in &by_language {
            let idx = self.select_model_index(lang);
            let model = &mut self.models[idx];
            let language = model.language.clone();

            let lang_texts: Vec<&String> = indices.iter().map(|&i| &texts[i]).collect();
            let lang_indices: Vec<usize> = indices.to_vec();

            let encodings: Vec<_> = lang_texts
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
            let attention_mask_tensor =
                Tensor::from_array(([batch_size, max_seq_len], attention_mask))
                    .map_err(|e| NerError::InferenceFailed(e.to_string()))?;

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
                model
                    .session
                    .run(ort::inputs![
                        "input_ids" => input_ids_tensor,
                        "attention_mask" => attention_mask_tensor,
                        "token_type_ids" => token_type_ids_tensor
                    ])
                    .map_err(|e| NerError::InferenceFailed(e.to_string()))?
            } else {
                model
                    .session
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

            for (local_i, enc) in encodings.iter().enumerate() {
                let seq_len = enc.get_ids().len();
                let text = &lang_texts[local_i];
                let original_idx = lang_indices[local_i];

                let seq_view = logits_array.index_axis(ndarray::Axis(0), local_i);
                let needed = seq_len * num_labels;
                let seq_data: Vec<f32> =
                    seq_view.to_slice().unwrap_or(&[]).get(0..needed).unwrap_or(&[]).to_vec();
                let seq_logits: ndarray::Array3<f32> =
                    ndarray::Array3::from_shape_vec((1, seq_len, num_labels), seq_data)
                        .map_err(|e| NerError::InferenceFailed(e.to_string()))?;
                let label_indices = argmax_labels(&seq_logits);

                let labels: Vec<String> = label_indices
                    .iter()
                    .map(|&idx| {
                        model.id2label.get(&idx).cloned().unwrap_or_else(|| "O".to_string())
                    })
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

                all_results
                    .resize(all_results.len().max(original_idx + 1), (String::new(), Vec::new()));
                all_results[original_idx] = (texts[original_idx].clone(), hits);
            }
        }

        debug!(
            "Batch recognized {} entities across {} texts",
            all_results.iter().map(|(_, h)| h.len()).sum::<usize>(),
            texts.len()
        );
        Ok(all_results)
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
}
