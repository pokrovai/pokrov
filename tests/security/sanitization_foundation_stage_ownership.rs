use pokrov_core::types::{foundation_extension_points, foundation_stage_boundaries, StageId};

#[test]
fn only_policy_stage_owns_final_action_selection() {
    let boundaries = foundation_stage_boundaries();

    assert_eq!(
        boundaries.iter().filter(|boundary| boundary.owns_policy_decision).count(),
        1
    );
    assert!(boundaries
        .iter()
        .any(|boundary| boundary.stage_id == StageId::PolicyResolution && boundary.owns_policy_decision));
    assert!(foundation_extension_points()
        .iter()
        .all(|extension_point| !extension_point.policy_ownership_allowed));
}

#[test]
fn only_transform_stage_may_mutate_payloads() {
    let boundaries = foundation_stage_boundaries();

    assert_eq!(
        boundaries.iter().filter(|boundary| boundary.may_mutate_payload).count(),
        1
    );
    assert!(boundaries
        .iter()
        .any(|boundary| boundary.stage_id == StageId::Transformation && boundary.may_mutate_payload));
}
