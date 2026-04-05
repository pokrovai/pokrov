/// Shared deterministic helpers reused by native and configured recognizers.
pub mod deterministic;
mod runtime_rules;

pub use runtime_rules::{compile_custom_rules, detect_payload, CompiledCustomRule};

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
    fn detects_high_confidence_bearer_style_secret_tokens() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Authorization: Bearer ghp_1234567890abcdef1234567890abcdef1234"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.secrets.bearer_token"),
            "bearer token should be detected as a secret family hit"
        );
        assert!(
            hits.iter().any(|hit| hit.category == DetectionCategory::Secrets),
            "bearer token should produce a secret-category hit"
        );
    }

    #[test]
    fn detects_standalone_sk_codex_api_key_tokens() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Use temporary key sk_codex_1234567890abcdef1234567890 for Codex test run"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.secrets.sk_api_key"),
            "sk_codex key should be detected by dedicated sk-api-key rule"
        );
        assert!(
            hits.iter().any(|hit| hit.category == DetectionCategory::Secrets),
            "sk_codex key should produce a secret-category hit"
        );
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
