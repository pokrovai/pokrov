use pokrov_core::types::{phase_one_en_ru_entity_pack, EntityPackLanguage};

#[test]
fn phase_one_entity_pack_maps_each_supported_entity_to_at_least_one_family() {
    let pack = phase_one_en_ru_entity_pack();

    assert!(pack.supported_entities.iter().all(|entity| !entity.recognizer_families.is_empty()));
}

#[test]
fn phase_one_entity_pack_coverage_projection_contains_required_sections() {
    let pack = phase_one_en_ru_entity_pack();
    let report = pack.coverage_report();

    assert_eq!(report.supported_entities.len(), pack.supported_entities.len());
    assert_eq!(report.unsupported_entities.len(), pack.unsupported_entities.len());
    assert_eq!(report.entity_to_family_map.len(), pack.supported_entities.len());
    assert_eq!(report.entity_to_language_map.len(), pack.supported_entities.len());
    assert_eq!(report.entity_to_default_risk_class_map.len(), pack.supported_entities.len());
}

#[test]
fn phase_one_entity_pack_exposes_en_and_ru_specific_scope() {
    let pack = phase_one_en_ru_entity_pack();

    let en_only = pack
        .supported_entities
        .iter()
        .find(|entity| entity.entity_id == "en_person_name_adjacency")
        .expect("en-first adjacency entity must exist");
    let ru_only = pack
        .supported_entities
        .iter()
        .find(|entity| entity.entity_id == "ru_identifier_contextual")
        .expect("ru-first identifier entity must exist");

    assert_eq!(en_only.languages, vec![EntityPackLanguage::En]);
    assert_eq!(ru_only.languages, vec![EntityPackLanguage::Ru]);
}

#[test]
fn phase_one_entity_pack_keeps_ru_language_sensitive_list_requirements() {
    let pack = phase_one_en_ru_entity_pack();

    let corporate_marker = pack
        .supported_entities
        .iter()
        .find(|entity| entity.entity_id == "corporate_marker")
        .expect("corporate marker entity must exist");

    let ru_requirements = corporate_marker
        .language_requirements
        .iter()
        .find(|requirements| requirements.language == EntityPackLanguage::Ru)
        .expect("corporate marker ru requirements must exist");

    assert!(ru_requirements.requires_allowlist);
    assert!(ru_requirements.requires_denylist);
}

#[test]
fn phase_one_entity_pack_records_deferred_entities_explicitly() {
    let pack = phase_one_en_ru_entity_pack();

    assert!(pack
        .unsupported_entities
        .iter()
        .any(|entity| entity.entity_id == "medical_and_phi_ontology"));
    assert!(pack.unsupported_entities.iter().all(|entity| !entity.rationale.trim().is_empty()));
}
