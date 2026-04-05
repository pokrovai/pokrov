use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Supported language set for the phase-one entity pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityPackLanguage {
    En,
    Ru,
}

impl EntityPackLanguage {
    fn as_contract_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
        }
    }
}

/// Deterministic recognizer family classes used by one supported entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognizerFamily {
    Pattern,
    Validation,
    Checksum,
    Context,
    AllowlistSuppression,
    Denylist,
}

impl RecognizerFamily {
    fn as_contract_str(self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::Validation => "validation",
            Self::Checksum => "checksum",
            Self::Context => "context",
            Self::AllowlistSuppression => "allowlist_suppression",
            Self::Denylist => "denylist",
        }
    }
}

/// Phase-one risk buckets used by operator-direction defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityRiskClass {
    Secrets,
    HighConfidencePii,
    CorporateMarkers,
    CustomOrUnresolved,
}

impl EntityRiskClass {
    fn as_contract_str(self) -> &'static str {
        match self {
            Self::Secrets => "secrets",
            Self::HighConfidencePii => "high_confidence_pii",
            Self::CorporateMarkers => "corporate_markers",
            Self::CustomOrUnresolved => "custom_or_unresolved",
        }
    }
}

/// Directional operator expectation for one risk class in phase one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultOperatorDirection {
    BlockOrRedact,
    RedactOrMask,
    KeepOrProfileSafe,
    ConservativeProfileControlled,
}

/// Language-sensitive constraints required for one entity/language pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLanguageRequirement {
    pub language: EntityPackLanguage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_context_terms: Vec<String>,
    #[serde(default)]
    pub requires_validation: bool,
    #[serde(default)]
    pub requires_allowlist: bool,
    #[serde(default)]
    pub requires_denylist: bool,
}

/// Supported entity definition used by evaluation and parity setup flows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportedEntityDefinition {
    pub entity_id: String,
    pub risk_class: EntityRiskClass,
    pub default_operator_direction: DefaultOperatorDirection,
    pub recognizer_families: Vec<RecognizerFamily>,
    pub languages: Vec<EntityPackLanguage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub language_requirements: Vec<EntityLanguageRequirement>,
}

/// Unsupported or deferred entity scope recorded with an explicit rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedEntityDefinition {
    pub entity_id: String,
    pub rationale: String,
}

/// Default operator direction per risk class for the phase-one pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskClassDirection {
    pub risk_class: EntityRiskClass,
    pub direction: DefaultOperatorDirection,
}

/// Explicit phase-one EN/RU entity-pack definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPackDefinition {
    pub pack_id: String,
    pub supported_entities: Vec<SupportedEntityDefinition>,
    pub unsupported_entities: Vec<UnsupportedEntityDefinition>,
    pub risk_class_directions: Vec<RiskClassDirection>,
}

impl EntityPackDefinition {
    /// Builds the reporting projection consumed by coverage and parity tooling.
    pub fn coverage_report(&self) -> EntityPackCoverageReport {
        let supported_entities =
            self.supported_entities.iter().map(|entity| entity.entity_id.clone()).collect();
        let unsupported_entities =
            self.unsupported_entities.iter().map(|entity| entity.entity_id.clone()).collect();

        let mut entity_to_family_map = BTreeMap::new();
        let mut entity_to_language_map = BTreeMap::new();
        let mut entity_to_default_risk_class_map = BTreeMap::new();

        for entity in &self.supported_entities {
            entity_to_family_map.insert(
                entity.entity_id.clone(),
                entity
                    .recognizer_families
                    .iter()
                    .map(|family| family.as_contract_str().to_string())
                    .collect(),
            );
            entity_to_language_map.insert(
                entity.entity_id.clone(),
                entity
                    .languages
                    .iter()
                    .map(|language| language.as_contract_str().to_string())
                    .collect(),
            );
            entity_to_default_risk_class_map
                .insert(entity.entity_id.clone(), entity.risk_class.as_contract_str().to_string());
        }

        EntityPackCoverageReport {
            supported_entities,
            unsupported_entities,
            entity_to_family_map,
            entity_to_language_map,
            entity_to_default_risk_class_map,
        }
    }
}

/// Coverage-report shape required by the EN/RU entity-pack scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPackCoverageReport {
    pub supported_entities: Vec<String>,
    pub unsupported_entities: Vec<String>,
    pub entity_to_family_map: BTreeMap<String, Vec<String>>,
    pub entity_to_language_map: BTreeMap<String, Vec<String>>,
    pub entity_to_default_risk_class_map: BTreeMap<String, String>,
}

/// Returns the frozen phase-one EN/RU entity-pack definition.
pub fn phase_one_en_ru_entity_pack() -> EntityPackDefinition {
    EntityPackDefinition {
        pack_id: "phase_one_en_ru".to_string(),
        supported_entities: vec![
            SupportedEntityDefinition {
                entity_id: "email".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Validation,
                    RecognizerFamily::AllowlistSuppression,
                ],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: Vec::new(),
                        requires_validation: true,
                        requires_allowlist: true,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: Vec::new(),
                        requires_validation: true,
                        requires_allowlist: true,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "phone_number".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![RecognizerFamily::Pattern, RecognizerFamily::Context],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: vec![
                            "phone".to_string(),
                            "call".to_string(),
                            "mobile".to_string(),
                        ],
                        requires_validation: false,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: vec![
                            "телефон".to_string(),
                            "мобильный".to_string(),
                            "звонок".to_string(),
                        ],
                        requires_validation: false,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "card_like_number".to_string(),
                risk_class: EntityRiskClass::Secrets,
                default_operator_direction: DefaultOperatorDirection::BlockOrRedact,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Checksum,
                    RecognizerFamily::Context,
                ],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: vec!["card".to_string(), "payment".to_string()],
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: vec!["карта".to_string(), "оплата".to_string()],
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "iban".to_string(),
                risk_class: EntityRiskClass::Secrets,
                default_operator_direction: DefaultOperatorDirection::BlockOrRedact,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Checksum,
                    RecognizerFamily::Context,
                ],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: vec!["iban".to_string(), "bank".to_string()],
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: vec!["iban".to_string(), "банк".to_string()],
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "ip_address".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![RecognizerFamily::Pattern, RecognizerFamily::Validation],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: Vec::new(),
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: Vec::new(),
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "url_or_domain".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Validation,
                    RecognizerFamily::AllowlistSuppression,
                ],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: Vec::new(),
                        requires_validation: true,
                        requires_allowlist: true,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: Vec::new(),
                        requires_validation: true,
                        requires_allowlist: true,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "secret_token".to_string(),
                risk_class: EntityRiskClass::Secrets,
                default_operator_direction: DefaultOperatorDirection::BlockOrRedact,
                recognizer_families: vec![RecognizerFamily::Pattern, RecognizerFamily::Validation],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: vec!["token".to_string(), "api key".to_string()],
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: vec!["токен".to_string(), "ключ api".to_string()],
                        requires_validation: true,
                        requires_allowlist: false,
                        requires_denylist: false,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "corporate_marker".to_string(),
                risk_class: EntityRiskClass::CorporateMarkers,
                default_operator_direction: DefaultOperatorDirection::KeepOrProfileSafe,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Denylist,
                    RecognizerFamily::Context,
                ],
                languages: vec![EntityPackLanguage::En, EntityPackLanguage::Ru],
                language_requirements: vec![
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::En,
                        required_context_terms: vec![
                            "inc".to_string(),
                            "llc".to_string(),
                            "ltd".to_string(),
                        ],
                        requires_validation: false,
                        requires_allowlist: false,
                        requires_denylist: true,
                    },
                    EntityLanguageRequirement {
                        language: EntityPackLanguage::Ru,
                        required_context_terms: vec![
                            "ооо".to_string(),
                            "зао".to_string(),
                            "ип".to_string(),
                        ],
                        requires_validation: false,
                        requires_allowlist: true,
                        requires_denylist: true,
                    },
                ],
            },
            SupportedEntityDefinition {
                entity_id: "en_person_name_adjacency".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![RecognizerFamily::Pattern, RecognizerFamily::Context],
                languages: vec![EntityPackLanguage::En],
                language_requirements: vec![EntityLanguageRequirement {
                    language: EntityPackLanguage::En,
                    required_context_terms: vec![
                        "mr".to_string(),
                        "mrs".to_string(),
                        "name".to_string(),
                    ],
                    requires_validation: false,
                    requires_allowlist: false,
                    requires_denylist: false,
                }],
            },
            SupportedEntityDefinition {
                entity_id: "en_address_like_high_risk".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Context,
                    RecognizerFamily::Validation,
                ],
                languages: vec![EntityPackLanguage::En],
                language_requirements: vec![EntityLanguageRequirement {
                    language: EntityPackLanguage::En,
                    required_context_terms: vec![
                        "street".to_string(),
                        "avenue".to_string(),
                        "zip".to_string(),
                    ],
                    requires_validation: true,
                    requires_allowlist: false,
                    requires_denylist: false,
                }],
            },
            SupportedEntityDefinition {
                entity_id: "ru_identifier_contextual".to_string(),
                risk_class: EntityRiskClass::HighConfidencePii,
                default_operator_direction: DefaultOperatorDirection::RedactOrMask,
                recognizer_families: vec![
                    RecognizerFamily::Pattern,
                    RecognizerFamily::Context,
                    RecognizerFamily::AllowlistSuppression,
                    RecognizerFamily::Denylist,
                ],
                languages: vec![EntityPackLanguage::Ru],
                language_requirements: vec![EntityLanguageRequirement {
                    language: EntityPackLanguage::Ru,
                    required_context_terms: vec![
                        "паспорт".to_string(),
                        "договор".to_string(),
                        "идентификатор".to_string(),
                    ],
                    requires_validation: false,
                    requires_allowlist: true,
                    requires_denylist: true,
                }],
            },
        ],
        unsupported_entities: vec![
            UnsupportedEntityDefinition {
                entity_id: "ml_person_name_parity".to_string(),
                rationale: "Deferred: broad person-name parity requires ML NER families excluded from phase one"
                    .to_string(),
            },
            UnsupportedEntityDefinition {
                entity_id: "ml_location_and_organization_ner".to_string(),
                rationale:
                    "Deferred: location and organization families require heavy NLP outside phase-one deterministic scope"
                        .to_string(),
            },
            UnsupportedEntityDefinition {
                entity_id: "medical_and_phi_ontology".to_string(),
                rationale: "Deferred: medical and full PHI ontology is excluded from first native rollout"
                    .to_string(),
            },
            UnsupportedEntityDefinition {
                entity_id: "global_national_identifier_long_tail".to_string(),
                rationale: "Deferred: long-tail country identifier coverage is outside selected deterministic validators"
                    .to_string(),
            },
        ],
        risk_class_directions: vec![
            RiskClassDirection {
                risk_class: EntityRiskClass::Secrets,
                direction: DefaultOperatorDirection::BlockOrRedact,
            },
            RiskClassDirection {
                risk_class: EntityRiskClass::HighConfidencePii,
                direction: DefaultOperatorDirection::RedactOrMask,
            },
            RiskClassDirection {
                risk_class: EntityRiskClass::CorporateMarkers,
                direction: DefaultOperatorDirection::KeepOrProfileSafe,
            },
            RiskClassDirection {
                risk_class: EntityRiskClass::CustomOrUnresolved,
                direction: DefaultOperatorDirection::ConservativeProfileControlled,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{phase_one_en_ru_entity_pack, EntityPackLanguage};

    #[test]
    fn supported_entities_have_at_least_one_recognizer_family() {
        let pack = phase_one_en_ru_entity_pack();

        assert!(!pack.supported_entities.is_empty());
        assert!(pack
            .supported_entities
            .iter()
            .all(|entity| !entity.recognizer_families.is_empty()));
    }

    #[test]
    fn coverage_report_exposes_required_maps() {
        let pack = phase_one_en_ru_entity_pack();
        let report = pack.coverage_report();

        assert_eq!(report.supported_entities.len(), pack.supported_entities.len());
        assert_eq!(report.unsupported_entities.len(), pack.unsupported_entities.len());
        assert_eq!(report.entity_to_family_map.len(), pack.supported_entities.len());
        assert_eq!(report.entity_to_language_map.len(), pack.supported_entities.len());
        assert_eq!(report.entity_to_default_risk_class_map.len(), pack.supported_entities.len());

        let email_families = report
            .entity_to_family_map
            .get("email")
            .expect("email entry must be present in family map");
        assert!(email_families.iter().any(|family| family == "allowlist_suppression"));

        let email_risk_class = report
            .entity_to_default_risk_class_map
            .get("email")
            .expect("email entry must be present in risk class map");
        assert_eq!(email_risk_class, "high_confidence_pii");
    }

    #[test]
    fn en_ru_specific_entities_keep_language_boundaries() {
        let pack = phase_one_en_ru_entity_pack();

        let en_only = pack
            .supported_entities
            .iter()
            .find(|entity| entity.entity_id == "en_person_name_adjacency")
            .expect("en-first entity must be present");
        let ru_only = pack
            .supported_entities
            .iter()
            .find(|entity| entity.entity_id == "ru_identifier_contextual")
            .expect("ru-first entity must be present");

        assert_eq!(en_only.languages, vec![EntityPackLanguage::En]);
        assert_eq!(ru_only.languages, vec![EntityPackLanguage::Ru]);
    }

    #[test]
    fn ru_language_sensitive_entities_keep_list_support() {
        let pack = phase_one_en_ru_entity_pack();

        let ru_identifier = pack
            .supported_entities
            .iter()
            .find(|entity| entity.entity_id == "ru_identifier_contextual")
            .expect("ru identifier entity must be present");
        let ru_requirement = ru_identifier
            .language_requirements
            .iter()
            .find(|requirement| requirement.language == EntityPackLanguage::Ru)
            .expect("ru language requirement must be present");

        assert!(ru_requirement.requires_allowlist);
        assert!(ru_requirement.requires_denylist);
        assert!(!ru_requirement.required_context_terms.is_empty());
    }
}
