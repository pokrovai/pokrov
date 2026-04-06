use std::collections::HashMap;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;
use tracing::{debug, info};

use crate::decode::{argmax_labels, decode_bio_tags};
use crate::error::NerError;
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
}

impl NerEngine {
    pub fn new(config: NerConfig) -> Result<Self, NerError> {
        if config.models.is_empty() {
            return Err(NerError::NoModelsConfigured);
        }

        let mut models: Vec<LoadedModel> = Vec::with_capacity(config.models.len());

        for binding in &config.models {
            let (session, tokenizer, id2label, has_token_type_ids) =
                Self::load_model(&binding.model_path, &binding.tokenizer_path, &binding.language)?;
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

        Ok(Self { models, fallback_language: config.fallback_language })
    }

    fn load_model(
        model_path: &Path,
        tokenizer_path: &Path,
        language: &str,
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
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| NerError::TokenizationFailed(e.to_string()))?;
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
            self.models.iter().position(|m| m.language == self.fallback_language).unwrap_or(0)
        })
    }

    pub fn recognize(
        &mut self,
        text: &str,
        entity_types: &[NerEntityType],
    ) -> Result<Vec<NerHit>, NerError> {
        let items = [text.to_string()];
        let results = self.recognize_batch(&items, entity_types)?;
        Ok(results.into_iter().next().unwrap_or_default().1)
    }

    pub fn recognize_batch(
        &mut self,
        texts: &[String],
        entity_types: &[NerEntityType],
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results: Vec<(String, Vec<NerHit>)> = Vec::with_capacity(texts.len());

        for chunk_start in (0..texts.len()).step_by(32) {
            let chunk_end = (chunk_start + 32).min(texts.len());
            let chunk = &texts[chunk_start..chunk_end];
            all_results.extend(self.recognize_batch_inner(chunk, entity_types)?);
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
    ) -> Result<Vec<(String, Vec<NerHit>)>, NerError> {
        let detected = detect_language(&texts[0]);
        let idx = self.select_model_index(&detected);
        let model = &mut self.models[idx];
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

        let mut results = Vec::with_capacity(batch_size);

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

            let offsets: Vec<(usize, usize)> = enc
                .get_offsets()
                .iter()
                .map(|&(start, end)| (start as usize, end as usize))
                .collect();

            let raw_spans = decode_bio_tags(&labels, &offsets, text);

            let mut hits = Vec::new();
            for span in raw_spans {
                if span.text.is_empty() {
                    continue;
                }
                if !allowed_labels.is_empty() && !allowed_labels.contains_key(&span.label) {
                    continue;
                }
                let entity =
                    allowed_labels.get(&span.label).copied().unwrap_or(NerEntityType::Person);
                hits.push(NerHit {
                    entity,
                    text: span.text,
                    start: span.char_start,
                    end: span.char_end,
                    score: 1.0,
                    language: language.clone(),
                });
            }

            results.push((texts[i].clone(), hits));
        }

        debug!(
            "Batch recognized {} entities across {} texts",
            results.iter().map(|(_, h)| h.len()).sum::<usize>(),
            batch_size
        );
        Ok(results)
    }
}

fn load_id2label(model_path: &Path) -> HashMap<usize, String> {
    let config_path = model_path.parent().unwrap_or_else(|| Path::new(".")).join("config.json");

    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(id2label) = cfg.get("id2label").and_then(|v| v.as_object()) {
                    let map: HashMap<usize, String> = id2label
                        .iter()
                        .filter_map(|(k, v)| {
                            k.parse::<usize>().ok().zip(v.as_str().map(String::from))
                        })
                        .collect();
                    if !map.is_empty() {
                        info!(
                            "Loaded id2label from {}: {} labels",
                            config_path.display(),
                            map.len()
                        );
                        return map;
                    }
                }
            }
        }
    }

    default_id2label()
}

fn default_id2label() -> HashMap<usize, String> {
    let mut map = HashMap::new();
    map.insert(0, "O".to_string());
    map.insert(9, "B-PER".to_string());
    map.insert(10, "I-PER".to_string());
    map.insert(7, "B-ORG".to_string());
    map.insert(8, "I-ORG".to_string());
    map.insert(5, "B-LOC".to_string());
    map.insert(6, "I-LOC".to_string());
    map.insert(1, "B-GEOPOLIT".to_string());
    map.insert(2, "I-GEOPOLIT".to_string());
    map.insert(3, "B-MEDIA".to_string());
    map.insert(4, "I-MEDIA".to_string());
    map
}

fn detect_language(text: &str) -> String {
    let mut cyrillic_count = 0usize;
    let mut ascii_count = 0usize;

    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            ascii_count += 1;
        } else if matches!(c, '\u{0400}'..='\u{04FF}') {
            cyrillic_count += 1;
        }
    }

    if cyrillic_count > ascii_count {
        "ru".to_string()
    } else if ascii_count > 0 {
        "en".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn language_detection_russian() {
        assert_eq!(detect_language("Иван Петров"), "ru");
    }

    #[test]
    fn language_detection_english() {
        assert_eq!(detect_language("John Smith"), "en");
    }

    #[test]
    fn language_detection_unknown_when_no_alpha() {
        assert_eq!(detect_language("   "), "unknown");
    }

    #[test]
    fn language_detection_prefers_cyrillic() {
        assert_eq!(detect_language("Привет John"), "ru");
    }

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
    fn default_id2label_covers_per_org_loc() {
        let map = default_id2label();
        assert_eq!(map.get(&9), Some(&"B-PER".to_string()));
        assert_eq!(map.get(&7), Some(&"B-ORG".to_string()));
        assert_eq!(map.get(&5), Some(&"B-LOC".to_string()));
        assert_eq!(map.len(), 11);
    }
}
