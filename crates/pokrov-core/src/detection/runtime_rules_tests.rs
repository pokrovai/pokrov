use std::collections::BTreeSet;

use regex::Regex;

use crate::types::{DetectionCategory, PolicyAction};

use super::{emit_rule_matches, CompiledFieldGate, RuleMatchContext};

#[test]
fn field_gate_matches_expected_pointer_suffixes() {
    let gate = CompiledFieldGate {
        json_pointer_suffixes: vec!["/first_name".to_string(), "/last_name".to_string()],
    };

    assert!(gate.matches_json_pointer("/tool_args/first_name"));
    assert!(gate.matches_json_pointer("/profile/last_name"));
    assert!(!gate.matches_json_pointer("/profile/display_name"));
}

#[test]
fn shared_rule_executor_respects_field_gate_before_matching() {
    let matcher = Regex::new("Alice").expect("regex should compile");
    let field_gate = CompiledFieldGate { json_pointer_suffixes: vec!["/first_name".to_string()] };
    let rule = RuleMatchContext {
        rule_id: "builtin.test.person_name",
        category: DetectionCategory::Pii,
        action: PolicyAction::Redact,
        priority: 100,
        replacement_template: None,
        matcher: &matcher,
        validator: crate::types::DeterministicValidatorKind::None,
        normalization: crate::types::DeterministicNormalizationMode::Preserve,
        deterministic_context: None,
        deterministic_allowlist: None,
        field_gate: Some(&field_gate),
    };
    let allowlist = BTreeSet::new();
    let mut hit_limit_reached = false;
    let mut hits = Vec::new();

    emit_rule_matches(
        "/tool_args/display_name",
        "Alice",
        rule,
        &allowlist,
        usize::MAX,
        &mut hit_limit_reached,
        &mut hits,
    );

    assert!(hits.is_empty());
    assert!(!hit_limit_reached);
}
