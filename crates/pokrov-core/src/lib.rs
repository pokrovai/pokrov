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
    DegradedSummary, EvaluateDecision, EvaluateError, EvaluateRequest, EvaluateResult, EvaluatorConfig,
    ExecutedSummary, FoundationExecutionTrace, FoundationTransformResult, NormalizedHit, PolicyProfile,
    ResolvedHit, ResolvedLocationKind, ResolvedLocationRecord, ResolvedSpan, TransformPlan,
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
            artifacts
                .hits
                .iter()
                .map(NormalizedHit::from_detection_hit)
                .collect::<Vec<_>>(),
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

    fn evaluate_internal(&self, request: &EvaluateRequest) -> Result<EvaluationArtifacts, EvaluateError> {
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
        let hits = detect_payload(&request.payload, &compiled_profile.profile, &compiled_profile.custom_rules);
        let resolved_spans = resolve_overlaps(hits.clone());
        let final_action = select_final_action(&resolved_spans);
        let hits_by_category = category_hit_counts(&hits);

        let resolved_locations = resolved_spans
            .iter()
            .map(|span| ResolvedLocationRecord {
                location_kind: ResolvedLocationKind::JsonField,
                json_pointer: Some(span.json_pointer.clone()),
                logical_field_path: None,
                category: span.category,
                effective_action: span.effective_action,
                start: Some(span.start),
                end: Some(span.end),
            })
            .collect::<Vec<_>>();

        let decision = EvaluateDecision {
            final_action,
            rule_hits_total: hits.len() as u32,
            hits_by_category,
            hits_by_family: family_counts(hits.len() as u32, resolved_spans.len() as u32),
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
            recognizer_families_executed: recognizer_families_executed(&compiled_profile.custom_rules),
            transform_applied: transform.transformed_fields_count > 0,
        };
        let degraded = DegradedSummary {
            is_degraded: false,
            reasons: Vec::new(),
            fail_closed_applied: false,
            missing_execution_paths: Vec::new(),
        };
        let explain =
            build_explain_summary(&profile_id, request.mode, &decision, &resolved_spans, &executed, &degraded);
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
    if !custom_rules.is_empty() {
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

fn family_counts(rule_hits_total: u32, resolved_hits_total: u32) -> BTreeMap<String, u32> {
    BTreeMap::from([
        ("normalized_hit".to_string(), rule_hits_total),
        ("resolved_hit".to_string(), resolved_hits_total),
    ])
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
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("first evaluation should pass");

        let two = engine
            .evaluate(EvaluateRequest {
                request_id: "r-2".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload,
                path_class: PathClass::Direct,
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("second evaluation should pass");

        assert_eq!(one.decision.replay_identity, two.decision.replay_identity);
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
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
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
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("forward evaluation should pass");
        let two = engine_reversed
            .evaluate(EvaluateRequest {
                request_id: "r-reversed".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload,
                path_class: PathClass::Direct,
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("reversed evaluation should pass");

        assert_eq!(one.decision.final_action, PolicyAction::Redact);
        assert_eq!(one.decision.final_action, two.decision.final_action);
        assert_eq!(one.decision.replay_identity, two.decision.replay_identity);
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
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
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
            effective_language: "en".to_string(),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
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

    #[test]
    fn executed_recognizer_families_include_custom_when_enabled_rules_exist() {
        let engine = engine();
        let result = engine
            .evaluate(EvaluateRequest {
                request_id: "r-executed-custom".to_string(),
                profile_id: "strict".to_string(),
                mode: EvaluationMode::Enforce,
                payload: json!({"content": "Project Andromeda"}),
                path_class: PathClass::Direct,
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("evaluation should pass");

        assert_eq!(
            result.executed.recognizer_families_executed,
            vec!["builtin".to_string(), "custom".to_string()]
        );
    }

    #[test]
    fn executed_recognizer_families_exclude_custom_when_not_compiled() {
        let engine = engine();
        let result = engine
            .evaluate(EvaluateRequest {
                request_id: "r-executed-no-custom".to_string(),
                profile_id: "minimal".to_string(),
                mode: EvaluationMode::Enforce,
                payload: json!({"content": "no sensitive data"}),
                path_class: PathClass::Direct,
                effective_language: "en".to_string(),
                entity_scope_filters: Vec::new(),
                recognizer_family_filters: Vec::new(),
                allowlist_additions: Vec::new(),
            })
            .expect("evaluation should pass");

        assert_eq!(result.executed.recognizer_families_executed, vec!["builtin".to_string()]);
    }
}
