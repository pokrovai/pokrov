use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathClass {
    Direct,
    Llm,
    Mcp,
}

impl Default for PathClass {
    fn default() -> Self {
        Self::Direct
    }
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
    pub custom_rules: Vec<CustomRule>,
    pub custom_rules_enabled: bool,
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
    pub hits_by_category: BTreeMap<String, u32>,
    pub resolved_spans: Vec<ResolvedSpanView>,
    pub deterministic_signature: String,
}

/// Serializable metadata-only resolved span view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSpanView {
    pub category: DetectionCategory,
    pub effective_action: PolicyAction,
    pub start: usize,
    pub end: usize,
}

/// Result of applying transform actions to JSON string leaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformResult {
    pub final_action: PolicyAction,
    pub sanitized_payload: Option<Value>,
    pub blocked: bool,
    pub transformed_fields_count: u32,
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
    pub duration_ms: u64,
    pub path_class: PathClass,
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
    pub executed: bool,
}

/// Core evaluation errors mapped to API response codes by adapter layer.
#[derive(Debug, thiserror::Error)]
pub enum EvaluateError {
    #[error("invalid profile '{0}'")]
    InvalidProfile(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid profile config: {0}")]
    InvalidProfileConfig(String),
}
