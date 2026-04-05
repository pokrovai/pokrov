use std::collections::BTreeMap;

use pokrov_core::{
    types::{
        CategoryActions, CustomRule, DetectionCategory, EvaluateRequest, EvaluationMode,
        EvaluatorConfig, PathClass, PolicyProfile,
    },
    SanitizationEngine,
};
use serde_json::{json, Value};

use crate::sanitization_deterministic_test_support::payment_card_recognizer_fixture;

pub fn foundation_engine() -> SanitizationEngine {
    let strict = PolicyProfile {
        profile_id: "strict".to_string(),
        mode_default: EvaluationMode::Enforce,
        category_actions: CategoryActions {
            secrets: pokrov_core::types::PolicyAction::Block,
            pii: pokrov_core::types::PolicyAction::Redact,
            corporate_markers: pokrov_core::types::PolicyAction::Mask,
            custom: pokrov_core::types::PolicyAction::Redact,
        },
        mask_visible_suffix: 4,
        custom_rules_enabled: true,
        custom_rules: vec![
            CustomRule {
                rule_id: "custom.project_andromeda".to_string(),
                category: DetectionCategory::CorporateMarkers,
                pattern: "(?i)project\\s+andromeda".to_string(),
                action: pokrov_core::types::PolicyAction::Redact,
                priority: 900,
                replacement_template: None,
                enabled: true,
            },
            CustomRule {
                rule_id: "deterministic.payment_card.pattern.pan".to_string(),
                category: DetectionCategory::Secrets,
                pattern: "\\b(?:\\d[ -]*?){13,16}\\b".to_string(),
                action: pokrov_core::types::PolicyAction::Block,
                priority: 800,
                replacement_template: None,
                enabled: true,
            },
        ],
    };

    let profiles = BTreeMap::from([("strict".to_string(), strict)]);

    SanitizationEngine::new(EvaluatorConfig { default_profile: "strict".to_string(), profiles })
        .expect("foundation engine should build")
}

pub fn foundation_payload() -> Value {
    json!({
        "messages": [
            {
                "role": "user",
                "content": "Project Andromeda token sk-test-12345678 and user@example.com"
            }
        ]
    })
}

pub fn foundation_request(request_id: &str, mode: EvaluationMode) -> EvaluateRequest {
    EvaluateRequest {
        request_id: request_id.to_string(),
        profile_id: "strict".to_string(),
        mode,
        payload: foundation_payload(),
        path_class: PathClass::Direct,
        effective_language: "en".to_string(),
        entity_scope_filters: Vec::new(),
        recognizer_family_filters: Vec::new(),
        allowlist_additions: Vec::new(),
    }
}

pub fn foundation_evaluation_boundary_readme() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval/README.md")
}

pub fn foundation_deterministic_fixture() -> pokrov_config::model::DeterministicRecognizerConfig {
    payment_card_recognizer_fixture()
}
