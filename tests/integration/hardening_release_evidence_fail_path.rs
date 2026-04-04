use std::{io::Read, path::PathBuf};

use tempfile::NamedTempFile;

#[tokio::test]
async fn release_evidence_fail_output_contains_failed_gates_and_remediation() {
    let output = NamedTempFile::new().expect("temp evidence file should be created");
    let output_path: PathBuf = output.path().to_path_buf();

    pokrov_runtime::bootstrap::run(pokrov_runtime::bootstrap::BootstrapArgs {
        config_path: None,
        release_evidence_output: Some(output_path.clone()),
        release_id: Some("release-test".to_string()),
        evidence_artifacts: Vec::new(),
    })
    .await
    .expect("release evidence generation should succeed");

    let mut file = std::fs::File::open(&output_path).expect("evidence file should exist");
    let mut body = String::new();
    file.read_to_string(&mut body)
        .expect("evidence file should be readable");

    let payload: serde_json::Value = serde_json::from_str(&body).expect("evidence must be valid json");
    assert_eq!(payload["gate_status"], "fail");

    let failed_gates = payload["failed_gates"]
        .as_array()
        .expect("failed_gates must be an array");
    let remediation = payload["remediation"]
        .as_array()
        .expect("remediation must be an array");

    assert!(
        !failed_gates.is_empty(),
        "failed evidence must include at least one failed gate"
    );
    assert_eq!(
        failed_gates.len(),
        remediation.len(),
        "failed gates and remediation entries must have one-to-one mapping"
    );
    assert!(
        failed_gates
            .iter()
            .any(|item| item.as_str() == Some("performance")),
        "default generated evidence should include performance failure"
    );
}
