use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use audit::{build_audit_summary, build_explain_summary};
use detection::{compile_custom_rules, detect_payload, CompiledCustomRule};
use dry_run::is_execution_enabled;
use policy::{category_hit_counts, resolve_overlaps, select_final_action};
use transform::apply_transforms;

use crate::types::{
    DegradedSummary, EvaluateDecision, EvaluateError, EvaluateRequest, EvaluateResult,
    EvaluatorConfig, ExecutedSummary, FoundationExecutionTrace, FoundationTransformResult,
    NormalizedHit, PolicyProfile, ResolvedHit, ResolvedLocationKind, ResolvedLocationRecord,
    ResolvedSpan, TransformPlan,
};

pub mod audit;
pub mod detection;
pub mod dry_run;
pub mod policy;
pub mod transform;
pub mod traversal;
pub mod types;

#[derive(Debug, Clone)]
struct CompiledProfile {
    profile: PolicyProfile,
    custom_rules: Vec<CompiledCustomRule>,
}

#[derive(Debug, Clone)]
struct EvaluationArtifacts {
    profile_id: String,
    mask_visible_suffix: u8,
    hits: Vec<crate::types::DetectionHit>,
    resolved_spans: Vec<ResolvedSpan>,
    decision: EvaluateDecision,
    transform: crate::types::TransformResult,
    explain: crate::types::ExplainSummary,
    audit: crate::types::AuditSummary,
    executed: ExecutedSummary,
    degraded: DegradedSummary,
}

/// Evaluates sanitization requests against the configured policy profiles.
#[derive(Debug, Clone)]
pub struct SanitizationEngine {
    default_profile: String,
    profiles: Arc<BTreeMap<String, CompiledProfile>>,
}

impl SanitizationEngine {
    /// Builds a sanitization engine from the static evaluator configuration.
    pub fn new(config: EvaluatorConfig) -> Result<Self, EvaluateError> {
        let mut profiles = BTreeMap::new();

        for (profile_id, profile) in config.profiles {
            if profile.mask_visible_suffix > 8 {
                return Err(EvaluateError::InvalidProfile(format!(
                    "profile '{}' mask_visible_suffix must be <= 8",
                    profile_id
                )));
            }

            let custom_rules = compile_custom_rules(&profile)?;
            profiles.insert(profile_id.clone(), CompiledProfile { profile, custom_rules });
        }

        if !profiles.contains_key(&config.default_profile) {
            return Err(EvaluateError::InvalidProfile(format!(
                "default profile '{}' is missing",
                config.default_profile
            )));
        }

        Ok(Self { default_profile: config.default_profile, profiles: Arc::new(profiles) })
    }

    /// Evaluates one payload through the current sanitization pipeline.
    pub fn evaluate(&self, request: EvaluateRequest) -> Result<EvaluateResult, EvaluateError> {
        let artifacts = self.evaluate_internal(&request)?;

        Ok(EvaluateResult {
            request_id: request.request_id,
            profile_id: artifacts.profile_id,
            mode: request.mode,
            decision: artifacts.decision,
            transform: artifacts.transform,
            explain: artifacts.explain,
            audit: artifacts.audit,
            executed: artifacts.executed,
            degraded: artifacts.degraded,
        })
    }

    /// Produces the shared foundation contract trace for runtime and evaluation proofs.
    pub fn trace_foundation_flow(
        &self,
        request: EvaluateRequest,
    ) -> Result<FoundationExecutionTrace, EvaluateError> {
        let artifacts = self.evaluate_internal(&request)?;
        let resolved_hits = artifacts
            .resolved_spans
            .iter()
            .map(ResolvedHit::from_resolved_span)
            .collect::<Vec<_>>();
        let result = EvaluateResult {
            request_id: request.request_id.clone(),
            profile_id: artifacts.profile_id,
            mode: request.mode,
            decision: artifacts.decision.clone(),
            transform: artifacts.transform.clone(),
            explain: artifacts.explain,
            audit: artifacts.audit,
            executed: artifacts.executed,
            degraded: artifacts.degraded,
        };

        Ok(FoundationExecutionTrace::from_contracts(
            &request,
            &result,
            artifacts.hits.iter().map(NormalizedHit::from_detection_hit).collect::<Vec<_>>(),
            resolved_hits,
            TransformPlan::from_decision(
                request.mode,
                &artifacts.resolved_spans,
                &result.decision,
                artifacts.mask_visible_suffix,
            ),
            FoundationTransformResult::from_transform_result(&result.transform),
        ))
    }

    fn evaluate_internal(
        &self,
        request: &EvaluateRequest,
    ) -> Result<EvaluationArtifacts, EvaluateError> {
        if request.request_id.trim().is_empty() {
            return Err(EvaluateError::InvalidInput("request_id must not be empty".to_string()));
        }

        if request.effective_language.trim().is_empty() {
            return Err(EvaluateError::InvalidInput(
                "effective_language must not be empty".to_string(),
            ));
        }

        let profile_id = if request.profile_id.trim().is_empty() {
            self.default_profile.clone()
        } else {
            request.profile_id.clone()
        };

        let Some(compiled_profile) = self.profiles.get(&profile_id) else {
            return Err(EvaluateError::InvalidProfile(profile_id));
        };

        let started = Instant::now();
        let hits = detect_payload(
            &request.payload,
            &compiled_profile.profile,
            &compiled_profile.custom_rules,
            &request.allowlist_additions,
        );
        let resolved_spans = resolve_overlaps(&hits);
        let final_action = select_final_action(&resolved_spans);
        let hits_by_category = category_hit_counts(&hits);

        let resolved_locations = resolved_spans
            .iter()
            .map(|span| ResolvedLocationRecord {
                location_kind: ResolvedLocationKind::JsonField,
                json_pointer: Some(span.json_pointer.clone()),
                logical_field_path: Some(logical_field_path(&span.json_pointer)),
                category: span.category,
                effective_action: span.effective_action,
                start: Some(span.start),
                end: Some(span.end),
            })
            .collect::<Vec<_>>();

        let decision = EvaluateDecision {
            final_action,
            rule_hits_total: hits.len() as u32,
            deterministic_candidates_total: hits
                .iter()
                .filter(|hit| hit.rule_id.starts_with("deterministic."))
                .count() as u32,
            suppressed_candidates_total: suppressed_candidates_total(&resolved_spans),
            hits_by_category,
            hits_by_family: family_counts(&hits, resolved_spans.len() as u32),
            reason_codes: decision_reason_codes(&resolved_spans),
            resolved_locations,
            replay_identity: replay_identity(&profile_id, request, &resolved_spans),
        };

        let transform = apply_transforms(
            &request.payload,
            &resolved_spans,
            decision.final_action,
            compiled_profile.profile.mask_visible_suffix,
        );
        let executed = ExecutedSummary {
            execution_enabled: is_execution_enabled(request.mode),
            stages_completed: vec![
                "input_normalization".to_string(),
                "recognizer_execution".to_string(),
                "analysis_and_suppression".to_string(),
                "policy_resolution".to_string(),
                "transformation".to_string(),
                "safe_explain".to_string(),
                "audit_summary".to_string(),
            ],
            recognizer_families_executed: recognizer_families_executed(
                &compiled_profile.custom_rules,
            ),
            transform_applied: transform.transformed_fields_count > 0,
        };
        let degraded = DegradedSummary {
            is_degraded: false,
            reasons: Vec::new(),
            fail_closed_applied: false,
            missing_execution_paths: Vec::new(),
        };
        let explain = build_explain_summary(
            &profile_id,
            request.mode,
            &decision,
            &resolved_spans,
            &executed,
            &degraded,
        );
        let audit = build_audit_summary(
            request,
            &profile_id,
            &decision,
            &resolved_spans,
            &executed,
            &degraded,
            started.elapsed(),
        );

        Ok(EvaluationArtifacts {
            profile_id,
            mask_visible_suffix: compiled_profile.profile.mask_visible_suffix,
            hits,
            resolved_spans,
            decision,
            transform,
            explain,
            audit,
            executed,
            degraded,
        })
    }
}

fn recognizer_families_executed(custom_rules: &[CompiledCustomRule]) -> Vec<String> {
    let mut families = vec!["builtin".to_string()];
    if custom_rules.iter().any(|rule| rule.rule_id.starts_with("deterministic.")) {
        families.push("deterministic".to_string());
    }
    if custom_rules.iter().any(|rule| rule.rule_id.starts_with("custom.")) {
        families.push("custom".to_string());
    }
    families
}

fn replay_identity(
    profile_id: &str,
    request: &EvaluateRequest,
    resolved_spans: &[crate::types::ResolvedSpan],
) -> String {
    let mut hasher = DefaultHasher::new();
    profile_id.hash(&mut hasher);
    request.mode.hash(&mut hasher);
    request.path_class.hash(&mut hasher);
    request.effective_language.hash(&mut hasher);
    request.entity_scope_filters.hash(&mut hasher);
    request.recognizer_family_filters.hash(&mut hasher);
    request.allowlist_additions.hash(&mut hasher);

    for span in resolved_spans {
        span.json_pointer.hash(&mut hasher);
        span.start.hash(&mut hasher);
        span.end.hash(&mut hasher);
        span.winning_rule_id.hash(&mut hasher);
        span.category.hash(&mut hasher);
        span.effective_action.hash(&mut hasher);
        span.priority.hash(&mut hasher);
    }

    format!("{:016x}", hasher.finish())
}

fn family_counts(
    hits: &[crate::types::DetectionHit],
    resolved_hits_total: u32,
) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::from([
        ("normalized_hit".to_string(), hits.len() as u32),
        ("resolved_hit".to_string(), resolved_hits_total),
        ("builtin".to_string(), 0),
        ("custom".to_string(), 0),
        ("deterministic".to_string(), 0),
    ]);

    for hit in hits {
        let key = if hit.rule_id.starts_with("deterministic.") {
            "deterministic"
        } else if hit.rule_id.starts_with("custom.") {
            "custom"
        } else {
            "builtin"
        };
        if let Some(value) = counts.get_mut(key) {
            *value += 1;
        }
    }

    counts
}

fn decision_reason_codes(resolved_spans: &[crate::types::ResolvedSpan]) -> Vec<String> {
    resolved_spans.iter().map(|span| format!("winner:{}", span.winning_rule_id)).collect()
}

fn suppressed_candidates_total(resolved_spans: &[crate::types::ResolvedSpan]) -> u32 {
    resolved_spans.iter().map(|span| span.suppressed_rule_ids.len() as u32).sum()
}

fn logical_field_path(pointer: &str) -> String {
    pointer
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
