use std::collections::BTreeMap;

use serde_json::Value;

use crate::{
    traversal::map_string_leaves,
    types::{PolicyAction, ResolvedSpan, TransformResult},
};

pub fn apply_transforms(
    payload: &Value,
    resolved_spans: &[ResolvedSpan],
    final_action: PolicyAction,
    mask_visible_suffix: u8,
) -> TransformResult {
    if final_action == PolicyAction::Block {
        return TransformResult {
            final_action,
            sanitized_payload: None,
            blocked: true,
            transformed_fields_count: 0,
            transform_metadata: vec!["policy_block".to_string()],
        };
    }

    let mut spans_by_pointer: BTreeMap<&str, Vec<&ResolvedSpan>> = BTreeMap::new();
    for span in resolved_spans {
        spans_by_pointer.entry(span.json_pointer.as_str()).or_default().push(span);
    }

    for spans in spans_by_pointer.values_mut() {
        spans.sort_by(|left, right| left.start.cmp(&right.start).then_with(|| left.end.cmp(&right.end)));
    }

    let (sanitized_payload, transformed_fields_count) = map_string_leaves(payload, &mut |pointer, text| {
        let Some(spans) = spans_by_pointer.get(pointer) else {
            return text.to_string();
        };

        apply_spans(text, spans, mask_visible_suffix)
    });

    TransformResult {
        final_action,
        sanitized_payload: Some(sanitized_payload),
        blocked: false,
        transformed_fields_count,
        transform_metadata: if transformed_fields_count == 0 {
            vec!["pass_through".to_string()]
        } else {
            vec!["json_string_leaf_mutation".to_string()]
        },
    }
}

fn apply_spans(text: &str, spans: &[&ResolvedSpan], mask_visible_suffix: u8) -> String {
    if spans.is_empty() {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;

    for span in spans {
        if span.start > cursor {
            out.push_str(&text[cursor..span.start]);
        }

        let clamped_end = span.end.min(text.len());
        if clamped_end <= span.start {
            continue;
        }

        let fragment = &text[span.start..clamped_end];
        out.push_str(&transform_fragment(
            fragment,
            span.effective_action,
            span.replacement_template.as_deref(),
            mask_visible_suffix,
        ));
        cursor = clamped_end;
    }

    if cursor < text.len() {
        out.push_str(&text[cursor..]);
    }

    out
}

fn transform_fragment(
    fragment: &str,
    action: PolicyAction,
    replacement_template: Option<&str>,
    mask_visible_suffix: u8,
) -> String {
    match action {
        PolicyAction::Allow => fragment.to_string(),
        PolicyAction::Mask => mask_fragment(fragment, mask_visible_suffix as usize),
        PolicyAction::Replace => replacement_template.unwrap_or("[REPLACED]").to_string(),
        PolicyAction::Redact => "[REDACTED]".to_string(),
        PolicyAction::Block => "[BLOCKED]".to_string(),
    }
}

fn mask_fragment(fragment: &str, visible_suffix: usize) -> String {
    let mut chars = fragment.chars().collect::<Vec<_>>();
    let len = chars.len();
    let visible = visible_suffix.min(len);

    for ch in chars.iter_mut().take(len.saturating_sub(visible)) {
        if !ch.is_whitespace() {
            *ch = '*';
        }
    }

    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::types::{DetectionCategory, PolicyAction, ResolvedSpan};

    use super::apply_transforms;

    #[test]
    fn applies_mask_replace_and_redact_actions() {
        let payload = json!({"message": "api_key=secret-123 user@example.com"});
        let spans = vec![
            ResolvedSpan {
                json_pointer: "/message".to_string(),
                start: 8,
                end: 18,
                winning_rule_id: "secret".to_string(),
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Mask,
                priority: 1,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            },
            ResolvedSpan {
                json_pointer: "/message".to_string(),
                start: 19,
                end: 35,
                winning_rule_id: "email".to_string(),
                category: DetectionCategory::Pii,
                effective_action: PolicyAction::Redact,
                priority: 1,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            },
        ];

        let transformed = apply_transforms(&payload, &spans, PolicyAction::Redact, 4);

        assert!(!transformed.blocked);
        let text = transformed
            .sanitized_payload
            .as_ref()
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str())
            .expect("message should exist");
        assert_eq!(text, "api_key=******-123 [REDACTED]");
    }

    #[test]
    fn block_short_circuit_removes_payload() {
        let payload = json!({"message": "secret"});
        let transformed = apply_transforms(&payload, &[], PolicyAction::Block, 4);

        assert!(transformed.blocked);
        assert!(transformed.sanitized_payload.is_none());
    }
}
