use std::collections::BTreeMap;

use crate::types::{DetectionCategory, DetectionHit, PolicyAction, ResolvedSpan};

pub fn resolve_overlaps(mut hits: Vec<DetectionHit>) -> Vec<ResolvedSpan> {
    hits.sort_by(|left, right| {
        left.json_pointer
            .cmp(&right.json_pointer)
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| right.end.cmp(&left.end))
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.rule_id.cmp(&right.rule_id))
    });

    let mut resolved: Vec<ResolvedSpan> = Vec::new();

    for hit in hits {
        match resolved.last_mut() {
            None => resolved.push(to_span(hit, Vec::new())),
            Some(last) if last.json_pointer != hit.json_pointer || hit.start >= last.end => {
                resolved.push(to_span(hit, Vec::new()));
            }
            Some(last) => {
                let new_wins = compare_winner(
                    hit.action,
                    hit.priority,
                    &hit.rule_id,
                    last.effective_action,
                    last.priority,
                    &last.winning_rule_id,
                );

                if new_wins {
                    let mut suppressed = vec![last.winning_rule_id.clone()];
                    suppressed.extend(last.suppressed_rule_ids.iter().cloned());

                    let start = last.start.min(hit.start);
                    let end = last.end.max(hit.end);
                    *last = ResolvedSpan {
                        json_pointer: hit.json_pointer,
                        start,
                        end,
                        winning_rule_id: hit.rule_id,
                        category: hit.category,
                        effective_action: hit.action,
                        priority: hit.priority,
                        replacement_template: hit.replacement_template,
                        suppressed_rule_ids: suppressed,
                    };
                } else {
                    last.suppressed_rule_ids.push(hit.rule_id);
                    last.start = last.start.min(hit.start);
                    last.end = last.end.max(hit.end);
                }
            }
        }
    }

    resolved
}

pub fn select_final_action(resolved: &[ResolvedSpan]) -> PolicyAction {
    resolved
        .iter()
        .map(|span| span.effective_action)
        .max_by_key(|action| action.strictness_rank())
        .unwrap_or(PolicyAction::Allow)
}

pub fn category_hit_counts(hits: &[DetectionHit]) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for hit in hits {
        let key = category_to_key(hit.category).to_string();
        let entry = counts.entry(key).or_insert(0);
        *entry += 1;
    }
    counts
}

pub fn category_to_key(category: DetectionCategory) -> &'static str {
    match category {
        DetectionCategory::Secrets => "secrets",
        DetectionCategory::Pii => "pii",
        DetectionCategory::CorporateMarkers => "corporate_markers",
        DetectionCategory::Custom => "custom",
    }
}

fn to_span(hit: DetectionHit, suppressed_rule_ids: Vec<String>) -> ResolvedSpan {
    ResolvedSpan {
        json_pointer: hit.json_pointer,
        start: hit.start,
        end: hit.end,
        winning_rule_id: hit.rule_id,
        category: hit.category,
        effective_action: hit.action,
        priority: hit.priority,
        replacement_template: hit.replacement_template,
        suppressed_rule_ids,
    }
}

fn compare_winner(
    new_action: PolicyAction,
    new_priority: u16,
    new_rule_id: &str,
    old_action: PolicyAction,
    old_priority: u16,
    old_rule_id: &str,
) -> bool {
    new_action.strictness_rank() > old_action.strictness_rank()
        || (new_action == old_action
            && (new_priority > old_priority
                || (new_priority == old_priority && new_rule_id < old_rule_id)))
}

#[cfg(test)]
mod tests {
    use crate::types::{DetectionCategory, DetectionHit, PolicyAction, ResolvedSpan};

    use super::{category_hit_counts, resolve_overlaps, select_final_action};

    #[test]
    fn overlap_resolution_prefers_stricter_action_deterministically() {
        let hits = vec![
            DetectionHit {
                rule_id: "rule-a".to_string(),
                category: DetectionCategory::Pii,
                json_pointer: "/payload".to_string(),
                start: 0,
                end: 8,
                action: PolicyAction::Mask,
                priority: 100,
                replacement_template: None,
            },
            DetectionHit {
                rule_id: "rule-b".to_string(),
                category: DetectionCategory::Secrets,
                json_pointer: "/payload".to_string(),
                start: 2,
                end: 12,
                action: PolicyAction::Block,
                priority: 10,
                replacement_template: None,
            },
        ];

        let resolved = resolve_overlaps(hits);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].winning_rule_id, "rule-b");
        assert_eq!(resolved[0].effective_action, PolicyAction::Block);
    }

    #[test]
    fn final_action_uses_highest_precedence() {
        let resolved = vec![
            ResolvedSpan {
                json_pointer: "/a".to_string(),
                start: 0,
                end: 1,
                winning_rule_id: "a".to_string(),
                category: DetectionCategory::CorporateMarkers,
                effective_action: PolicyAction::Mask,
                priority: 1,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            },
            ResolvedSpan {
                json_pointer: "/a".to_string(),
                start: 2,
                end: 3,
                winning_rule_id: "b".to_string(),
                category: DetectionCategory::Secrets,
                effective_action: PolicyAction::Redact,
                priority: 1,
                replacement_template: None,
                suppressed_rule_ids: Vec::new(),
            },
        ];

        assert_eq!(select_final_action(&resolved), PolicyAction::Redact);
    }

    #[test]
    fn category_counts_are_stable() {
        let hits = vec![
            DetectionHit {
                rule_id: "x".to_string(),
                category: DetectionCategory::Pii,
                json_pointer: "/a".to_string(),
                start: 0,
                end: 1,
                action: PolicyAction::Mask,
                priority: 1,
                replacement_template: None,
            },
            DetectionHit {
                rule_id: "y".to_string(),
                category: DetectionCategory::Pii,
                json_pointer: "/a".to_string(),
                start: 2,
                end: 3,
                action: PolicyAction::Mask,
                priority: 1,
                replacement_template: None,
            },
        ];

        let counts = category_hit_counts(&hits);
        assert_eq!(counts.get("pii"), Some(&2));
    }
}
