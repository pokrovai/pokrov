use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Access model for one dataset or corpus source used by parity workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetAccessModel {
    OpenTooling,
    Restricted,
    ExternalDownload,
    InternalOnly,
    SourceDependent,
}

/// Repository residency policy for one dataset inventory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetRepositoryStatus {
    RepoSafe,
    RestrictedOnly,
}

/// Execution scope policy for one dataset inventory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetExecutionScope {
    CiSafe,
    LocalOnly,
}

/// Canonical metadata record for one dataset inventory source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetInventoryRecord {
    pub dataset_id: String,
    pub display_name: String,
    pub access_model: DatasetAccessModel,
    pub license_constraints: String,
    pub language_coverage: Vec<String>,
    pub entity_coverage: Vec<String>,
    pub intended_handler_families: Vec<String>,
    pub repository_status: DatasetRepositoryStatus,
    pub execution_scope: DatasetExecutionScope,
}

/// Comparative baseline implementations tracked by the evaluation lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSystem {
    VanillaPresidio,
    TunedPresidio,
    PokrovCurrentNative,
    PokrovUpdatedNative,
    NlmScrubber,
}

/// Baseline requirement tier tied to deterministic or future workstreams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineRequirementTier {
    MandatoryDeterministic,
    OptionalFutureWorkstreams,
}

/// Baseline catalog entry with comparative scope and requirement tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineSystemRecord {
    pub system: BaselineSystem,
    pub description: String,
    pub tier: BaselineRequirementTier,
    pub intended_handler_families: Vec<String>,
}

/// Required starter-corpus case groups for Phase 1A parity bootstrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterCorpusCaseGroup {
    DeterministicPositives,
    DeterministicNegatives,
    ContextPairs,
    OverlapAndOperatorCases,
    StructuredJsonCases,
    AdversarialSmokeCases,
}

/// Starter-corpus volume targets used by initial parity and regression runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterCorpusVolumeTargets {
    pub per_priority_family_min: u16,
    pub per_priority_family_max: u16,
    pub shared_hard_negatives: u16,
    pub structured_json_cases: u16,
    pub adversarial_smoke_cases: u16,
}

/// Phase 1A starter-corpus contract used for repeatable baseline execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterCorpusDefinition {
    pub corpus_id: String,
    pub required_groups: Vec<StarterCorpusCaseGroup>,
    pub deterministic_positive_families: Vec<String>,
    pub deterministic_negative_scenarios: Vec<String>,
    pub repo_safe_policy: String,
    pub restricted_reference_policy: String,
    pub target_volume: StarterCorpusVolumeTargets,
}

/// Baseline run requirement that must be satisfied for parity stability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineRunRequirement {
    pub run_id: String,
    pub baseline: BaselineSystem,
    pub tier: BaselineRequirementTier,
    pub corpus_id: String,
    pub required_metadata: Vec<String>,
    pub intended_handler_families: Vec<String>,
}

/// Returns the canonical dataset inventory required by baseline reporting.
pub fn foundation_dataset_inventory() -> Vec<DatasetInventoryRecord> {
    vec![
        DatasetInventoryRecord {
            dataset_id: "presidio_research".to_string(),
            display_name: "presidio-research".to_string(),
            access_model: DatasetAccessModel::OpenTooling,
            license_constraints: "inherits source-specific licenses; redistribution allowed only for synthetic outputs cleared for publication".to_string(),
            language_coverage: vec!["en".to_string()],
            entity_coverage: vec![
                "deterministic pii".to_string(),
                "recognizer parity workflows".to_string(),
            ],
            intended_handler_families: vec![
                "pattern".to_string(),
                "checksum".to_string(),
                "context".to_string(),
                "operators".to_string(),
                "evaluation workflows".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RepoSafe,
            execution_scope: DatasetExecutionScope::CiSafe,
        },
        DatasetInventoryRecord {
            dataset_id: "ai4privacy_pii_masking_200k".to_string(),
            display_name: "ai4privacy/pii-masking-200k".to_string(),
            access_model: DatasetAccessModel::ExternalDownload,
            license_constraints: "open dataset terms per upstream card; verify redistribution constraints before republishing snapshots".to_string(),
            language_coverage: vec!["en".to_string(), "multi-language subsets".to_string()],
            entity_coverage: vec![
                "synthetic pii annotations".to_string(),
                "de-identification training/evaluation".to_string(),
            ],
            intended_handler_families: vec![
                "deterministic text families".to_string(),
                "baseline parity workflows".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
        DatasetInventoryRecord {
            dataset_id: "nvidia_nemotron_pii".to_string(),
            display_name: "nvidia/Nemotron-PII".to_string(),
            access_model: DatasetAccessModel::ExternalDownload,
            license_constraints: "cc-by-4.0 attribution requirements apply to derivatives and exports".to_string(),
            language_coverage: vec!["en".to_string()],
            entity_coverage: vec![
                "synthetic pii labels".to_string(),
                "entity extraction benchmarks".to_string(),
            ],
            intended_handler_families: vec![
                "deterministic text families".to_string(),
                "future recognizer parity".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
        DatasetInventoryRecord {
            dataset_id: "gretel_pii_masking_en_v1".to_string(),
            display_name: "gretelai/gretel-pii-masking-en-v1".to_string(),
            access_model: DatasetAccessModel::ExternalDownload,
            license_constraints: "apache-2.0 upstream license with notice retention requirements".to_string(),
            language_coverage: vec!["en".to_string()],
            entity_coverage: vec![
                "synthetic pii masking pairs".to_string(),
                "masking quality evaluation".to_string(),
            ],
            intended_handler_families: vec![
                "deterministic text families".to_string(),
                "operator quality evaluation".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
        DatasetInventoryRecord {
            dataset_id: "n2c2_i2b2_deid".to_string(),
            display_name: "n2c2 / i2b2 de-identification datasets".to_string(),
            access_model: DatasetAccessModel::Restricted,
            license_constraints: "restricted clinical license; no plain fixture redistribution"
                .to_string(),
            language_coverage: vec!["en".to_string()],
            entity_coverage: vec!["clinical phi".to_string()],
            intended_handler_families: vec![
                "future phi families".to_string(),
                "remote recognizers".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
        DatasetInventoryRecord {
            dataset_id: "tcia_pseudo_phi_dicom".to_string(),
            display_name: "Pseudo-PHI-DICOM-Data (TCIA)".to_string(),
            access_model: DatasetAccessModel::ExternalDownload,
            license_constraints: "external dataset terms apply; do not mirror full payloads into repository fixtures".to_string(),
            language_coverage: vec!["medical-image metadata contexts".to_string()],
            entity_coverage: vec!["dicom phi".to_string(), "ocr text leakage".to_string()],
            intended_handler_families: vec![
                "future image ocr".to_string(),
                "future dicom workflows".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
        DatasetInventoryRecord {
            dataset_id: "pokrov_internal_deidentified".to_string(),
            display_name: "Pokrov internal de-identified corpus".to_string(),
            access_model: DatasetAccessModel::InternalOnly,
            license_constraints:
                "internal governance only; export prohibited outside secured environments"
                    .to_string(),
            language_coverage: vec!["en".to_string(), "ru".to_string()],
            entity_coverage: vec![
                "prompt and tool pii".to_string(),
                "corporate markers".to_string(),
                "structured json cases".to_string(),
                "adversarial proxy patterns".to_string(),
            ],
            intended_handler_families: vec![
                "proxy text flows".to_string(),
                "structured json".to_string(),
                "adversarial mixed-language".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
        DatasetInventoryRecord {
            dataset_id: "optional_hard_negative_corpora".to_string(),
            display_name: "Optional hard-negative corpora".to_string(),
            access_model: DatasetAccessModel::SourceDependent,
            license_constraints:
                "varies by source; each selected corpus must record redistribution policy"
                    .to_string(),
            language_coverage: vec!["en".to_string(), "ru".to_string()],
            entity_coverage: vec!["false-positive lookalikes".to_string()],
            intended_handler_families: vec![
                "deterministic families".to_string(),
                "allowlist behavior".to_string(),
            ],
            repository_status: DatasetRepositoryStatus::RestrictedOnly,
            execution_scope: DatasetExecutionScope::LocalOnly,
        },
    ]
}

/// Returns the baseline-system catalog for deterministic and future workstreams.
pub fn foundation_baseline_systems() -> Vec<BaselineSystemRecord> {
    vec![
        BaselineSystemRecord {
            system: BaselineSystem::VanillaPresidio,
            description: "default Presidio analyzer and anonymizer configuration".to_string(),
            tier: BaselineRequirementTier::MandatoryDeterministic,
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineSystemRecord {
            system: BaselineSystem::TunedPresidio,
            description: "Presidio tuned with presidio-research workflows".to_string(),
            tier: BaselineRequirementTier::MandatoryDeterministic,
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineSystemRecord {
            system: BaselineSystem::PokrovCurrentNative,
            description: "current Pokrov native baseline used before updates".to_string(),
            tier: BaselineRequirementTier::MandatoryDeterministic,
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineSystemRecord {
            system: BaselineSystem::PokrovUpdatedNative,
            description: "updated Pokrov native baseline after rework changes".to_string(),
            tier: BaselineRequirementTier::MandatoryDeterministic,
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineSystemRecord {
            system: BaselineSystem::NlmScrubber,
            description: "optional PHI-oriented comparative baseline".to_string(),
            tier: BaselineRequirementTier::OptionalFutureWorkstreams,
            intended_handler_families: vec![
                "future phi workstreams".to_string(),
                "future image workstreams".to_string(),
            ],
        },
    ]
}

/// Returns the frozen Phase 1A starter-corpus definition.
pub fn foundation_phase_one_a_starter_corpus() -> StarterCorpusDefinition {
    StarterCorpusDefinition {
        corpus_id: "phase_1a_starter_corpus".to_string(),
        required_groups: vec![
            StarterCorpusCaseGroup::DeterministicPositives,
            StarterCorpusCaseGroup::DeterministicNegatives,
            StarterCorpusCaseGroup::ContextPairs,
            StarterCorpusCaseGroup::OverlapAndOperatorCases,
            StarterCorpusCaseGroup::StructuredJsonCases,
            StarterCorpusCaseGroup::AdversarialSmokeCases,
        ],
        deterministic_positive_families: vec![
            "email".to_string(),
            "phone".to_string(),
            "card_like_number".to_string(),
            "iban".to_string(),
            "ip".to_string(),
            "url".to_string(),
            "secret_token".to_string(),
            "corporate_marker".to_string(),
        ],
        deterministic_negative_scenarios: vec![
            "invalid lookalikes".to_string(),
            "allowlist scenarios".to_string(),
            "numeric non-entities".to_string(),
        ],
        repo_safe_policy:
            "starter fixtures in repository must remain synthetic or redistributable".to_string(),
        restricted_reference_policy:
            "restricted datasets are tracked only as metadata references outside repository fixtures"
                .to_string(),
        target_volume: StarterCorpusVolumeTargets {
            per_priority_family_min: 25,
            per_priority_family_max: 40,
            shared_hard_negatives: 100,
            structured_json_cases: 50,
            adversarial_smoke_cases: 30,
        },
    }
}

/// Returns the minimum baseline-run matrix required for stable parity reporting.
pub fn foundation_baseline_run_matrix() -> Vec<BaselineRunRequirement> {
    let required_metadata = vec![
        "run_id".to_string(),
        "started_at_utc".to_string(),
        "finished_at_utc".to_string(),
        "git_revision".to_string(),
        "dataset_inventory_id".to_string(),
        "starter_corpus_id".to_string(),
        "baseline_system".to_string(),
        "handler_scope".to_string(),
        "metric_groups".to_string(),
        "report_output_kinds".to_string(),
    ];

    vec![
        BaselineRunRequirement {
            run_id: "phase_1a_vanilla_presidio".to_string(),
            baseline: BaselineSystem::VanillaPresidio,
            tier: BaselineRequirementTier::MandatoryDeterministic,
            corpus_id: "phase_1a_starter_corpus".to_string(),
            required_metadata: required_metadata.clone(),
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineRunRequirement {
            run_id: "phase_1a_tuned_presidio".to_string(),
            baseline: BaselineSystem::TunedPresidio,
            tier: BaselineRequirementTier::MandatoryDeterministic,
            corpus_id: "phase_1a_starter_corpus".to_string(),
            required_metadata: required_metadata.clone(),
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineRunRequirement {
            run_id: "phase_1a_pokrov_current_native".to_string(),
            baseline: BaselineSystem::PokrovCurrentNative,
            tier: BaselineRequirementTier::MandatoryDeterministic,
            corpus_id: "phase_1a_starter_corpus".to_string(),
            required_metadata: required_metadata.clone(),
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineRunRequirement {
            run_id: "phase_1a_pokrov_updated_native".to_string(),
            baseline: BaselineSystem::PokrovUpdatedNative,
            tier: BaselineRequirementTier::MandatoryDeterministic,
            corpus_id: "phase_1a_starter_corpus".to_string(),
            required_metadata: required_metadata.clone(),
            intended_handler_families: vec!["deterministic text families".to_string()],
        },
        BaselineRunRequirement {
            run_id: "phase_1a_nlm_scrubber_optional".to_string(),
            baseline: BaselineSystem::NlmScrubber,
            tier: BaselineRequirementTier::OptionalFutureWorkstreams,
            corpus_id: "phase_1a_starter_corpus".to_string(),
            required_metadata,
            intended_handler_families: vec![
                "future phi workstreams".to_string(),
                "future image workstreams".to_string(),
            ],
        },
    ]
}

/// Reports missing required metadata fields for dataset inventory records.
pub fn foundation_dataset_inventory_missing_metadata(
    records: &[DatasetInventoryRecord],
) -> Vec<String> {
    let mut missing = Vec::new();

    for record in records {
        if record.dataset_id.trim().is_empty() {
            missing.push("dataset_id".to_string());
        }
        if record.display_name.trim().is_empty() {
            missing.push(format!("{}.display_name", record.dataset_id));
        }
        if record.license_constraints.trim().is_empty() {
            missing.push(format!("{}.license_constraints", record.dataset_id));
        }
        if record.language_coverage.is_empty() {
            missing.push(format!("{}.language_coverage", record.dataset_id));
        }
        if record.entity_coverage.is_empty() {
            missing.push(format!("{}.entity_coverage", record.dataset_id));
        }
        if record.intended_handler_families.is_empty() {
            missing.push(format!("{}.intended_handler_families", record.dataset_id));
        }
    }

    missing
}

/// Reports missing mandatory case groups in the Phase 1A starter corpus definition.
pub fn foundation_starter_corpus_missing_groups(
    starter: &StarterCorpusDefinition,
) -> Vec<StarterCorpusCaseGroup> {
    let required = [
        StarterCorpusCaseGroup::DeterministicPositives,
        StarterCorpusCaseGroup::DeterministicNegatives,
        StarterCorpusCaseGroup::ContextPairs,
        StarterCorpusCaseGroup::OverlapAndOperatorCases,
        StarterCorpusCaseGroup::StructuredJsonCases,
        StarterCorpusCaseGroup::AdversarialSmokeCases,
    ];
    let existing = starter.required_groups.iter().copied().collect::<BTreeSet<_>>();

    required
        .into_iter()
        .filter(|group| !existing.contains(group))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        foundation_baseline_run_matrix, foundation_baseline_systems, foundation_dataset_inventory,
        foundation_dataset_inventory_missing_metadata, foundation_phase_one_a_starter_corpus,
        foundation_starter_corpus_missing_groups, BaselineRequirementTier, BaselineSystem,
        DatasetRepositoryStatus, StarterCorpusCaseGroup,
    };

    #[test]
    fn dataset_inventory_keeps_required_metadata_and_access_boundaries() {
        let inventory = foundation_dataset_inventory();
        let missing = foundation_dataset_inventory_missing_metadata(&inventory);

        assert_eq!(inventory.len(), 8);
        assert!(missing.is_empty(), "missing required metadata: {missing:?}");
        assert!(inventory
            .iter()
            .any(|entry| entry.dataset_id == "ai4privacy_pii_masking_200k"));
        assert!(inventory
            .iter()
            .any(|entry| entry.dataset_id == "nvidia_nemotron_pii"));
        assert!(inventory
            .iter()
            .any(|entry| entry.dataset_id == "gretel_pii_masking_en_v1"));
        assert!(inventory
            .iter()
            .any(|entry| entry.repository_status == DatasetRepositoryStatus::RepoSafe));
        assert!(inventory
            .iter()
            .any(|entry| entry.repository_status == DatasetRepositoryStatus::RestrictedOnly));
    }

    #[test]
    fn phase_one_a_starter_corpus_contains_all_mandatory_groups() {
        let starter = foundation_phase_one_a_starter_corpus();
        let missing = foundation_starter_corpus_missing_groups(&starter);

        assert!(missing.is_empty(), "missing starter groups: {missing:?}");
        assert!(starter
            .required_groups
            .contains(&StarterCorpusCaseGroup::StructuredJsonCases));
        assert_eq!(starter.target_volume.per_priority_family_min, 25);
        assert_eq!(starter.target_volume.per_priority_family_max, 40);
        assert_eq!(starter.target_volume.shared_hard_negatives, 100);
        assert_eq!(starter.target_volume.structured_json_cases, 50);
        assert_eq!(starter.target_volume.adversarial_smoke_cases, 30);
    }

    #[test]
    fn baseline_matrix_tracks_mandatory_and_optional_systems() {
        let systems = foundation_baseline_systems();
        let run_matrix = foundation_baseline_run_matrix();

        assert_eq!(systems.len(), 5);
        assert!(systems
            .iter()
            .any(|system| system.system == BaselineSystem::NlmScrubber));
        assert!(run_matrix.iter().any(|run| {
            run.baseline == BaselineSystem::VanillaPresidio
                && run.tier == BaselineRequirementTier::MandatoryDeterministic
        }));
        assert!(run_matrix.iter().any(|run| {
            run.baseline == BaselineSystem::TunedPresidio
                && run.tier == BaselineRequirementTier::MandatoryDeterministic
        }));
        assert!(run_matrix.iter().any(|run| {
            run.baseline == BaselineSystem::PokrovCurrentNative
                && run.tier == BaselineRequirementTier::MandatoryDeterministic
        }));
        assert!(run_matrix.iter().any(|run| {
            run.baseline == BaselineSystem::PokrovUpdatedNative
                && run.tier == BaselineRequirementTier::MandatoryDeterministic
        }));
        assert!(run_matrix.iter().any(|run| {
            run.baseline == BaselineSystem::NlmScrubber
                && run.tier == BaselineRequirementTier::OptionalFutureWorkstreams
        }));
        assert!(run_matrix
            .iter()
            .all(|run| run.required_metadata.contains(&"git_revision".to_string())));
    }
}
