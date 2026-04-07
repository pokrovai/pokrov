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
    NormalizedHit, PolicyAction, PolicyProfile, ResolvedHit, ResolvedLocationKind,
    ResolvedLocationRecord, ResolvedSpan, TransformPlan,
};

pub mod audit;
pub mod detection;
pub mod dry_run;
pub mod policy;
pub mod transform;
pub mod traversal;
pub mod types;
pub mod util;

#[cfg(feature = "ner")]
pub mod ner_adapter;

#[cfg(feature = "ner")]
use ner_adapter::{NerAdapter, NerAdapterError, NerFailMode};

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
    #[cfg(feature = "ner")]
    ner: Option<Arc<NerAdapter>>,
    #[cfg(feature = "ner")]
    ner_profile_entity_types: std::collections::HashMap<String, Vec<pokrov_ner::NerEntityType>>,
    #[cfg(feature = "ner")]
    ner_profile_fail_modes: std::collections::HashMap<String, NerFailMode>,
    #[cfg(feature = "ner")]
    skip_llm_tools_and_system_for_ner: bool,
    #[cfg(feature = "ner")]
    ner_skip_fields: Vec<regex::Regex>,
    #[cfg(feature = "ner")]
    ner_strip_values: Vec<regex::Regex>,
    #[cfg(feature = "ner")]
    ner_exclude_entity_patterns: Vec<regex::Regex>,
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

        Ok(Self {
            default_profile: config.default_profile,
            profiles: Arc::new(profiles),
            #[cfg(feature = "ner")]
            ner: None,
            #[cfg(feature = "ner")]
            ner_profile_entity_types: std::collections::HashMap::new(),
            #[cfg(feature = "ner")]
            ner_profile_fail_modes: std::collections::HashMap::new(),
            #[cfg(feature = "ner")]
            skip_llm_tools_and_system_for_ner: true,
            #[cfg(feature = "ner")]
            ner_skip_fields: Vec::new(),
            #[cfg(feature = "ner")]
            ner_strip_values: Vec::new(),
            #[cfg(feature = "ner")]
            ner_exclude_entity_patterns: Vec::new(),
        })
    }

    #[cfg(feature = "ner")]
    pub fn with_ner(mut self, adapter: Arc<NerAdapter>) -> Self {
        self.ner = Some(adapter);
        self
    }

    #[cfg(feature = "ner")]
    pub fn with_ner_profiles(
        mut self,
        profiles: std::collections::HashMap<String, Vec<pokrov_ner::NerEntityType>>,
    ) -> Self {
        self.ner_profile_entity_types = profiles;
        self
    }

    #[cfg(feature = "ner")]
    pub fn with_ner_fail_modes(
        mut self,
        fail_modes: std::collections::HashMap<String, NerFailMode>,
    ) -> Self {
        self.ner_profile_fail_modes = fail_modes;
        self
    }

    #[cfg(feature = "ner")]
    pub fn with_ner_llm_skip_filter(mut self, enabled: bool) -> Self {
        self.skip_llm_tools_and_system_for_ner = enabled;
        self
    }

    /// Appends regex patterns for JSON pointer paths that NER should skip.
    #[cfg(feature = "ner")]
    pub fn with_ner_skip_fields(mut self, patterns: Vec<regex::Regex>) -> Self {
        self.ner_skip_fields = patterns;
        self
    }

    /// Appends regex patterns for text content that NER should strip
    /// (replace with spaces) before inference. Detected spans are remapped
    /// back to the original text offsets.
    #[cfg(feature = "ner")]
    pub fn with_ner_strip_values(mut self, patterns: Vec<regex::Regex>) -> Self {
        self.ner_strip_values = patterns;
        self
    }

    /// Sets regex patterns that exclude NER hits whose recognized text
    /// matches (e.g., `^_E_` skips GraphQL entity type markers).
    #[cfg(feature = "ner")]
    pub fn with_ner_exclude_entity_patterns(mut self, patterns: Vec<regex::Regex>) -> Self {
        self.ner_exclude_entity_patterns = patterns;
        self
    }

    /// Collects NER hits from string leaves and appends them to the detection hit list.
    /// Deduplicates texts via HashSet to avoid redundant inference on identical values.
    #[cfg(feature = "ner")]
    fn apply_ner_detection(
        &self,
        payload: &serde_json::Value,
        path_class: crate::types::PathClass,
        profile_id: &str,
        ner: &Arc<NerAdapter>,
        hits: &mut Vec<crate::types::DetectionHit>,
    ) -> Result<(), NerAdapterError> {
        use std::collections::HashSet;

        let mut items: Vec<(String, String)> = Vec::new();
        traversal::visit_string_leaves(payload, &mut |pointer, text| {
            if should_skip_ner_pointer(
                self.skip_llm_tools_and_system_for_ner,
                path_class,
                payload,
                pointer,
            ) {
                return;
            }
            if !self.ner_skip_fields.is_empty() {
                let segments: Vec<&str> = pointer.split('/').collect();
                if segments.iter().any(|s| self.ner_skip_fields.iter().any(|re| re.is_match(s))) {
                    return;
                }
            }
            if text.len() >= 3 && looks_like_ner_candidate(text) {
                items.push((pointer.to_string(), text.to_string()));
            }
        });
        if items.is_empty() {
            return Ok(());
        }

        let mut seen: HashSet<&str> = HashSet::with_capacity(items.len());
        let mut unique_texts: Vec<String> = Vec::new();
        let mut text_to_indices: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();

        for (idx, (_, text)) in items.iter().enumerate() {
            if seen.insert(text.as_str()) {
                unique_texts.push(text.clone());
            }
            text_to_indices.entry(text.clone()).or_default().push(idx);
        }

        let stripped_texts: Vec<String> = if self.ner_strip_values.is_empty() {
            unique_texts.clone()
        } else {
            unique_texts
                .iter()
                .map(|text| strip_regex_regions(text, &self.ner_strip_values))
                .collect()
        };

        let Some(compiled_profile) = self.profiles.get(profile_id) else {
            return Ok(());
        };
        let category_actions = &compiled_profile.profile.category_actions;

        let stripped_refs: Vec<(String, &str)> =
            stripped_texts.iter().map(|t| (t.clone(), t.as_str())).collect();

        let batch_results =
            if let Some(profile_types) = self.ner_profile_entity_types.get(profile_id) {
                ner.recognize_batch_sync_with_types(&stripped_refs, profile_types)
            } else {
                ner.recognize_batch_sync(&stripped_refs)
            };

        let batch_results = batch_results?;

        for (unique_idx, (_, normalized)) in batch_results.iter().enumerate() {
            let unique_text = &unique_texts[unique_idx];
            if let Some(indices) = text_to_indices.get(unique_text) {
                for &item_idx in indices {
                    let pointer = &items[item_idx].0;
                    let source_text = &items[item_idx].1;
                    for nh in normalized {
                        if !self.ner_exclude_entity_patterns.is_empty() {
                            let entity_text = &source_text[nh.start..nh.end.min(source_text.len())];
                            let trimmed = entity_text.trim_matches('"');
                            if self
                                .ner_exclude_entity_patterns
                                .iter()
                                .any(|re| re.is_match(trimmed))
                            {
                                continue;
                            }
                        }
                        let Some((start, end)) =
                            normalize_ner_span_bounds(source_text, nh.start, nh.end)
                        else {
                            continue;
                        };
                        hits.push(crate::types::DetectionHit {
                            rule_id: nh.rule_id.clone(),
                            category: nh.category,
                            json_pointer: pointer.clone(),
                            start,
                            end,
                            action: category_actions.action_for(nh.category),
                            priority: nh.priority,
                            replacement_template: if nh.replacement_template_present {
                                Some(String::new())
                            } else {
                                None
                            },
                        });
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(feature = "ner")]
    fn ner_fail_mode_for_profile(&self, profile_id: &str, ner: &NerAdapter) -> NerFailMode {
        self.ner_profile_fail_modes.get(profile_id).copied().unwrap_or_else(|| ner.fail_mode())
    }

    #[cfg(feature = "ner")]
    fn ner_failure_reason_code(error: &NerAdapterError) -> &'static str {
        match error {
            NerAdapterError::Timeout(_) => "ner_inference_timeout",
            NerAdapterError::EngineFailed(_) => "ner_inference_failed",
        }
    }

    #[cfg(feature = "ner")]
    fn ner_fail_closed_reason_code(error: &NerAdapterError) -> &'static str {
        match error {
            NerAdapterError::Timeout(_) => "fail_closed:ner_inference_timeout",
            NerAdapterError::EngineFailed(_) => "fail_closed:ner_inference_failed",
        }
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
        #[allow(unused_mut)]
        let mut hits = detect_payload(
            &request.payload,
            &compiled_profile.profile,
            &compiled_profile.custom_rules,
            &request.allowlist_additions,
        );
        #[allow(unused_mut)]
        let mut degraded_reasons: Vec<String> = Vec::new();
        #[allow(unused_mut)]
        let mut fail_closed_applied = false;

        #[cfg(feature = "ner")]
        {
            if let Some(ref ner) = self.ner {
                if compiled_profile.profile.ner_enabled {
                    if let Err(error) = self.apply_ner_detection(
                        &request.payload,
                        request.path_class,
                        &profile_id,
                        ner,
                        &mut hits,
                    ) {
                        tracing::warn!(
                            error = %error,
                            profile_id = %profile_id,
                            "NER batch inference failed"
                        );
                        let reason = Self::ner_failure_reason_code(&error);
                        if !degraded_reasons.iter().any(|existing| existing == reason) {
                            degraded_reasons.push(reason.to_string());
                        }
                        if self.ner_fail_mode_for_profile(&profile_id, ner)
                            == NerFailMode::FailClosed
                            && is_execution_enabled(request.mode)
                        {
                            fail_closed_applied = true;
                            let fail_closed_reason = Self::ner_fail_closed_reason_code(&error);
                            if !degraded_reasons
                                .iter()
                                .any(|existing| existing == fail_closed_reason)
                            {
                                degraded_reasons.push(fail_closed_reason.to_string());
                            }
                        }
                    }
                }
            }
        }
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

        let mut decision = EvaluateDecision {
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
        if fail_closed_applied {
            decision.final_action = PolicyAction::Block;
            let fail_closed_winner = "winner:fail_closed.ner_inference";
            if !decision.reason_codes.iter().any(|existing| existing == fail_closed_winner) {
                decision.reason_codes.push(fail_closed_winner.to_string());
            }
        }

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
                #[cfg(feature = "ner")]
                self.ner.is_some(),
                #[cfg(not(feature = "ner"))]
                false,
            ),
            transform_applied: transform.transformed_fields_count > 0,
        };
        let degraded = DegradedSummary {
            is_degraded: !degraded_reasons.is_empty(),
            reasons: degraded_reasons,
            fail_closed_applied,
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

fn recognizer_families_executed(
    custom_rules: &[CompiledCustomRule],
    ner_enabled: bool,
) -> Vec<String> {
    let mut families = vec!["builtin".to_string()];
    if custom_rules.iter().any(|rule| rule.rule_id.starts_with("deterministic.")) {
        families.push("deterministic".to_string());
    }
    if custom_rules.iter().any(|rule| rule.rule_id.starts_with("custom.")) {
        families.push("custom".to_string());
    }
    if ner_enabled {
        families.push("ner".to_string());
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
        ("ner".to_string(), 0),
    ]);

    for hit in hits {
        let key = if hit.rule_id.starts_with("ner.") {
            "ner"
        } else if hit.rule_id.starts_with("deterministic.") {
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

#[cfg(feature = "ner")]
fn should_skip_ner_pointer(
    skip_llm_tools_and_system_for_ner: bool,
    path_class: crate::types::PathClass,
    payload: &serde_json::Value,
    pointer: &str,
) -> bool {
    if !skip_llm_tools_and_system_for_ner {
        return false;
    }

    if path_class != crate::types::PathClass::Llm {
        return false;
    }

    if pointer == "/tools" || pointer.starts_with("/tools/") {
        return true;
    }

    let Some(rest) = pointer.strip_prefix("/messages/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(idx_segment) = parts.next() else {
        return false;
    };
    let Ok(message_idx) = idx_segment.parse::<usize>() else {
        return false;
    };
    let Some(field) = parts.next() else {
        return false;
    };
    if field != "content" {
        return false;
    }

    llm_message_role(payload, message_idx).is_some_and(|role| role.eq_ignore_ascii_case("system"))
}

#[cfg(feature = "ner")]
fn llm_message_role(payload: &serde_json::Value, message_idx: usize) -> Option<&str> {
    payload
        .as_object()
        .and_then(|root| root.get("messages"))
        .and_then(serde_json::Value::as_array)
        .and_then(|messages| messages.get(message_idx))
        .and_then(serde_json::Value::as_object)
        .and_then(|message| message.get("role"))
        .and_then(serde_json::Value::as_str)
}

#[cfg(all(test, feature = "ner"))]
mod ner_filter_tests {
    use serde_json::json;

    use crate::types::PathClass;

    use super::{looks_like_ner_candidate, normalize_ner_span_bounds, should_skip_ner_pointer};

    #[test]
    fn skips_llm_tools_payload_for_ner() {
        let payload = json!({
            "messages": [{"role": "user", "content": "hello"}],
            "tools": [{"function": {"name": "x", "description": "long schema"}}]
        });
        assert!(should_skip_ner_pointer(true, PathClass::Llm, &payload, "/tools/0/function/name"));
    }

    #[test]
    fn skips_llm_system_message_content_for_ner() {
        let payload = json!({
            "messages": [
                {"role": "system", "content": "very long system prompt"},
                {"role": "user", "content": "hello"}
            ]
        });
        assert!(should_skip_ner_pointer(true, PathClass::Llm, &payload, "/messages/0/content"));
        assert!(!should_skip_ner_pointer(true, PathClass::Llm, &payload, "/messages/1/content"));
    }

    #[test]
    fn does_not_skip_when_filter_is_disabled() {
        let payload = json!({
            "messages": [{"role": "system", "content": "system prompt"}],
            "tools": [{"function": {"description": "tool schema"}}]
        });
        assert!(!should_skip_ner_pointer(false, PathClass::Llm, &payload, "/messages/0/content"));
        assert!(!should_skip_ner_pointer(false, PathClass::Llm, &payload, "/tools/0/function"));
    }

    #[test]
    fn normalizes_partial_latin_name_span_from_ner() {
        let text = "My Name mikhail Fedorov.";
        let start = text.find("Fedorov").expect("name should be present");
        let partial_end = start + "Fedoro".len();

        let normalized =
            normalize_ner_span_bounds(text, start, partial_end).expect("span should normalize");

        assert_eq!(&text[normalized.0..normalized.1], "Fedorov");
    }

    #[test]
    fn normalizes_partial_cyrillic_name_span_from_ner() {
        let text = "Иван Петров работает в Газпроме";

        let normalized = normalize_ner_span_bounds(text, 0, "И".len())
            .expect("span should normalize to whole token");

        assert_eq!(&text[normalized.0..normalized.1], "Иван");
    }

    #[test]
    fn accepts_json_key_value_fragment_containing_name() {
        assert!(looks_like_ner_candidate(
            r#"sjkhsakjhdfk "name" : "Михайлов Артём Сергеевич", "__typename" : "_E_Calendar""#
        ));
    }

    #[test]
    fn accepts_plain_text_with_name() {
        assert!(looks_like_ner_candidate("Михайлов Артём Сергеевич"));
    }
}

#[cfg(feature = "ner")]
fn looks_like_ner_candidate(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let first_char = match trimmed.chars().next() {
        Some(c) => c,
        None => return false,
    };

    if first_char.is_ascii_digit()
        && (trimmed.contains(':') || trimmed.contains('-') || trimmed.contains('/'))
    {
        return false;
    }

    let mut has_alpha = false;
    for word in trimmed.split_whitespace() {
        for c in word.chars() {
            if c.is_alphabetic() {
                has_alpha = true;
                break;
            }
        }
        if has_alpha {
            break;
        }
    }
    if !has_alpha {
        return false;
    }

    if trimmed.len() < 2 {
        return false;
    }

    true
}

/// Replaces all regex matches in `text` with spaces, preserving byte length.
/// Stripped text has identical byte offsets to the original, so NER spans
/// can be applied directly without remapping.
#[cfg(feature = "ner")]
fn strip_regex_regions(text: &str, patterns: &[regex::Regex]) -> String {
    let mut stripped: Vec<u8> = text.as_bytes().to_vec();
    for re in patterns {
        for mat in re.find_iter(text) {
            for b in &mut stripped[mat.start()..mat.end()] {
                *b = b' ';
            }
        }
    }
    // Only ASCII space bytes are written, so the result is always valid UTF-8.
    String::from_utf8(stripped).expect("replacing ASCII bytes in valid UTF-8 produces valid UTF-8")
}

#[cfg(feature = "ner")]
fn normalize_ner_span_bounds(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }

    let mut start = start.min(text.len());
    let mut end = end.min(text.len());
    if start >= end {
        return None;
    }

    // NER offsets must be normalized to UTF-8 boundaries before slicing.
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    if start >= end {
        return None;
    }

    // Some NER models return sub-token pieces for names in mixed-language text.
    // Expand to surrounding token characters to avoid partial-redaction leaks.
    while start > 0 {
        let Some((prev_start, prev_char)) = text[..start].char_indices().next_back() else {
            break;
        };
        if is_sensitive_token_char(prev_char) {
            start = prev_start;
        } else {
            break;
        }
    }

    while end < text.len() {
        let Some(next_char) = text[end..].chars().next() else {
            break;
        };
        if is_sensitive_token_char(next_char) {
            end += next_char.len_utf8();
        } else {
            break;
        }
    }

    (start < end).then_some((start, end))
}

#[cfg(feature = "ner")]
fn is_sensitive_token_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '\'')
}

#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
