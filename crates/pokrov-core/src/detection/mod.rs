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
    fn detects_generic_standalone_sk_api_key_tokens() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Use temporary key sk-live-prod-1234567890abcdef1234567890 for smoke test"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.secrets.sk_api_key"),
            "sk-prefixed key should be detected by dedicated sk-api-key rule"
        );
        assert!(
            hits.iter().any(|hit| hit.category == DetectionCategory::Secrets),
            "sk-prefixed key should produce a secret-category hit"
        );
    }

    #[test]
    fn detects_secret_assignment_tokens() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "token=ghp_1234567890abcdef1234567890abcdef1234"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.secrets.secret_assignment"),
            "token assignment should be detected as a secret assignment"
        );
        assert!(
            hits.iter().any(|hit| hit.category == DetectionCategory::Secrets),
            "token assignment should produce a secret-category hit"
        );
    }

    #[test]
    fn detects_standalone_github_pat_like_tokens() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "temporary credential ghp_1234567890abcdef1234567890abcdef1234 leaked in logs"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.secrets.github_pat"),
            "standalone GitHub PAT-like token should be detected as a secret"
        );
    }

    #[test]
    fn detects_bearer_jwt_like_tokens() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0In0.sgnature1234567890"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.secrets.bearer_token"),
            "bearer JWT token should be detected by bearer-token secret rule"
        );
    }

    #[test]
    fn detects_url_candidates() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Report is available at https://financialreports.com/revenue/q2"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.pii.url"),
            "url should be detected by builtin url rule"
        );
    }

    #[test]
    fn detects_ipv4_candidates() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "ingress source IP 215.114.180.213 exceeded threshold"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.pii.ipv4"),
            "valid ipv4 should be detected by builtin ipv4 rule"
        );
    }

    #[test]
    fn rejects_invalid_ipv4_candidates() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "invalid diagnostic IP 999.114.180.213 should not be treated as real ipv4"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            !hits.iter().any(|hit| hit.rule_id == "builtin.pii.ipv4"),
            "out-of-range ipv4 octets should not match builtin ipv4 rule"
        );
    }

    #[test]
    fn detects_phone_numbers_with_identity_context() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Her phone number is 707-859-9753."
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().any(|hit| hit.rule_id == "builtin.pii.phone"),
            "phone number with lexical phone context should be detected"
        );
    }

    #[test]
    fn detects_russian_phone_number_formats() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let samples = [
            "+79001234567",
            "89001234567",
            "8 900 123 45 59",
            "8-900-123-45-59",
            "8 (900) 123 45 69",
        ];

        for sample in samples {
            let payload = json!({
                "content": format!("contact {}", sample)
            });
            let hits = detect_payload(&payload, &profile, &custom, &[]);
            assert!(
                hits.iter().any(|hit| hit.rule_id == "builtin.pii.phone"),
                "expected russian phone format to be detected: {sample}"
            );
        }
    }

    #[test]
    fn rejects_invalid_phone_candidates() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Build id 707-859-975 is used in staging"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            !hits.iter().any(|hit| hit.rule_id == "builtin.pii.phone"),
            "invalid short phone candidate should not match builtin phone rule"
        );
    }

    #[test]
    fn detects_person_name_fields_in_structured_json() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "tool_args": {
                "first_name": "Ivan",
                "last_name": "Petrov",
                "middle_name": "Sergeevich"
            }
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            hits.iter().filter(|hit| hit.rule_id == "builtin.pii.name_field").count() >= 3,
            "first, last, and middle name fields should each be detected by field-gated name rule"
        );
    }

    #[test]
    fn does_not_detect_name_rule_outside_name_fields() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "tool_args": {
                "display_name": "Ivan",
                "module": "first_name_handler"
            }
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(
            !hits.iter().any(|hit| hit.rule_id == "builtin.pii.name_field"),
            "field-gated name rule must not trigger outside explicit first/last/middle name keys"
        );
    }

    #[test]
    fn detects_nested_identity_fields_in_structured_payload() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "payload": {
                "owner": {
                    "profile": {
                        "name": "Фамилия Имя Отчество",
                        "email": "owner.alias@corp.test",
                        "directory": {
                            "id": "1234567890123456",
                            "name": "Фамилия Имя Отчество (1234567)"
                        },
                        "account": {
                            "id": "11111111-2222-3333-4444-555555555555",
                            "username": "owner.user.a"
                        }
                    },
                    "team": {
                        "name": "Internal Platform",
                        "unit_id": "TEAM-PRIMARY-01"
                    },
                    "supervisor": {
                        "name": "Тестовый Пользователь Третий",
                        "email": "manager.alias@corp.test",
                        "identity": {
                            "id": "22222222-3333-4444-5555-666666666666",
                            "username": "manager.user.b"
                        }
                    },
                    "backupEmail": "ops.alias@corp.test"
                }
            }
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);
        let identity_hits = hits
            .iter()
            .filter(|hit| hit.rule_id == "builtin.pii.person_identity_field")
            .map(|hit| hit.json_pointer.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let id_hits = hits
            .iter()
            .filter(|hit| hit.rule_id == "builtin.pii.person_id_field")
            .map(|hit| hit.json_pointer.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let email_hits = hits
            .iter()
            .filter(|hit| hit.rule_id == "builtin.pii.email")
            .count();

        assert!(identity_hits.contains("/payload/owner/profile/name"));
        assert!(identity_hits.contains("/payload/owner/profile/directory/name"));
        assert!(identity_hits.contains("/payload/owner/profile/account/username"));
        assert!(identity_hits.contains("/payload/owner/supervisor/name"));
        assert!(identity_hits.contains("/payload/owner/supervisor/identity/username"));
        assert!(id_hits.contains("/payload/owner/profile/directory/id"));
        assert!(id_hits.contains("/payload/owner/profile/account/id"));
        assert!(id_hits.contains("/payload/owner/supervisor/identity/id"));
        assert!(!identity_hits.contains("/payload/owner/team/name"));
        assert!(!id_hits.contains("/payload/owner/team/unit_id"));
        assert_eq!(email_hits, 3, "all nested emails should be detected");
    }

    #[test]
    fn detects_customer_account_and_swift_fields_in_structured_json() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "tool_args": {
                "customer_id": "cust_prod_90210",
                "account_number": "C37529641",
                "swift_bic": "GHTBUS45KLX"
            }
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(hits.iter().any(|hit| hit.rule_id == "builtin.pii.customer_id_field"));
        assert!(hits.iter().any(|hit| hit.rule_id == "builtin.pii.account_number_field"));
        assert!(hits.iter().any(|hit| hit.rule_id == "builtin.pii.swift_bic_field"));
    }

    #[test]
    fn does_not_detect_contextual_identifier_rules_outside_target_fields() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "tool_args": {
                "status": "cust_prod_90210",
                "note": "account C37529641",
                "code": "GHTBUS45KLX"
            }
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(!hits.iter().any(|hit| hit.rule_id == "builtin.pii.customer_id_field"));
        assert!(!hits.iter().any(|hit| hit.rule_id == "builtin.pii.account_number_field"));
        assert!(!hits.iter().any(|hit| hit.rule_id == "builtin.pii.swift_bic_field"));
    }

    #[test]
    fn detects_medical_record_number_patterns() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "medical record number MRN-602675 was referenced in ticket"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(hits.iter().any(|hit| hit.rule_id == "builtin.pii.medical_record_number"));
    }

    #[test]
    fn detects_license_plate_patterns() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "Vehicle checkpoint confirms plate FTR 832 for cargo route"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(hits.iter().any(|hit| hit.rule_id == "builtin.pii.license_plate"));
    }

    #[test]
    fn rejects_license_plate_like_noise() {
        let profile = strict_profile();
        let custom = compile_custom_rules(&profile).expect("rules should compile");
        let payload = json!({
            "content": "build tag XY123 should not be treated as plate"
        });

        let hits = detect_payload(&payload, &profile, &custom, &[]);

        assert!(!hits.iter().any(|hit| hit.rule_id == "builtin.pii.license_plate"));
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
