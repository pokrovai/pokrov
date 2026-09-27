use std::collections::BTreeMap;

use serde_json::json;

use crate::{
    types::{
        CategoryActions, CustomRule, DetectionCategory, DeterministicNormalizationMode,
        DeterministicRuleKind, DeterministicRuleMetadata, DeterministicValidatorKind,
        EvaluateRequest, EvaluationMode, EvaluatorConfig, PathClass, PolicyAction, PolicyProfile,
    },
    SanitizationEngine,
};

fn engine_with_single_profile(profile: PolicyProfile) -> SanitizationEngine {
    SanitizationEngine::new(EvaluatorConfig {
        default_profile: profile.profile_id.clone(),
        rehydration_key: None,
        profiles: BTreeMap::from([(profile.profile_id.clone(), profile)]),
    })
    .expect("engine should build")
}

fn engine() -> SanitizationEngine {
    let strict = PolicyProfile {
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
        ner_enabled: true,
    };

    let minimal = PolicyProfile {
        profile_id: "minimal".to_string(),
        mode_default: EvaluationMode::Enforce,
        category_actions: CategoryActions {
            secrets: PolicyAction::Mask,
            pii: PolicyAction::Allow,
            corporate_markers: PolicyAction::Allow,
            custom: PolicyAction::Allow,
        },
        mask_visible_suffix: 4,
        max_hits_per_request: 4096,
        custom_rules_enabled: false,
        custom_rules: Vec::new(),
        ner_enabled: false,
    };

    let custom = PolicyProfile {
        profile_id: "custom".to_string(),
        mode_default: EvaluationMode::DryRun,
        category_actions: CategoryActions {
            secrets: PolicyAction::Redact,
            pii: PolicyAction::Mask,
            corporate_markers: PolicyAction::Mask,
            custom: PolicyAction::Redact,
        },
        mask_visible_suffix: 3,
        max_hits_per_request: 4096,
        custom_rules_enabled: true,
        custom_rules: Vec::new(),
        ner_enabled: false,
    };

    let profiles = BTreeMap::from([
        ("minimal".to_string(), minimal),
        ("strict".to_string(), strict),
        ("custom".to_string(), custom),
    ]);

    SanitizationEngine::new(EvaluatorConfig {
        default_profile: "strict".to_string(),
        rehydration_key: None,
        profiles,
    })
    .expect("engine should build")
}

#[test]
fn deterministic_replay_returns_same_signature() {
    let engine = engine();
    let payload = json!({"content": "Project Andromeda card 4111 1111 1111 1111"});

    let one = engine
        .evaluate(EvaluateRequest {
            request_id: "r-1".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: payload.clone(),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("first evaluation should pass");

    let two = engine
        .evaluate(EvaluateRequest {
            request_id: "r-2".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload,
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("second evaluation should pass");

    assert_eq!(one.decision.replay_identity, two.decision.replay_identity);
    assert_eq!(one.decision.final_action, two.decision.final_action);
}

#[test]
fn empty_profile_id_uses_default_profile_for_result_and_audit() {
    let engine = engine();

    let result = engine
        .evaluate(EvaluateRequest {
            request_id: "r-default".to_string(),
            profile_id: String::new(),
            mode: EvaluationMode::Enforce,
            payload: json!({"content": "sk-test-abc12345"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("evaluation should pass");

    assert_eq!(result.profile_id, "strict");
    assert_eq!(result.explain.profile_id, "strict");
    assert_eq!(result.audit.profile_id, "strict");
}

#[test]
fn conflicting_partial_transforms_stay_deterministic_across_rule_order() {
    fn engine_with_rules(custom_rules: Vec<CustomRule>) -> SanitizationEngine {
        engine_with_single_profile(PolicyProfile {
            profile_id: "strict".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Allow,
                pii: PolicyAction::Allow,
                corporate_markers: PolicyAction::Allow,
                custom: PolicyAction::Allow,
            },
            mask_visible_suffix: 4,
            max_hits_per_request: 4096,
            custom_rules_enabled: true,
            custom_rules,
            ner_enabled: false,
        })
    }

    let broader_replace = CustomRule {
        rule_id: "custom.alpha_broader".to_string(),
        category: DetectionCategory::Custom,
        pattern: "(?i)alpha\\s+secret".to_string(),
        action: PolicyAction::Replace,
        priority: 120,
        replacement_template: Some("[CUSTOM_REPLACED]".to_string()),
        enabled: true,
        deterministic: None,
    };
    let inner_redact = CustomRule {
        rule_id: "custom.alpha_inner".to_string(),
        category: DetectionCategory::Custom,
        pattern: "(?i)secret".to_string(),
        action: PolicyAction::Redact,
        priority: 80,
        replacement_template: None,
        enabled: true,
        deterministic: None,
    };

    let engine_forward = engine_with_rules(vec![broader_replace.clone(), inner_redact.clone()]);
    let engine_reversed = engine_with_rules(vec![inner_redact, broader_replace]);

    let payload = json!({"message": "alpha secret token"});
    let one = engine_forward
        .evaluate(EvaluateRequest {
            request_id: "r-forward".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: payload.clone(),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("forward evaluation should pass");
    let two = engine_reversed
        .evaluate(EvaluateRequest {
            request_id: "r-reversed".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload,
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("reversed evaluation should pass");

    assert_eq!(one.decision.final_action, PolicyAction::Redact);
    assert_eq!(one.decision.final_action, two.decision.final_action);
    assert_eq!(one.decision.replay_identity, two.decision.replay_identity);
    assert_eq!(one.transform.sanitized_payload, two.transform.sanitized_payload);

    let transformed_text = one
        .transform
        .sanitized_payload
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(|value| value.as_str())
        .expect("sanitized message should exist");
    assert!(
        !transformed_text.contains("secret"),
        "resolved winner span must not leave partial sensitive passthrough"
    );
}

#[test]
fn foundation_trace_does_not_export_sanitized_payload_content() {
    let engine = engine();
    let trace = engine
        .trace_foundation_flow(EvaluateRequest {
            request_id: "r-trace".to_string(),
            profile_id: "minimal".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"message": "hello from Project X, token sk-test-abc12345"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("trace should build");

    let serialized = serde_json::to_string(&trace).expect("trace must serialize");

    assert!(!serialized.contains("hello from Project X"));
    assert!(!serialized.contains("sk-test-abc12345"));
    assert!(!serialized.contains("abc12345"));
}

#[test]
fn foundation_trace_plan_changes_when_mask_suffix_changes() {
    fn engine_with_mask_suffix(mask_visible_suffix: u8) -> SanitizationEngine {
        engine_with_single_profile(PolicyProfile {
            profile_id: "strict".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Allow,
                pii: PolicyAction::Allow,
                corporate_markers: PolicyAction::Allow,
                custom: PolicyAction::Allow,
            },
            mask_visible_suffix,
            max_hits_per_request: 4096,
            custom_rules_enabled: true,
            custom_rules: vec![CustomRule {
                rule_id: "custom.mask_secret".to_string(),
                category: DetectionCategory::Custom,
                pattern: "secret-[0-9]+".to_string(),
                action: PolicyAction::Mask,
                priority: 300,
                replacement_template: None,
                enabled: true,
                deterministic: None,
            }],
            ner_enabled: false,
        })
    }

    let request = EvaluateRequest {
        request_id: "r-mask".to_string(),
        profile_id: "strict".to_string(),
        mode: EvaluationMode::Enforce,
        payload: json!({"message": "token secret-123456"}),
        path_class: PathClass::Direct,
        effective_language: "en".to_string(),
        entity_scope_filters: Vec::new(),
        recognizer_family_filters: Vec::new(),
        allowlist_additions: Vec::new(),
    };
    let engine_suffix_2 = engine_with_mask_suffix(2);
    let engine_suffix_4 = engine_with_mask_suffix(4);
    let result_suffix_2 =
        engine_suffix_2.evaluate(request.clone()).expect("evaluation with suffix 2 should pass");
    let result_suffix_4 =
        engine_suffix_4.evaluate(request.clone()).expect("evaluation with suffix 4 should pass");
    let trace_suffix_2 = engine_suffix_2
        .trace_foundation_flow(request.clone())
        .expect("trace with suffix 2 should build");
    let trace_suffix_4 =
        engine_suffix_4.trace_foundation_flow(request).expect("trace with suffix 4 should build");

    assert_ne!(
        result_suffix_2.transform.sanitized_payload, result_suffix_4.transform.sanitized_payload,
        "different mask suffixes must change runtime output"
    );
    assert_ne!(
        trace_suffix_2.transform_plan, trace_suffix_4.transform_plan,
        "different mask suffixes must change the exported transform plan"
    );
}

#[test]
fn executed_recognizer_families_include_custom_when_enabled_rules_exist() {
    let engine = engine();
    let result = engine
        .evaluate(EvaluateRequest {
            request_id: "r-executed-custom".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"content": "Project Andromeda"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("evaluation should pass");

    assert_eq!(
        result.executed.recognizer_families_executed,
        vec!["builtin".to_string(), "custom".to_string()]
    );
}

#[test]
fn executed_recognizer_families_exclude_custom_when_not_compiled() {
    let engine = engine();
    let result = engine
        .evaluate(EvaluateRequest {
            request_id: "r-executed-no-custom".to_string(),
            profile_id: "minimal".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"content": "no sensitive data"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("evaluation should pass");

    assert_eq!(result.executed.recognizer_families_executed, vec!["builtin".to_string()]);
}

#[test]
fn deterministic_candidates_total_counts_only_deterministic_hits() {
    let engine = engine_with_single_profile(PolicyProfile {
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
            rule_id: "deterministic.payment_card.pattern.pan".to_string(),
            category: DetectionCategory::Secrets,
            pattern: "\\b(?:\\d[ -]*?){13,16}\\b".to_string(),
            action: PolicyAction::Block,
            priority: 600,
            replacement_template: None,
            enabled: true,
            deterministic: Some(DeterministicRuleMetadata {
                recognizer_id: "payment_card".to_string(),
                allowlist_exact: Vec::new(),
                rule: DeterministicRuleKind::Pattern {
                    validator: DeterministicValidatorKind::None,
                    normalization: DeterministicNormalizationMode::Preserve,
                    context: None,
                },
            }),
        }],
        ner_enabled: false,
    });

    let result = engine
        .evaluate(EvaluateRequest {
            request_id: "r-deterministic-count".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"content": "token sk-test-12345678 card 4111 1111 1111 1111"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("evaluation should pass");

    assert!(result.decision.rule_hits_total > result.decision.deterministic_candidates_total);
    assert_eq!(result.decision.deterministic_candidates_total, 1);
}

fn marker_rule() -> CustomRule {
    CustomRule {
        rule_id: "custom.org_marker".to_string(),
        category: DetectionCategory::Custom,
        pattern: "acme-corp".to_string(),
        action: PolicyAction::Replace,
        priority: 100,
        replacement_template: Some(crate::rehydrate::REVERSIBLE_TEMPLATE.to_string()),
        enabled: true,
        deterministic: None,
    }
}

fn allow_all_profile(custom_rules: Vec<CustomRule>) -> PolicyProfile {
    PolicyProfile {
        profile_id: "strict".to_string(),
        mode_default: EvaluationMode::Enforce,
        category_actions: CategoryActions {
            secrets: PolicyAction::Allow,
            pii: PolicyAction::Allow,
            corporate_markers: PolicyAction::Allow,
            custom: PolicyAction::Allow,
        },
        mask_visible_suffix: 4,
        max_hits_per_request: 4096,
        custom_rules_enabled: true,
        custom_rules,
        ner_enabled: false,
    }
}

#[test]
fn engine_fails_closed_when_marker_rule_has_no_key() {
    let result = SanitizationEngine::new(EvaluatorConfig {
        default_profile: "strict".to_string(),
        rehydration_key: None,
        profiles: BTreeMap::from([("strict".to_string(), allow_all_profile(vec![marker_rule()]))]),
    });

    assert!(result.is_err(), "marker rules without key material must fail closed");
}

#[test]
fn evaluate_without_map_degrades_marker_to_replaced() {
    // `evaluate` drops the rehydration map, so minting a token there would
    // produce an unrestorable orphan; the marker must degrade visibly instead.
    let engine = SanitizationEngine::new(EvaluatorConfig {
        default_profile: "strict".to_string(),
        rehydration_key: Some("unit-rehydration-key".to_string()),
        profiles: BTreeMap::from([("strict".to_string(), allow_all_profile(vec![marker_rule()]))]),
    })
    .expect("engine should build with key material");

    let result = engine
        .evaluate(EvaluateRequest {
            request_id: "r-out".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"message": "use acme-corp-utils"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("evaluation passes");

    let text = serde_json::to_string(
        result.transform.sanitized_payload.as_ref().expect("payload transformed"),
    )
    .expect("payload serializes");
    assert!(text.contains("[REPLACED]"), "orphan-safe fallback must be visible: {text}");
    assert!(!text.contains("__PKV_"));
    assert!(!text.contains("acme-corp"));
}

#[test]
fn evaluate_with_rehydration_round_trips_marker_fragments() {
    let engine = SanitizationEngine::new(EvaluatorConfig {
        default_profile: "strict".to_string(),
        rehydration_key: Some("unit-rehydration-key".to_string()),
        profiles: BTreeMap::from([("strict".to_string(), allow_all_profile(vec![marker_rule()]))]),
    })
    .expect("engine should build with key material");

    let request = |id: &str| EvaluateRequest {
        request_id: id.to_string(),
        profile_id: "strict".to_string(),
        mode: EvaluationMode::Enforce,
        payload: json!({"message": "use acme-corp-utils and acme-corp again"}),
        path_class: PathClass::Direct,
        effective_language: "en".to_string(),
        entity_scope_filters: Vec::new(),
        recognizer_family_filters: Vec::new(),
        allowlist_additions: Vec::new(),
    };

    let first = engine.evaluate_with_rehydration(request("r-1")).expect("evaluation passes");
    let sanitized = first
        .result
        .transform
        .sanitized_payload
        .as_ref()
        .expect("marker rule must transform the payload");
    let sanitized_text = serde_json::to_string(sanitized).expect("payload serializes");
    assert!(!sanitized_text.contains("acme-corp"));
    assert!(sanitized_text.contains("__PKV_"));
    assert_eq!(first.rehydration.spans_total(), 2);

    // Deterministic derivation: a fresh request re-derives the same token and
    // rebuilds the map without any persisted state.
    let second = engine.evaluate_with_rehydration(request("r-2")).expect("evaluation passes");
    let second_text = serde_json::to_string(
        second.result.transform.sanitized_payload.as_ref().expect("payload transformed"),
    )
    .expect("payload serializes");
    assert_eq!(sanitized_text, second_text);

    // Audit and explain artifacts must never contain original fragments or
    // token mappings.
    let audit = serde_json::to_string(&first.result.audit).expect("audit serializes");
    let explain = serde_json::to_string(&first.result.explain).expect("explain serializes");
    assert!(!audit.contains("acme-corp"));
    assert!(!explain.contains("acme-corp"));
}

#[test]
fn evaluate_with_rehydration_restores_tokens_over_json_leaves() {
    let engine = SanitizationEngine::new(EvaluatorConfig {
        default_profile: "strict".to_string(),
        rehydration_key: Some("unit-rehydration-key".to_string()),
        profiles: BTreeMap::from([("strict".to_string(), allow_all_profile(vec![marker_rule()]))]),
    })
    .expect("engine should build with key material");

    let outcome = engine
        .evaluate_with_rehydration(EvaluateRequest {
            request_id: "r-rt".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"message": "use acme-corp-utils"}),
            path_class: PathClass::Direct,
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
        })
        .expect("evaluation passes");

    let sanitized = outcome.result.transform.sanitized_payload.expect("transformed");
    let message = sanitized["message"].as_str().expect("string leaf");
    let token = message
        .strip_prefix("use ")
        .and_then(|text| text.strip_suffix("-utils"))
        .expect("tokenized message embeds a __PKV_ token");
    assert!(token.starts_with("__PKV_"));

    let upstream_echo = json!({"choices": [{"content": format!("use {token}::init")}]});
    let (restored, report) = crate::rehydrate::rehydrate_value(upstream_echo, &outcome.rehydration);

    assert_eq!(restored["choices"][0]["content"], "use acme-corp::init");
    assert_eq!(report.restored, 1);
    assert_eq!(report.unrestored, 0);
}
