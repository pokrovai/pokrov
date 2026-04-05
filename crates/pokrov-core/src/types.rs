use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod foundation;

pub use foundation::{
    foundation_evaluation_boundaries, foundation_extension_points, foundation_stage_boundaries,
    EvaluationArtifactBoundary, EvaluationArtifactClass, EvidenceClass, ExtensionPointContract,
    ExtensionPointKind, FoundationExecutionTrace, FoundationTransformResult, HitLocationKind,
    NormalizedHit, PipelineStageBoundary, ResolvedHit, StageArtifact, StageId, SuppressionStatus,
    TransformPlan, ValidationStatus,
};

/// Supported policy actions applied after detection and overlap resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyAction {
    Allow,
    Mask,
    Replace,
    Redact,
    Block,
}

impl PolicyAction {
    /// Returns strictness rank used for deterministic winner selection.
    pub const fn strictness_rank(self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::Mask => 1,
            Self::Replace => 2,
            Self::Redact => 3,
            Self::Block => 4,
        }
    }
}

/// Evaluation execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationMode {
    Enforce,
    DryRun,
}

/// Request class used for metadata-only audit context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathClass {
    #[default]
    Direct,
    Llm,
    Mcp,
}

/// Supported detection categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionCategory {
    Secrets,
    Pii,
    CorporateMarkers,
    Custom,
}

/// Deterministic validator applied to one matched candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterministicValidatorKind {
    None,
    Luhn,
}

/// Candidate normalization mode applied before deterministic validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterministicNormalizationMode {
    Preserve,
    Lowercase,
    AlnumLowercase,
}

/// Recognizer-scoped lexical context controls for deterministic pattern hits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeterministicContextPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub positive_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub negative_terms: Vec<String>,
    #[serde(default = "default_context_score_boost")]
    pub score_boost: i16,
    #[serde(default = "default_context_score_penalty")]
    pub score_penalty: i16,
    pub window: u8,
    pub suppress_on_negative: bool,
}

const fn default_context_score_boost() -> i16 {
    10
}

const fn default_context_score_penalty() -> i16 {
    10
}

/// Deterministic rule kind derived from profile recognizer configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeterministicRuleKind {
    Pattern {
        validator: DeterministicValidatorKind,
        normalization: DeterministicNormalizationMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<DeterministicContextPolicy>,
    },
    DenylistExact,
}

/// Optional deterministic metadata attached to custom rules generated from recognizers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeterministicRuleMetadata {
    pub recognizer_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowlist_exact: Vec<String>,
    pub rule: DeterministicRuleKind,
}

/// Custom detection rule defined by profile configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRule {
    pub rule_id: String,
    pub category: DetectionCategory,
    pub pattern: String,
    pub action: PolicyAction,
    pub priority: u16,
    pub replacement_template: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deterministic: Option<DeterministicRuleMetadata>,
}

/// Category defaults for built-in rule packs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryActions {
    pub secrets: PolicyAction,
    pub pii: PolicyAction,
    pub corporate_markers: PolicyAction,
    pub custom: PolicyAction,
}

impl CategoryActions {
    /// Returns default action for a category when a built-in hit is detected.
    pub const fn action_for(&self, category: DetectionCategory) -> PolicyAction {
        match category {
            DetectionCategory::Secrets => self.secrets,
            DetectionCategory::Pii => self.pii,
            DetectionCategory::CorporateMarkers => self.corporate_markers,
            DetectionCategory::Custom => self.custom,
        }
    }
}

/// Profile-level policy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyProfile {
    pub profile_id: String,
    pub mode_default: EvaluationMode,
    pub category_actions: CategoryActions,
    pub mask_visible_suffix: u8,
    #[serde(default = "default_max_hits_per_request")]
    pub max_hits_per_request: u32,
    pub custom_rules: Vec<CustomRule>,
    pub custom_rules_enabled: bool,
}

const fn default_max_hits_per_request() -> u32 {
    4096
}

/// Top-level evaluator configuration shared with runtime bootstrap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatorConfig {
    pub default_profile: String,
    pub profiles: BTreeMap<String, PolicyProfile>,
}

/// Public request passed from API layer into sanitization engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluateRequest {
    pub request_id: String,
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub payload: Value,
    pub path_class: PathClass,
    pub effective_language: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_scope_filters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recognizer_family_filters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowlist_additions: Vec<String>,
}

/// Raw detection hit before overlap resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionHit {
    pub rule_id: String,
    pub category: DetectionCategory,
    pub json_pointer: String,
    pub start: usize,
    pub end: usize,
    pub action: PolicyAction,
    pub priority: u16,
    pub replacement_template: Option<String>,
}

/// Non-overlapping span after deterministic overlap resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSpan {
    pub json_pointer: String,
    pub start: usize,
    pub end: usize,
    pub winning_rule_id: String,
    pub category: DetectionCategory,
    pub effective_action: PolicyAction,
    pub priority: u16,
    pub replacement_template: Option<String>,
    pub suppressed_rule_ids: Vec<String>,
}

/// Deterministic policy decision based on resolved spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluateDecision {
    pub final_action: PolicyAction,
    pub rule_hits_total: u32,
    pub deterministic_candidates_total: u32,
    pub suppressed_candidates_total: u32,
    pub hits_by_category: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hits_by_family: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
    pub resolved_locations: Vec<ResolvedLocationRecord>,
    pub replay_identity: String,
}

/// Supported resolved location classes shared by text and structured inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedLocationKind {
    TextSpan,
    JsonField,
    LogicalField,
}

/// Serializable metadata-only resolved location view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLocationRecord {
    pub location_kind: ResolvedLocationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_pointer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_field_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    pub category: DetectionCategory,
    pub effective_action: PolicyAction,
}

/// Result of applying transform actions to JSON string leaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformResult {
    pub final_action: PolicyAction,
    pub sanitized_payload: Option<Value>,
    pub blocked: bool,
    pub transformed_fields_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transform_metadata: Vec<String>,
}

/// Explainability category summary without raw fragments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainCategory {
    pub category: DetectionCategory,
    pub hits: u32,
    pub effective_action: PolicyAction,
}

/// Explainability payload returned to API consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainSummary {
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub final_action: PolicyAction,
    pub categories: Vec<ExplainCategory>,
    pub rule_hits_total: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub family_counts: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entity_counts: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confidence_buckets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degradation_markers: Vec<String>,
}

/// Metadata-only audit event safe for logs and counters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSummary {
    pub request_id: String,
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub final_action: PolicyAction,
    pub rule_hits_total: u32,
    pub hits_by_category: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counts_by_family: BTreeMap<String, u32>,
    pub duration_ms: u64,
    pub path_class: PathClass,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degradation_metadata: Vec<String>,
}

/// Metadata-only execution section reused by runtime and evaluation consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutedSummary {
    pub execution_enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages_completed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recognizer_families_executed: Vec<String>,
    pub transform_applied: bool,
}

/// Metadata-only degraded execution section reused by all consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegradedSummary {
    pub is_degraded: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    pub fail_closed_applied: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_execution_paths: Vec<String>,
}

/// Final evaluate response generated by the core pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluateResult {
    pub request_id: String,
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub decision: EvaluateDecision,
    pub transform: TransformResult,
    pub explain: ExplainSummary,
    pub audit: AuditSummary,
    pub executed: ExecutedSummary,
    pub degraded: DegradedSummary,
}

/// Core evaluation errors mapped to API response codes by adapter layer.
#[derive(Debug, thiserror::Error)]
pub enum EvaluateError {
    #[error("invalid profile '{0}'")]
    InvalidProfile(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("runtime failure: {0}")]
    RuntimeFailure(String),
}
