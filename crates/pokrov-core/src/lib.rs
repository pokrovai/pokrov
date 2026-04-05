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
    foundation_evaluation_boundaries, foundation_extension_points, foundation_stage_boundaries,
    EvaluateDecision, EvaluateError, EvaluateRequest, EvaluateResult, EvaluatorConfig,
    FoundationExecutionTrace, FoundationTransformResult, NormalizedHit, PolicyProfile, ResolvedHit,
    ResolvedSpan, ResolvedSpanView, TransformPlan,
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
    executed: bool,
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
                return Err(EvaluateError::InvalidProfileConfig(format!(
                    "profile '{}' mask_visible_suffix must be <= 8",
                    profile_id
                )));
            }

            let custom_rules = compile_custom_rules(&profile)?;
            profiles.insert(profile_id.clone(), CompiledProfile { profile, custom_rules });
        }

        if !profiles.contains_key(&config.default_profile) {
            return Err(EvaluateError::InvalidProfileConfig(format!(
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
        })
    }

    /// Produces the shared foundation contract trace for runtime and evaluation proofs.
    pub fn trace_foundation_flow(
        &self,
        request: EvaluateRequest,
    ) -> Result<FoundationExecutionTrace, EvaluateError> {
        let request_id = request.request_id.clone();
        let mode = request.mode;
        let path_class = request.path_class;
        let artifacts = self.evaluate_internal(&request)?;
        let resolved_hits = artifacts
            .resolved_spans
            .iter()
            .map(ResolvedHit::from_resolved_span)
            .collect::<Vec<_>>();

        Ok(FoundationExecutionTrace {
            request_id,
            profile_id: artifacts.profile_id,
            mode,
            path_class,
            stage_boundaries: foundation_stage_boundaries(),
            extension_points: foundation_extension_points(),
            normalized_hits: artifacts
                .hits
                .iter()
                .map(NormalizedHit::from_detection_hit)
                .collect::<Vec<_>>(),
            resolved_hits: resolved_hits.clone(),
            transform_plan: TransformPlan::from_decision(
                mode,
                &artifacts.resolved_spans,
                &artifacts.decision,
                artifacts.mask_visible_suffix,
            ),
            transform_result: FoundationTransformResult::from_transform_result(&artifacts.transform),
            explain: artifacts.explain,
            audit: artifacts.audit,
            evaluation_boundaries: foundation_evaluation_boundaries(),
            executed: artifacts.executed,
        })
    }

    fn evaluate_internal(&self, request: &EvaluateRequest) -> Result<EvaluationArtifacts, EvaluateError> {
        if request.request_id.trim().is_empty() {
            return Err(EvaluateError::InvalidRequest("request_id must not be empty".to_string()));
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
        let hits = detect_payload(&request.payload, &compiled_profile.profile, &compiled_profile.custom_rules);
        let resolved_spans = resolve_overlaps(hits.clone());
        let final_action = select_final_action(&resolved_spans);
        let hits_by_category = category_hit_counts(&hits);

        let resolved_span_views = resolved_spans
            .iter()
            .map(|span| ResolvedSpanView {
                category: span.category,
                effective_action: span.effective_action,
                start: span.start,
                end: span.end,
            })
            .collect::<Vec<_>>();

        let decision = EvaluateDecision {
            final_action,
            rule_hits_total: hits.len() as u32,
            hits_by_category,
            resolved_spans: resolved_span_views,
            deterministic_signature: deterministic_signature(&profile_id, &resolved_spans),
        };

        let transform = apply_transforms(
            &request.payload,
            &resolved_spans,
            decision.final_action,
            compiled_profile.profile.mask_visible_suffix,
        );
        let explain = build_explain_summary(&profile_id, request.mode, &decision, &resolved_spans);
        let audit = build_audit_summary(
            request,
            &profile_id,
            &decision,
            &resolved_spans,
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
            executed: is_execution_enabled(request.mode),
        })
    }
}

fn deterministic_signature(profile_id: &str, resolved_spans: &[crate::types::ResolvedSpan]) -> String {
    let mut hasher = DefaultHasher::new();
    profile_id.hash(&mut hasher);

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::{
        types::{
            CategoryActions, CustomRule, DetectionCategory, EvaluateRequest, EvaluationMode,
            EvaluatorConfig, PathClass, PolicyAction, PolicyProfile,
        },
        SanitizationEngine,
    };

    fn engine_with_single_profile(profile: PolicyProfile) -> SanitizationEngine {
        SanitizationEngine::new(EvaluatorConfig {
            default_profile: profile.profile_id.clone(),
            profiles: BTreeMap::from([(profile.profile_id.clone(), profile)]),
        })
        .expect("engine should build")
    }

    fn engine() -> SanitizationEngine {
        let strict = PolicyProfile {
            profile_id: "strict".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Block,
                pii: PolicyAction::Redact,
                corporate_markers: PolicyAction::Mask,
                custom: PolicyAction::Redact,
            },
            mask_visible_suffix: 4,
            custom_rules_enabled: true,
            custom_rules: vec![CustomRule {
                rule_id: "custom.project_andromeda".to_string(),
                category: DetectionCategory::CorporateMarkers,
                pattern: "(?i)project\\s+andromeda".to_string(),
                action: PolicyAction::Redact,
                priority: 900,
                replacement_template: None,
                enabled: true,
            }],
        };

        let minimal = PolicyProfile {
            profile_id: "minimal".to_string(),
            mode_default: EvaluationMode::Enforce,
            category_actions: CategoryActions {
                secrets: PolicyAction::Mask,
                pii: PolicyAction::Allow,
                corporate_markers: PolicyAction::Allow,
                custom: PolicyAction::Allow,
            },
            mask_visible_suffix: 4,
            custom_rules_enabled: false,
            custom_rules: Vec::new(),
        };

        let custom = PolicyProfile {
            profile_id: "custom".to_string(),
            mode_default: EvaluationMode::DryRun,
            category_actions: CategoryActions {
                secrets: PolicyAction::Redact,
                pii: PolicyAction::Mask,
                corporate_markers: PolicyAction::Mask,
                custom: PolicyAction::Redact,
            },
            mask_visible_suffix: 3,
            custom_rules_enabled: true,
            custom_rules: Vec::new(),
        };

        let profiles = BTreeMap::from([
            ("minimal".to_string(), minimal),
            ("strict".to_string(), strict),
            ("custom".to_string(), custom),
        ]);

        SanitizationEngine::new(EvaluatorConfig { default_profile: "strict".to_string(), profiles })
            .expect("engine should build")
    }

    #[test]
    fn deterministic_replay_returns_same_signature() {
        let engine = engine();
        let payload = json!({"content": "Project Andromeda card 4111 1111 1111 1111"});

        let one = engine
            .evaluate(EvaluateRequest {
                request_id: "r-1".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload: payload.clone(),
                path_class: PathClass::Direct,
            })
            .expect("first evaluation should pass");

        let two = engine
            .evaluate(EvaluateRequest {
                request_id: "r-2".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload,
                path_class: PathClass::Direct,
            })
            .expect("second evaluation should pass");

        assert_eq!(one.decision.deterministic_signature, two.decision.deterministic_signature);
        assert_eq!(one.decision.final_action, two.decision.final_action);
    }

    #[test]
    fn empty_profile_id_uses_default_profile_for_result_and_audit() {
        let engine = engine();

        let result = engine
            .evaluate(EvaluateRequest {
                request_id: "r-default".to_string(),
                profile_id: String::new(),
                mode: EvaluationMode::Enforce,
                payload: json!({"content": "sk-test-abc12345"}),
                path_class: PathClass::Direct,
            })
            .expect("evaluation should pass");

        assert_eq!(result.profile_id, "strict");
        assert_eq!(result.explain.profile_id, "strict");
        assert_eq!(result.audit.profile_id, "strict");
    }

    #[test]
    fn conflicting_partial_transforms_stay_deterministic_across_rule_order() {
        fn engine_with_rules(custom_rules: Vec<CustomRule>) -> SanitizationEngine {
            engine_with_single_profile(PolicyProfile {
                profile_id: "strict".to_string(),
                mode_default: EvaluationMode::Enforce,
                category_actions: CategoryActions {
                    secrets: PolicyAction::Allow,
                    pii: PolicyAction::Allow,
                    corporate_markers: PolicyAction::Allow,
                    custom: PolicyAction::Allow,
                },
                mask_visible_suffix: 4,
                custom_rules_enabled: true,
                custom_rules,
            })
        }

        let broader_replace = CustomRule {
            rule_id: "custom.alpha_broader".to_string(),
            category: DetectionCategory::Custom,
            pattern: "(?i)alpha\\s+secret".to_string(),
            action: PolicyAction::Replace,
            priority: 120,
            replacement_template: Some("[CUSTOM_REPLACED]".to_string()),
            enabled: true,
        };
        let inner_redact = CustomRule {
            rule_id: "custom.alpha_inner".to_string(),
            category: DetectionCategory::Custom,
            pattern: "(?i)secret".to_string(),
            action: PolicyAction::Redact,
            priority: 80,
            replacement_template: None,
            enabled: true,
        };

        let engine_forward = engine_with_rules(vec![broader_replace.clone(), inner_redact.clone()]);
        let engine_reversed = engine_with_rules(vec![inner_redact, broader_replace]);

        let payload = json!({"message": "alpha secret token"});
        let one = engine_forward
            .evaluate(EvaluateRequest {
                request_id: "r-forward".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload: payload.clone(),
                path_class: PathClass::Direct,
            })
            .expect("forward evaluation should pass");
        let two = engine_reversed
            .evaluate(EvaluateRequest {
                request_id: "r-reversed".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload,
                path_class: PathClass::Direct,
            })
            .expect("reversed evaluation should pass");

        assert_eq!(one.decision.final_action, PolicyAction::Redact);
        assert_eq!(one.decision.final_action, two.decision.final_action);
        assert_eq!(one.decision.deterministic_signature, two.decision.deterministic_signature);
        assert_eq!(one.transform.sanitized_payload, two.transform.sanitized_payload);

        let transformed_text = one
            .transform
            .sanitized_payload
            .as_ref()
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str())
            .expect("sanitized message should exist");
        assert!(
            !transformed_text.contains("secret"),
            "resolved winner span must not leave partial sensitive passthrough"
        );
    }

    #[test]
    fn foundation_trace_does_not_export_sanitized_payload_content() {
        let engine = engine();
        let trace = engine
            .trace_foundation_flow(EvaluateRequest {
                request_id: "r-trace".to_string(),
                profile_id: "minimal".to_string(),
                mode: EvaluationMode::Enforce,
                payload: json!({"message": "hello from Project X, token sk-test-abc12345"}),
                path_class: PathClass::Direct,
            })
            .expect("trace should build");

        let serialized = serde_json::to_string(&trace).expect("trace must serialize");

        assert!(!serialized.contains("hello from Project X"));
        assert!(!serialized.contains("sk-test-abc12345"));
        assert!(!serialized.contains("abc12345"));
    }

    #[test]
    fn foundation_trace_plan_changes_when_mask_suffix_changes() {
        fn engine_with_mask_suffix(mask_visible_suffix: u8) -> SanitizationEngine {
            engine_with_single_profile(PolicyProfile {
                profile_id: "strict".to_string(),
                mode_default: EvaluationMode::Enforce,
                category_actions: CategoryActions {
                    secrets: PolicyAction::Allow,
                    pii: PolicyAction::Allow,
                    corporate_markers: PolicyAction::Allow,
                    custom: PolicyAction::Allow,
                },
                mask_visible_suffix,
                custom_rules_enabled: true,
                custom_rules: vec![CustomRule {
                    rule_id: "custom.mask_secret".to_string(),
                    category: DetectionCategory::Custom,
                    pattern: "secret-[0-9]+".to_string(),
                    action: PolicyAction::Mask,
                    priority: 300,
                    replacement_template: None,
                    enabled: true,
                }],
            })
        }

        let request = EvaluateRequest {
            request_id: "r-mask".to_string(),
            profile_id: "strict".to_string(),
            mode: EvaluationMode::Enforce,
            payload: json!({"message": "token secret-123456"}),
            path_class: PathClass::Direct,
        };
        let engine_suffix_2 = engine_with_mask_suffix(2);
        let engine_suffix_4 = engine_with_mask_suffix(4);
        let result_suffix_2 = engine_suffix_2
            .evaluate(request.clone())
            .expect("evaluation with suffix 2 should pass");
        let result_suffix_4 = engine_suffix_4
            .evaluate(request.clone())
            .expect("evaluation with suffix 4 should pass");
        let trace_suffix_2 = engine_suffix_2
            .trace_foundation_flow(request.clone())
            .expect("trace with suffix 2 should build");
        let trace_suffix_4 = engine_suffix_4
            .trace_foundation_flow(request)
            .expect("trace with suffix 4 should build");

        assert_ne!(
            result_suffix_2.transform.sanitized_payload,
            result_suffix_4.transform.sanitized_payload,
            "different mask suffixes must change runtime output"
        );
        assert_ne!(
            trace_suffix_2.transform_plan, trace_suffix_4.transform_plan,
            "different mask suffixes must change the exported transform plan"
        );
    }
}
