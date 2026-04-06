use std::collections::HashMap;
use std::path::Path;

use tracing::info;

/// Loads the id-to-label mapping from a `config.json` file adjacent to the model.
/// Falls back to a hardcoded default with a warning if the file is absent or empty.
pub fn load_id2label(model_path: &Path) -> HashMap<usize, String> {
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

    tracing::warn!(
        "No config.json with id2label found at {}, using hardcoded default label mapping. \
         Predictions may be incorrect if the model uses different label indices.",
        config_path.display()
    );
    default_id2label()
}

pub fn default_id2label() -> HashMap<usize, String> {
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

/// Computes mean softmax probability for the tokens in a span.
/// Serves as the entity-level confidence score for threshold filtering.
pub fn span_token_confidence(
    logits: &ndarray::Array3<f32>,
    label_indices: &[usize],
    token_start: usize,
    token_end: usize,
) -> f32 {
    let (_, seq_len, num_labels) = logits.dim();
    let mut total_prob = 0.0f32;
    let mut count = 0usize;

    for t in token_start..=token_end.min(seq_len.saturating_sub(1)) {
        let max_exp =
            (0..num_labels).map(|l| logits[[0, t, l]].max(0.0)).reduce(f32::max).unwrap_or(1.0);
        let sum_exp: f32 = (0..num_labels).map(|l| (logits[[0, t, l]] - max_exp).exp()).sum();
        let chosen = label_indices[t].min(num_labels - 1);
        let prob = ((logits[[0, t, chosen]] - max_exp).exp()) / sum_exp;
        total_prob += prob;
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        total_prob / count as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_id2label_covers_per_org_loc() {
        let map = default_id2label();
        assert_eq!(map.get(&9), Some(&"B-PER".to_string()));
        assert_eq!(map.get(&7), Some(&"B-ORG".to_string()));
        assert_eq!(map.get(&5), Some(&"B-LOC".to_string()));
        assert_eq!(map.len(), 11);
    }
}
