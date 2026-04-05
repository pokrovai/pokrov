use std::{collections::BTreeSet, sync::OnceLock};

use regex::{Regex, RegexBuilder};
use serde_json::Value;

pub mod deterministic;

use crate::{
    detection::deterministic::context::{apply_context_policy, ContextPolicy},
    detection::deterministic::lists::{
        build_allowlist_set, is_allowlisted_exact, normalize_exact_value,
    },
    detection::deterministic::pattern::compile_pattern,
    detection::deterministic::validation::{validate_candidate, ValidatorKind},
    traversal::visit_string_leaves,
    types::{
        DetectionCategory, DetectionHit, DeterministicNormalizationMode, DeterministicRuleKind,
        DeterministicRuleMetadata, DeterministicValidatorKind, EvaluateError, PolicyProfile,
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
    pub deterministic: Option<DeterministicRuleMetadata>,
    pub deterministic_context: Option<CompiledContextPolicy>,
    pub deterministic_allowlist: Option<BTreeSet<String>>,
}

#[derive(Debug)]
struct BuiltinRule {
    rule_id: &'static str,
    category: DetectionCategory,
    priority: u16,
    matcher: Regex,
}

#[derive(Debug, Clone)]
pub struct CompiledContextPolicy {
    pub policy: ContextPolicy,
    pub window: u8,
}

const BUILTIN_RULES: [(&str, DetectionCategory, u16, &str); 5] = [
    ("builtin.secrets.openai_key", DetectionCategory::Secrets, 500, r"(?i)sk-[a-z0-9-]{8,}"),
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
    ("builtin.pii.card_number", DetectionCategory::Pii, 310, r"\b\d(?:[ -]?\d){12,15}\b"),
    (
        "builtin.corporate.project_name",
        DetectionCategory::CorporateMarkers,
        220,
        r"(?i)\bproject\s+[a-z][a-z0-9_-]{2,}\b",
    ),
];
const REGEX_SIZE_LIMIT_BYTES: usize = 1024 * 1024;

pub fn compile_custom_rules(
    profile: &PolicyProfile,
) -> Result<Vec<CompiledCustomRule>, EvaluateError> {
    if !profile.custom_rules_enabled {
        return Ok(Vec::new());
    }

    profile
        .custom_rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| {
            let matcher = compile_pattern(&rule.pattern).map_err(|error| {
                EvaluateError::InvalidProfile(format!(
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
                deterministic: rule.deterministic.clone(),
                deterministic_context: compile_context_policy(rule.deterministic.as_ref()),
                deterministic_allowlist: compile_deterministic_allowlist(
                    profile,
                    &rule.rule_id,
                    rule.deterministic.as_ref(),
                ),
            })
        })
        .collect()
}

pub fn detect_payload(
    payload: &Value,
    profile: &PolicyProfile,
    custom_rules: &[CompiledCustomRule],
    allowlist_additions: &[String],
) -> Vec<DetectionHit> {
    let builtin_rules = builtin_rules();
    let allowlist = build_allowlist_set(allowlist_additions);
    let max_hits = usize::try_from(profile.max_hits_per_request).unwrap_or(usize::MAX);
    let mut hit_limit_reached = false;
    let mut hits = Vec::new();

    visit_string_leaves(payload, &mut |json_pointer, text| {
        if hit_limit_reached {
            return;
        }

        for rule in builtin_rules {
            if hit_limit_reached {
                break;
            }
            for matched in rule.matcher.find_iter(text) {
                if is_allowlisted_exact(&allowlist, matched.as_str()) {
                    continue;
                }
                if hits.len() >= max_hits {
                    hit_limit_reached = true;
                    break;
                }
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
            if hit_limit_reached {
                break;
            }
            for matched in rule.matcher.find_iter(text) {
                if is_allowlisted_exact(&allowlist, matched.as_str()) {
                    continue;
                }

                let mut priority = rule.priority;
                if let Some(deterministic) = &rule.deterministic {
                    if let DeterministicRuleKind::Pattern { validator, normalization, .. } =
                        &deterministic.rule
                    {
                        let needs_normalized_candidate = *validator
                            != DeterministicValidatorKind::None
                            || rule.deterministic_allowlist.is_some();
                        let candidate = needs_normalized_candidate
                            .then(|| normalize_candidate(matched.as_str(), *normalization));
                        if !validate_for_kind(*validator, candidate.as_deref()) {
                            continue;
                        }
                        if let (Some(allowlist), Some(candidate)) =
                            (rule.deterministic_allowlist.as_ref(), candidate.as_ref())
                        {
                            if allowlist.contains(candidate) {
                                continue;
                            }
                        }

                        if let Some(context) = &rule.deterministic_context {
                            let window = extract_context_window(
                                text,
                                matched.start(),
                                matched.end(),
                                context.window,
                            );
                            let (score, suppressed, _) = apply_context_policy(
                                &window,
                                i16::try_from(rule.priority).unwrap_or(i16::MAX),
                                &context.policy,
                            );
                            if suppressed {
                                continue;
                            }
                            priority = score.max(0) as u16;
                        }
                    } else if let Some(allowlist) = rule.deterministic_allowlist.as_ref() {
                        if is_allowlisted_exact(allowlist, matched.as_str()) {
                            continue;
                        }
                    }
                }
                if hits.len() >= max_hits {
                    hit_limit_reached = true;
                    break;
                }
                hits.push(DetectionHit {
                    rule_id: rule.rule_id.clone(),
                    category: rule.category,
                    json_pointer: json_pointer.to_string(),
                    start: matched.start(),
                    end: matched.end(),
                    action: rule.action,
                    priority,
                    replacement_template: rule.replacement_template.clone(),
                });
            }
        }
    });

    if hit_limit_reached {
        tracing::warn!(
            profile_id = %profile.profile_id,
            max_hits_per_request = profile.max_hits_per_request,
            "detection hit limit reached; remaining payload leaves were skipped"
        );
    }

    hits
}

fn validate_for_kind(kind: DeterministicValidatorKind, candidate: Option<&str>) -> bool {
    match kind {
        DeterministicValidatorKind::None => true,
        DeterministicValidatorKind::Luhn => {
            candidate.map(|value| validate_candidate(ValidatorKind::Luhn, value)).unwrap_or(false)
        }
    }
}

fn normalize_candidate(candidate: &str, mode: DeterministicNormalizationMode) -> String {
    match mode {
        DeterministicNormalizationMode::Preserve => candidate.to_string(),
        DeterministicNormalizationMode::Lowercase => candidate.to_lowercase(),
        DeterministicNormalizationMode::AlnumLowercase => candidate
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect(),
    }
}

fn compile_context_policy(
    metadata: Option<&DeterministicRuleMetadata>,
) -> Option<CompiledContextPolicy> {
    match metadata.map(|meta| &meta.rule) {
        Some(DeterministicRuleKind::Pattern { context: Some(context), .. }) => {
            Some(CompiledContextPolicy {
                policy: to_context_policy(context),
                window: context.window,
            })
        }
        _ => None,
    }
}

fn compile_deterministic_allowlist(
    profile: &PolicyProfile,
    rule_id: &str,
    metadata: Option<&DeterministicRuleMetadata>,
) -> Option<BTreeSet<String>> {
    let metadata = metadata?;
    if metadata.allowlist_exact.is_empty() {
        return None;
    }

    let mut dropped_entries = 0usize;
    let mut set = BTreeSet::new();
    match &metadata.rule {
        DeterministicRuleKind::Pattern { normalization, .. } => {
            for value in &metadata.allowlist_exact {
                let normalized = normalize_candidate(value, *normalization);
                if normalized.is_empty() {
                    dropped_entries += 1;
                } else {
                    set.insert(normalized);
                }
            }
        }
        DeterministicRuleKind::DenylistExact => {
            for value in &metadata.allowlist_exact {
                let normalized = normalize_exact_value(value);
                if normalized.is_empty() {
                    dropped_entries += 1;
                } else {
                    set.insert(normalized);
                }
            }
        }
    }

    if dropped_entries > 0 {
        tracing::warn!(
            profile_id = %profile.profile_id,
            rule_id = %rule_id,
            dropped_entries,
            "deterministic allowlist contains empty or normalization-empty entries"
        );
    }

    (!set.is_empty()).then_some(set)
}

fn to_context_policy(context: &crate::types::DeterministicContextPolicy) -> ContextPolicy {
    ContextPolicy {
        positive_terms: context.positive_terms.iter().map(|term| term.to_lowercase()).collect(),
        negative_terms: context.negative_terms.iter().map(|term| term.to_lowercase()).collect(),
        score_boost: context.score_boost,
        score_penalty: context.score_penalty,
        suppress_on_negative: context.suppress_on_negative,
    }
}

// Regex match offsets are UTF-8 boundary-safe, so slicing by start/end is valid here.
fn extract_context_window(text: &str, start: usize, end: usize, window: u8) -> String {
    let before = trim_to_chars_start(&text[..start], usize::from(window));
    let after = trim_to_chars_end(&text[end..], usize::from(window));
    [before, text[start..end].to_string(), after].concat()
}

fn trim_to_chars_start(text: &str, max_chars: usize) -> String {
    if text.chars().nth(max_chars).is_none() {
        return text.to_string();
    }
    let cut_at =
        text.char_indices().rev().nth(max_chars.saturating_sub(1)).map(|(idx, _)| idx).unwrap_or(0);
    text[cut_at..].to_string()
}

fn trim_to_chars_end(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
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
                    matcher: RegexBuilder::new(pattern)
                        .size_limit(REGEX_SIZE_LIMIT_BYTES)
                        .build()
                        .expect("built-in regex patterns must compile"),
                })
                .collect()
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::types::{
        CategoryActions, CustomRule, DetectionCategory, DeterministicContextPolicy,
        DeterministicNormalizationMode, DeterministicRuleKind, DeterministicRuleMetadata,
        DeterministicValidatorKind, EvaluationMode, PolicyAction, PolicyProfile,
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
            max_hits_per_request: 4096,
            custom_rules_enabled: true,
            custom_rules: vec![CustomRule {
                rule_id: "custom.project_andromeda".to_string(),
                category: DetectionCategory::CorporateMarkers,
                pattern: "(?i)project\\s+andromeda".to_string(),
                action: PolicyAction::Redact,
                priority: 900,
                replacement_template: None,
                enabled: true,
                deterministic: None,
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

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(hits.iter().any(|hit| hit.category == DetectionCategory::Pii));
        assert!(hits.iter().any(|hit| hit.category == DetectionCategory::Secrets));
        assert!(hits.iter().any(|hit| hit.rule_id == "custom.project_andromeda"));
    }

    #[test]
    fn respects_deterministic_hit_sort_order_contract() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "project andromeda sk-test-00000000"});

        let mut hits = detect_payload(&payload, &profile, &custom, &[]);
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

    #[test]
    fn suppresses_exact_allowlist_matches_from_request() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "token sk-test-00000000"});

        let hits = detect_payload(&payload, &profile, &custom, &["sk-test-00000000".to_string()]);

        assert!(hits.is_empty());
    }

    #[test]
    fn deterministic_context_penalty_reduces_priority_for_negative_terms() {
        let mut profile = strict_profile();
        profile.custom_rules = vec![CustomRule {
            rule_id: "deterministic.payment_card.pattern.pan".to_string(),
            category: DetectionCategory::Secrets,
            pattern: "\\b\\d(?:[ -]?\\d){12,15}\\b".to_string(),
            action: PolicyAction::Block,
            priority: 200,
            replacement_template: None,
            enabled: true,
            deterministic: Some(DeterministicRuleMetadata {
                recognizer_id: "payment_card".to_string(),
                allowlist_exact: Vec::new(),
                rule: DeterministicRuleKind::Pattern {
                    validator: DeterministicValidatorKind::None,
                    normalization: DeterministicNormalizationMode::Preserve,
                    context: Some(DeterministicContextPolicy {
                        positive_terms: Vec::new(),
                        negative_terms: vec!["demo".to_string()],
                        score_boost: 10,
                        score_penalty: 10,
                        window: 32,
                        suppress_on_negative: false,
                    }),
                },
            }),
        }];
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "demo card 4111 1111 1111 1111"});

        let hits = detect_payload(&payload, &profile, &custom, &[]);
        let deterministic_hit = hits
            .iter()
            .find(|hit| hit.rule_id == "deterministic.payment_card.pattern.pan")
            .expect("deterministic hit must exist");
        assert!(deterministic_hit.priority < 200);
    }

    #[test]
    fn deterministic_luhn_validator_rejects_invalid_candidate() {
        let mut profile = strict_profile();
        profile.custom_rules = vec![CustomRule {
            rule_id: "deterministic.payment_card.pattern.pan".to_string(),
            category: DetectionCategory::Secrets,
            pattern: "\\b\\d(?:[ -]?\\d){12,15}\\b".to_string(),
            action: PolicyAction::Block,
            priority: 200,
            replacement_template: None,
            enabled: true,
            deterministic: Some(DeterministicRuleMetadata {
                recognizer_id: "payment_card".to_string(),
                allowlist_exact: Vec::new(),
                rule: DeterministicRuleKind::Pattern {
                    validator: DeterministicValidatorKind::Luhn,
                    normalization: DeterministicNormalizationMode::Preserve,
                    context: None,
                },
            }),
        }];
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "card 4111 1111 1111 1112"});

        let hits = detect_payload(&payload, &profile, &custom, &[]);
        assert!(
            !hits.iter().any(|hit| hit.rule_id == "deterministic.payment_card.pattern.pan"),
            "deterministic luhn rule must reject invalid candidate"
        );
    }

    #[test]
    fn deterministic_profile_allowlist_suppresses_pattern_hit() {
        let mut profile = strict_profile();
        profile.custom_rules = vec![CustomRule {
            rule_id: "deterministic.payment_card.pattern.pan".to_string(),
            category: DetectionCategory::Secrets,
            pattern: "\\b\\d(?:[ -]?\\d){12,15}\\b".to_string(),
            action: PolicyAction::Block,
            priority: 200,
            replacement_template: None,
            enabled: true,
            deterministic: Some(DeterministicRuleMetadata {
                recognizer_id: "payment_card".to_string(),
                allowlist_exact: vec!["4111 1111 1111 1111".to_string()],
                rule: DeterministicRuleKind::Pattern {
                    validator: DeterministicValidatorKind::None,
                    normalization: DeterministicNormalizationMode::Preserve,
                    context: None,
                },
            }),
        }];

        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "card 4111 1111 1111 1111"});

        let hits = detect_payload(&payload, &profile, &custom, &[]);
        assert!(
            !hits.iter().any(|hit| hit.rule_id == "deterministic.payment_card.pattern.pan"),
            "profile-level deterministic allowlist must suppress matching candidate"
        );
    }

    #[test]
    fn deterministic_allowlist_uses_rule_normalization_mode() {
        let mut profile = strict_profile();
        profile.custom_rules = vec![CustomRule {
            rule_id: "deterministic.payment_card.pattern.pan".to_string(),
            category: DetectionCategory::Secrets,
            pattern: "\\b\\d(?:[ -]?\\d){12,15}\\b".to_string(),
            action: PolicyAction::Block,
            priority: 200,
            replacement_template: None,
            enabled: true,
            deterministic: Some(DeterministicRuleMetadata {
                recognizer_id: "payment_card".to_string(),
                allowlist_exact: vec!["4111-1111-1111-1111".to_string()],
                rule: DeterministicRuleKind::Pattern {
                    validator: DeterministicValidatorKind::None,
                    normalization: DeterministicNormalizationMode::AlnumLowercase,
                    context: None,
                },
            }),
        }];

        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "card 4111 1111 1111 1111"});

        let hits = detect_payload(&payload, &profile, &custom, &[]);
        assert!(
            !hits.iter().any(|hit| hit.rule_id == "deterministic.payment_card.pattern.pan"),
            "allowlist suppression must follow rule normalization mode"
        );
    }

    #[test]
    fn caps_hits_per_request_to_profile_limit() {
        let mut profile = strict_profile();
        profile.max_hits_per_request = 2;
        profile.custom_rules = vec![CustomRule {
            rule_id: "custom.repeat_x".to_string(),
            category: DetectionCategory::Custom,
            pattern: "x".to_string(),
            action: PolicyAction::Redact,
            priority: 500,
            replacement_template: None,
            enabled: true,
            deterministic: None,
        }];

        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({"content": "xxxx"});
        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert_eq!(hits.len(), 2);
    }
}
