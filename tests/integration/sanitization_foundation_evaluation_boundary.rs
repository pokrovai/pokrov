use std::fs;

use pokrov_core::types::foundation_evaluation_boundaries;

use crate::sanitization_foundation_test_support::foundation_evaluation_boundary_readme;

#[test]
fn evaluation_boundary_guidance_separates_repo_safe_and_restricted_inputs() {
    let artifact_boundaries = foundation_evaluation_boundaries();
    let readme = fs::read_to_string(foundation_evaluation_boundary_readme())
        .expect("evaluation boundary readme should exist");

    assert!(artifact_boundaries.iter().any(|boundary| boundary.commit_allowed));
    assert!(artifact_boundaries.iter().any(|boundary| !boundary.commit_allowed));
    assert!(readme.contains("repo-safe fixtures"));
    assert!(readme.contains("restricted external references"));
    assert!(!readme.contains("retention policy"));
}
