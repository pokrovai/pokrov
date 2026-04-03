use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::{
    traversal::visit_string_leaves,
    types::{
        DetectionCategory, DetectionHit, EvaluateError, PolicyProfile,
    },
};

#[derive(Debug, Clone)]
pub struct CompiledCustomRule {
    pub rule_id: String,
    pub category: DetectionCategory,
    pub action: crate::types::PolicyAction,
    pub priority: u16,
    pub replacement_template: Option<String>,
    pub matcher: Regex,
}

#[derive(Debug)]
struct BuiltinRule {
    rule_id: &'static str,
    category: DetectionCategory,
    priority: u16,
    matcher: Regex,
}

const BUILTIN_RULES: [(&str, DetectionCategory, u16, &str); 5] = [
    (
        "builtin.secrets.openai_key",
        DetectionCategory::Secrets,
        500,
        r"(?i)sk-[a-z0-9-]{8,}",
    ),
    (
        "builtin.secrets.api_key_assignment",
        DetectionCategory::Secrets,
        450,
        r"(?i)api[_-]?key\s*[:=]\s*[a-z0-9_\-]{8,}",
    ),
    (
        "builtin.pii.email",
        DetectionCategory::Pii,
        320,
        r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
    ),
    (
        "builtin.pii.card_number",
        DetectionCategory::Pii,
        310,
        r"\b(?:\d[ -]*?){13,16}\b",
    ),
    (
        "builtin.corporate.project_name",
        DetectionCategory::CorporateMarkers,
        220,
        r"(?i)\bproject\s+[a-z][a-z0-9_-]{2,}\b",
    ),
];

pub fn compile_custom_rules(profile: &PolicyProfile) -> Result<Vec<CompiledCustomRule>, EvaluateError> {
    if !profile.custom_rules_enabled {
        return Ok(Vec::new());
    }

    profile
        .custom_rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| {
            let matcher = Regex::new(&rule.pattern).map_err(|error| {
                EvaluateError::InvalidProfileConfig(format!(
                    "profile '{}' custom rule '{}' regex is invalid: {error}",
                    profile.profile_id, rule.rule_id
                ))
            })?;

            Ok(CompiledCustomRule {
                rule_id: rule.rule_id.clone(),
                category: rule.category,
                action: rule.action,
                priority: rule.priority,
                replacement_template: rule.replacement_template.clone(),
                matcher,
            })
        })
        .collect()
}

pub fn detect_payload(
    payload: &Value,
    profile: &PolicyProfile,
    custom_rules: &[CompiledCustomRule],
) -> Vec<DetectionHit> {
    let builtin_rules = builtin_rules();
    let mut hits = Vec::new();

    visit_string_leaves(payload, &mut |json_pointer, text| {
        for rule in builtin_rules {
            for matched in rule.matcher.find_iter(text) {
                hits.push(DetectionHit {
                    rule_id: rule.rule_id.to_string(),
                    category: rule.category,
                    json_pointer: json_pointer.to_string(),
                    start: matched.start(),
                    end: matched.end(),
                    action: profile.category_actions.action_for(rule.category),
                    priority: rule.priority,
                    replacement_template: None,
                });
            }
        }

        for rule in custom_rules {
            for matched in rule.matcher.find_iter(text) {
                hits.push(DetectionHit {
                    rule_id: rule.rule_id.clone(),
                    category: rule.category,
                    json_pointer: json_pointer.to_string(),
                    start: matched.start(),
                    end: matched.end(),
                    action: rule.action,
                    priority: rule.priority,
                    replacement_template: rule.replacement_template.clone(),
                });
            }
        }
    });

    hits
}

fn builtin_rules() -> &'static [BuiltinRule] {
    static BUILTIN: OnceLock<Vec<BuiltinRule>> = OnceLock::new();
    BUILTIN
        .get_or_init(|| {
            BUILTIN_RULES
                .iter()
                .map(|(rule_id, category, priority, pattern)| BuiltinRule {
                    rule_id,
                    category: *category,
                    priority: *priority,
                    matcher: Regex::new(pattern).expect("built-in regex patterns must compile"),
                })
                .collect()
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::types::{
        CategoryActions, CustomRule, DetectionCategory, EvaluationMode, PolicyAction, PolicyProfile,
    };

    use super::{compile_custom_rules, detect_payload};

    fn strict_profile() -> PolicyProfile {
        PolicyProfile {
            profile_id: "strict".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Block,
                pii: PolicyAction::Redact,
                corporate_markers: PolicyAction::Mask,
                custom: PolicyAction::Redact,
            },
            mask_visible_suffix: 4,
            custom_rules_enabled: true,
            custom_rules: vec![CustomRule {
                rule_id: "custom.project_andromeda".to_string(),
                category: DetectionCategory::CorporateMarkers,
                pattern: "(?i)project\\s+andromeda".to_string(),
                action: PolicyAction::Redact,
                priority: 900,
                replacement_template: None,
                enabled: true,
            }],
        }
    }

    #[test]
    fn detects_built_in_and_custom_hits() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "contact user@example.com for Project Andromeda, token sk-test-12345678"
        });

        let hits = detect_payload(&payload, &profile, &custom);

        assert!(hits.iter().any(|hit| hit.category == DetectionCategory::Pii));
        assert!(hits.iter().any(|hit| hit.category == DetectionCategory::Secrets));
        assert!(hits.iter().any(|hit| hit.rule_id == "custom.project_andromeda"));
    }

    #[test]
    fn respects_deterministic_hit_sort_order_contract() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "project andromeda sk-test-00000000"});

        let mut hits = detect_payload(&payload, &profile, &custom);
        hits.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| right.end.cmp(&left.end))
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| left.rule_id.cmp(&right.rule_id))
        });

        let expected = hits.clone();
        hits.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| right.end.cmp(&left.end))
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| left.rule_id.cmp(&right.rule_id))
        });

        assert_eq!(hits, expected);
    }
}
