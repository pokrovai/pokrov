use std::path::Path;

use crate::error::PromptInjectionProviderError;

/// Label substrings that identify the positive (attack) class across the
/// supported model families. Matching is case-insensitive.
const POSITIVE_LABEL_MARKERS: &[&str] =
    &["injection", "jailbreak", "malicious", "unsafe", "attack"];

/// Resolves which logits index carries the injection probability.
///
/// Resolution order: explicit config override → `config.json` `id2label`
/// entry whose name matches a positive-class marker → hard error asking for
/// an explicit override. A wrong index silently inverts the verdict, so an
/// ambiguous label set must fail loudly rather than guess — even for binary
/// classifiers, where `LABEL_0`/`LABEL_1` give no hint which side is the
/// positive class.
pub fn resolve_injection_index(
    model_path: &Path,
    override_index: Option<u32>,
) -> Result<usize, PromptInjectionProviderError> {
    if let Some(index) = override_index {
        return Ok(index as usize);
    }

    let config_path =
        model_path.parent().unwrap_or_else(|| Path::new(".")).join("config.json");
    let labels = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|cfg| cfg.get("id2label").and_then(|v| v.as_object()).cloned())
        .map(|object| {
            object
                .iter()
                .filter_map(|(k, v)| {
                    k.parse::<usize>().ok().zip(v.as_str().map(String::from))
                })
                .collect::<Vec<(usize, String)>>()
        });

    let Some(labels) = labels.filter(|labels| !labels.is_empty()) else {
        return Err(PromptInjectionProviderError::LabelResolution(format!(
            "no usable id2label in {}; set provider.injection_label_index",
            config_path.display()
        )));
    };

    for (index, label) in &labels {
        let normalized = label.to_ascii_lowercase();
        if POSITIVE_LABEL_MARKERS.iter().any(|marker| normalized.contains(marker)) {
            return Ok(*index);
        }
    }

    Err(PromptInjectionProviderError::LabelResolution(format!(
        "cannot identify injection label among {} labels in {}; \
         set provider.injection_label_index",
        labels.len(),
        config_path.display()
    )))
}
