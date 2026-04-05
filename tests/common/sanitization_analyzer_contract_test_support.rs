use pokrov_core::types::EvaluationMode;

use crate::sanitization_deterministic_test_support::payment_card_recognizer_fixture;
use crate::sanitization_foundation_test_support::{foundation_engine, foundation_request};

pub fn analyzer_contract_engine() -> pokrov_core::SanitizationEngine {
    foundation_engine()
}

pub fn analyzer_contract_request(
    request_id: &str,
    mode: EvaluationMode,
) -> pokrov_core::types::EvaluateRequest {
    foundation_request(request_id, mode)
}

pub fn analyzer_deterministic_fixture() -> pokrov_config::model::DeterministicRecognizerConfig {
    payment_card_recognizer_fixture()
}
