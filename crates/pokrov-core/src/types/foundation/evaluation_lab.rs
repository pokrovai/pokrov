use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::FoundationTransformResult;
use crate::types::{
    AuditSummary, DegradedSummary, EvaluateResult, ExecutedSummary, ExplainSummary, PolicyAction,
};

/// Supported evaluation case mode families for the v1 lab foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCaseMode {
    Text,
    StructuredJson,
    BatchStructured,
    ImageOcr,
}

/// Canonical corpus sources accepted by evaluation cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCaseSource {
    Synthetic,
    CuratedGold,
    Adversarial,
}

/// Stable replayable evaluation case contract for parity and readiness workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationCase {
    pub case_id: String,
    pub language: String,
    pub mode: EvaluationCaseMode,
    pub input: Value,
    pub expected_entities: Vec<String>,
    pub expected_operator_outcome: PolicyAction,
    pub expected_policy_outcome: PolicyAction,
    pub tags: Vec<String>,
    pub source: EvaluationCaseSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Evaluated operator correctness for one case without exposing raw payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorOutcome {
    Match,
    Mismatch,
}

/// Runtime-compatible evaluation result projection used by report generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub case_id: String,
    pub language: String,
    pub mode: EvaluationCaseMode,
    pub source: EvaluationCaseSource,
    pub expected_operator_outcome: PolicyAction,
    pub expected_policy_outcome: PolicyAction,
    pub actual_operator_outcome: PolicyAction,
    pub actual_policy_outcome: PolicyAction,
    pub operator_outcome: OperatorOutcome,
    pub policy_outcome_match: bool,
    pub decision: crate::types::EvaluateDecision,
    pub transform: FoundationTransformResult,
    pub explain: ExplainSummary,
    pub audit: AuditSummary,
    pub executed: ExecutedSummary,
    pub degraded: DegradedSummary,
}

impl EvaluationResult {
    /// Converts a runtime evaluation into a lab result without per-family adapters.
    pub fn from_runtime_contract(case: &EvaluationCase, runtime: &EvaluateResult) -> Self {
        let actual_operator_outcome = runtime.transform.final_action;
        let actual_policy_outcome = runtime.decision.final_action;
        let operator_outcome = if actual_operator_outcome == case.expected_operator_outcome {
            OperatorOutcome::Match
        } else {
            OperatorOutcome::Mismatch
        };

        Self {
            case_id: case.case_id.clone(),
            language: case.language.clone(),
            mode: case.mode,
            source: case.source,
            expected_operator_outcome: case.expected_operator_outcome,
            expected_policy_outcome: case.expected_policy_outcome,
            actual_operator_outcome,
            actual_policy_outcome,
            operator_outcome,
            policy_outcome_match: actual_policy_outcome == case.expected_policy_outcome,
            decision: runtime.decision.clone(),
            transform: FoundationTransformResult::from_transform_result(&runtime.transform),
            explain: runtime.explain.clone(),
            audit: runtime.audit.clone(),
            executed: runtime.executed.clone(),
            degraded: runtime.degraded.clone(),
        }
    }
}

/// Fixed detection metric dimensions consumed by parity and readiness reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionMetrics {
    pub precision: f64,
    pub recall: f64,
    pub f2: f64,
    pub per_entity_breakdown: BTreeMap<String, f64>,
    pub per_family_breakdown: BTreeMap<String, f64>,
    pub per_language_breakdown: BTreeMap<String, f64>,
}

/// Fixed parity metric dimensions compared with Presidio baselines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParityMetrics {
    pub detection_delta_vs_presidio: f64,
    pub operator_delta_vs_presidio: f64,
    pub coverage_delta_vs_presidio: f64,
}

/// Fixed safety metric dimensions for leakage and bypass posture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityMetrics {
    pub leakage_checks_passed: bool,
    pub fail_closed_correctness: bool,
    pub adversarial_bypass_rate: f64,
}

/// Fixed runtime metric dimensions for overhead tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub p50_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub native_recognizer_cost_ms: u64,
    pub remote_recognizer_cost_ms: u64,
}

/// Fixed transformation metric dimensions for operator and JSON correctness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformationMetrics {
    pub operator_correctness: f64,
    pub overlap_correctness: f64,
    pub json_validity_preservation: f64,
}

/// Groups all report-stable metric dimensions frozen for threshold evolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationMetricGroups {
    pub detection: DetectionMetrics,
    pub parity: ParityMetrics,
    pub security: SecurityMetrics,
    pub runtime: RuntimeMetrics,
    pub transformation: TransformationMetrics,
}

/// Report outputs emitted by the evaluation cycle and consumed by quality gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportOutputKind {
    FamilySummary,
    EntityBreakdown,
    ParityReport,
    SafetyAndLeakage,
    ReadinessScoreboard,
}

/// Corpus definitions frozen for the evaluation lab foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCorpusKind {
    Synthetic,
    CuratedGold,
    Adversarial,
}

/// Minimum requirements per corpus kind for deterministic replay consistency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationCorpusDefinition {
    pub kind: EvaluationCorpusKind,
    pub purpose: String,
    pub required_contents: Vec<String>,
}

/// Progressive quality-gate levels for readiness rollout control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGateLevel {
    Level0,
    Level1,
    Level2,
    Level3,
}

/// Declarative gate descriptor that keeps thresholds out of schema shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityGateDefinition {
    pub level: QualityGateLevel,
    pub blocking: bool,
    pub required_outputs: Vec<ReportOutputKind>,
    pub description: String,
}

/// Stable cycle-level evaluation report that aggregates results and metric groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub report_id: String,
    pub total_cases: u32,
    pub passed_cases: u32,
    pub metric_groups: EvaluationMetricGroups,
    pub output_kinds: Vec<ReportOutputKind>,
}

/// Stable parity report shape against a named baseline provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParityReport {
    pub report_id: String,
    pub baseline: String,
    pub metrics: ParityMetrics,
    pub family_deltas: BTreeMap<String, f64>,
    pub entity_deltas: BTreeMap<String, f64>,
}

/// Stable readiness scoreboard shape with explicit gate dependencies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadinessScoreboard {
    pub scoreboard_id: String,
    pub gate_levels: Vec<QualityGateDefinition>,
    pub metric_groups: EvaluationMetricGroups,
    pub output_kinds: Vec<ReportOutputKind>,
}

/// Returns the frozen corpus responsibilities required by the evaluation foundation.
pub fn foundation_evaluation_corpora() -> Vec<EvaluationCorpusDefinition> {
    vec![
        EvaluationCorpusDefinition {
            kind: EvaluationCorpusKind::Synthetic,
            purpose: "cover deterministic families cheaply and completely".to_string(),
            required_contents: vec![
                "regex variants".to_string(),
                "checksum valid and invalid pairs".to_string(),
                "context boost and suppression pairs".to_string(),
                "allowlist and denylist cases".to_string(),
                "overlap cases".to_string(),
                "operator cases".to_string(),
                "nested JSON cases".to_string(),
            ],
        },
        EvaluationCorpusDefinition {
            kind: EvaluationCorpusKind::CuratedGold,
            purpose: "measure realistic behavior on de-identified examples".to_string(),
            required_contents: vec![
                "EN and RU prompts".to_string(),
                "tool arguments and outputs".to_string(),
                "structured JSON cases".to_string(),
                "hard negatives".to_string(),
            ],
        },
        EvaluationCorpusDefinition {
            kind: EvaluationCorpusKind::Adversarial,
            purpose: "measure bypass resistance".to_string(),
            required_contents: vec![
                "spacing and punctuation obfuscation".to_string(),
                "unicode confusables".to_string(),
                "mixed-language strings".to_string(),
                "fragmented JSON patterns".to_string(),
                "simple exfiltration-oriented disguises".to_string(),
            ],
        },
    ]
}

/// Returns the progressive quality-gate model while keeping thresholds external.
pub fn foundation_quality_gates() -> Vec<QualityGateDefinition> {
    vec![
        QualityGateDefinition {
            level: QualityGateLevel::Level0,
            blocking: false,
            required_outputs: vec![ReportOutputKind::FamilySummary, ReportOutputKind::EntityBreakdown],
            description: "baseline collection only; no blocking gates".to_string(),
        },
        QualityGateDefinition {
            level: QualityGateLevel::Level1,
            blocking: true,
            required_outputs: vec![
                ReportOutputKind::FamilySummary,
                ReportOutputKind::EntityBreakdown,
                ReportOutputKind::ParityReport,
            ],
            description: "deterministic family regression gates against stable baselines".to_string(),
        },
        QualityGateDefinition {
            level: QualityGateLevel::Level2,
            blocking: true,
            required_outputs: vec![
                ReportOutputKind::FamilySummary,
                ReportOutputKind::EntityBreakdown,
                ReportOutputKind::ParityReport,
                ReportOutputKind::SafetyAndLeakage,
            ],
            description: "priority entity thresholds and deterministic rollout gates".to_string(),
        },
        QualityGateDefinition {
            level: QualityGateLevel::Level3,
            blocking: true,
            required_outputs: vec![
                ReportOutputKind::FamilySummary,
                ReportOutputKind::EntityBreakdown,
                ReportOutputKind::ParityReport,
                ReportOutputKind::SafetyAndLeakage,
                ReportOutputKind::ReadinessScoreboard,
            ],
            description: "structured and remote-flow gates with release-significant scoreboards"
                .to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{foundation_evaluation_corpora, foundation_quality_gates, EvaluationCorpusKind};

    #[test]
    fn corpus_definitions_cover_all_required_kinds() {
        let corpora = foundation_evaluation_corpora();

        assert_eq!(corpora.len(), 3);
        assert!(corpora.iter().any(|corpus| corpus.kind == EvaluationCorpusKind::Synthetic));
        assert!(corpora.iter().any(|corpus| corpus.kind == EvaluationCorpusKind::CuratedGold));
        assert!(corpora.iter().any(|corpus| corpus.kind == EvaluationCorpusKind::Adversarial));
        assert!(corpora.iter().all(|corpus| !corpus.required_contents.is_empty()));
    }

    #[test]
    fn quality_gates_are_progressive_and_non_empty() {
        let gates = foundation_quality_gates();

        assert_eq!(gates.len(), 4);
        assert!(!gates[0].blocking);
        assert!(gates[1..].iter().all(|gate| gate.blocking));
        assert!(gates.iter().all(|gate| !gate.required_outputs.is_empty()));
    }
}
