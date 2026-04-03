use std::path::PathBuf;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs/005-hardening-release/contracts/release-evidence.schema.yaml")
}

#[test]
fn release_evidence_schema_declares_required_top_level_fields() {
    let raw = std::fs::read_to_string(schema_path()).expect("release evidence schema should exist");
    let schema: serde_yaml::Value = serde_yaml::from_str(&raw).expect("schema yaml should parse");

    let required = schema["required"]
        .as_sequence()
        .expect("required list must exist")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();

    for field in [
        "release_id",
        "generated_at",
        "git_commit",
        "performance",
        "security",
        "operational",
        "artifacts",
        "gate_status",
    ] {
        assert!(
            required.contains(&field),
            "required field '{field}' is missing in release evidence schema"
        );
    }
}
