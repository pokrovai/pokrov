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
    EvaluateDecision, EvaluateError, EvaluateRequest, EvaluateResult, EvaluatorConfig, PolicyProfile,
    ResolvedSpanView,
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
pub struct SanitizationEngine {
    default_profile: String,
    profiles: Arc<BTreeMap<String, CompiledProfile>>,
}

impl SanitizationEngine {
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

    pub fn evaluate(&self, request: EvaluateRequest) -> Result<EvaluateResult, EvaluateError> {
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
        let audit = build_audit_summary(&request, &profile_id, &decision, started.elapsed());

        Ok(EvaluateResult {
            request_id: request.request_id,
            profile_id,
            mode: request.mode,
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
}
