use pokrov_config::model::{
    DeterministicNormalizationMode, DeterministicPatternConfig, DeterministicRecognizerConfig,
};
use pokrov_core::types::{DetectionCategory, PolicyAction};

/// Returns a baseline deterministic recognizer fixture reused by contract and flow tests.
pub fn payment_card_recognizer_fixture() -> DeterministicRecognizerConfig {
    DeterministicRecognizerConfig {
        id: "payment_card".to_string(),
        category: DetectionCategory::Secrets,
        action: PolicyAction::Block,
        family_priority: 600,
        enabled: true,
        patterns: vec![DeterministicPatternConfig {
            id: "pan".to_string(),
            expression: "\\b(?:\\d[ -]*?){13,16}\\b".to_string(),
            base_score: 200,
            validator: None,
            normalization: DeterministicNormalizationMode::AlnumLowercase,
        }],
        denylist_exact: vec!["9999 0000 0000 0000".to_string()],
        allowlist_exact: vec!["4111 1111 1111 1111".to_string()],
        context: None,
    }
}
