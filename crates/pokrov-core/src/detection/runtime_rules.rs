use std::{collections::BTreeSet, sync::OnceLock};

use regex::{Regex, RegexBuilder};
use serde_json::Value;

use crate::{
    detection::deterministic::context::{apply_context_policy, ContextPolicy},
    detection::deterministic::lists::{
        build_allowlist_set, is_allowlisted_exact, normalize_exact_value,
    },
    detection::deterministic::pattern::compile_pattern,
    detection::deterministic::validation::{validate_candidate, ValidatorKind},
    traversal::visit_string_leaves,
    types::{
        DetectionCategory, DetectionHit, DeterministicNormalizationMode, DeterministicRuleKind,
        DeterministicRuleMetadata, DeterministicValidatorKind, EvaluateError, PolicyAction,
        PolicyProfile,
    },
};

#[derive(Debug, Clone)]
pub struct CompiledCustomRule {
    pub rule_id: String,
    pub category: DetectionCategory,
    pub action: PolicyAction,
    pub priority: u16,
    pub replacement_template: Option<String>,
    pub matcher: Regex,
    pub deterministic: Option<DeterministicRuleMetadata>,
    pub deterministic_context: Option<CompiledContextPolicy>,
    pub deterministic_allowlist: Option<BTreeSet<String>>,
}

#[derive(Debug)]
struct BuiltinRule {
    rule_id: &'static str,
    category: DetectionCategory,
    priority: u16,
    matcher: Regex,
    validator: DeterministicValidatorKind,
    normalization: DeterministicNormalizationMode,
    field_gate: Option<CompiledFieldGate>,
}

#[derive(Debug, Clone)]
pub struct CompiledContextPolicy {
    pub policy: ContextPolicy,
    pub window: u8,
}

#[derive(Debug, Clone)]
struct CompiledFieldGate {
    json_pointer_suffixes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinFieldGateSpec {
    json_pointer_suffixes: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
struct BuiltinRuleSpec {
    rule_id: &'static str,
    category: DetectionCategory,
    priority: u16,
    pattern: &'static str,
    validator: DeterministicValidatorKind,
    normalization: DeterministicNormalizationMode,
    field_gate: Option<BuiltinFieldGateSpec>,
}

#[derive(Debug, Clone, Copy)]
struct RuleMatchContext<'a> {
    rule_id: &'a str,
    category: DetectionCategory,
    action: PolicyAction,
    priority: u16,
    replacement_template: Option<&'a String>,
    matcher: &'a Regex,
    validator: DeterministicValidatorKind,
    normalization: DeterministicNormalizationMode,
    deterministic_context: Option<&'a CompiledContextPolicy>,
    deterministic_allowlist: Option<&'a BTreeSet<String>>,
    field_gate: Option<&'a CompiledFieldGate>,
}

const BUILTIN_RULES: [BuiltinRuleSpec; 9] = [
    BuiltinRuleSpec {
        rule_id: "builtin.secrets.bearer_token",
        category: DetectionCategory::Secrets,
        priority: 470,
        pattern: r"(?i)\bbearer\s+(?:gh[pousr]_[a-z0-9]{20,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+|[A-Za-z0-9._\-]{24,})\b",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.secrets.secret_assignment",
        category: DetectionCategory::Secrets,
        priority: 468,
        pattern: r#"(?i)\b(?:token|secret|api[_-]?key|access[_-]?token|auth[_-]?token)\s*[:=]\s*['"]?(?:gh[pousr]_[a-z0-9]{20,}|sk[-_][a-z0-9][a-z0-9_-]{8,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+|[A-Za-z0-9._\-]{24,})['"]?"#,
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.secrets.sk_api_key",
        category: DetectionCategory::Secrets,
        priority: 465,
        pattern: r"(?i)\bsk[-_][a-z0-9][a-z0-9_-]{8,}\b",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.secrets.github_pat",
        category: DetectionCategory::Secrets,
        priority: 466,
        pattern: r"(?i)\bgh[pousr]_[a-z0-9]{20,}\b",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.pii.email",
        category: DetectionCategory::Pii,
        priority: 320,
        pattern: r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.pii.url",
        category: DetectionCategory::Pii,
        priority: 319,
        pattern: r#"(?i)\b(?:https?|ftp)://[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+(?::\d{2,5})?(?:/[a-z0-9._~:/?#\[\]@!$&'()*+,;=%-]*[a-z0-9/#])?"#,
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.pii.ipv4",
        category: DetectionCategory::Pii,
        priority: 318,
        pattern: r"\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\b",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.pii.card_number",
        category: DetectionCategory::Pii,
        priority: 310,
        pattern: r"\b\d(?:[ -]?\d){12,15}\b",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
    BuiltinRuleSpec {
        rule_id: "builtin.corporate.project_name",
        category: DetectionCategory::CorporateMarkers,
        priority: 220,
        pattern: r"(?i)\bproject\s+[a-z][a-z0-9_-]{2,}\b",
        validator: DeterministicValidatorKind::None,
        normalization: DeterministicNormalizationMode::Preserve,
        field_gate: None,
    },
];
const REGEX_SIZE_LIMIT_BYTES: usize = 1024 * 1024;

pub fn compile_custom_rules(
    profile: &PolicyProfile,
) -> Result<Vec<CompiledCustomRule>, EvaluateError> {
    if !profile.custom_rules_enabled {
        return Ok(Vec::new());
    }

    profile
        .custom_rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| {
            let matcher = compile_pattern(&rule.pattern).map_err(|error| {
                EvaluateError::InvalidProfile(format!(
                    "profile '{}' custom rule '{}' regex is invalid: {error}",
                    profile.profile_id, rule.rule_id
                ))
            })?;

            Ok(CompiledCustomRule {
                rule_id: rule.rule_id.clone(),
                category: rule.category,
                action: rule.action,
                priority: rule.priority,
                replacement_template: rule.replacement_template.clone(),
                matcher,
                deterministic: rule.deterministic.clone(),
                deterministic_context: compile_context_policy(rule.deterministic.as_ref()),
                deterministic_allowlist: compile_deterministic_allowlist(
                    profile,
                    &rule.rule_id,
                    rule.deterministic.as_ref(),
                ),
            })
        })
        .collect()
}

pub fn detect_payload(
    payload: &Value,
    profile: &PolicyProfile,
    custom_rules: &[CompiledCustomRule],
    allowlist_additions: &[String],
) -> Vec<DetectionHit> {
    let builtin_rules = builtin_rules();
    let allowlist = build_allowlist_set(allowlist_additions);
    let max_hits = usize::try_from(profile.max_hits_per_request).unwrap_or(usize::MAX);
    let mut hit_limit_reached = false;
    let mut hits = Vec::new();

    visit_string_leaves(payload, &mut |json_pointer, text| {
        if hit_limit_reached {
            return;
        }

        for rule in builtin_rules {
            if hit_limit_reached {
                break;
            }
            emit_rule_matches(
                json_pointer,
                text,
                builtin_rule_context(rule, profile),
                &allowlist,
                max_hits,
                &mut hit_limit_reached,
                &mut hits,
            );
        }

        for rule in custom_rules {
            if hit_limit_reached {
                break;
            }
            emit_rule_matches(
                json_pointer,
                text,
                custom_rule_context(rule),
                &allowlist,
                max_hits,
                &mut hit_limit_reached,
                &mut hits,
            );
        }
    });

    if hit_limit_reached {
        tracing::warn!(
            profile_id = %profile.profile_id,
            max_hits_per_request = profile.max_hits_per_request,
            "detection hit limit reached; remaining payload leaves were skipped"
        );
    }

    hits
}

fn emit_rule_matches(
    json_pointer: &str,
    text: &str,
    rule: RuleMatchContext<'_>,
    allowlist: &BTreeSet<String>,
    max_hits: usize,
    hit_limit_reached: &mut bool,
    hits: &mut Vec<DetectionHit>,
) {
    if rule
        .field_gate
        .is_some_and(|field_gate| !field_gate.matches_json_pointer(json_pointer))
    {
        return;
    }

    for matched in rule.matcher.find_iter(text) {
        if is_allowlisted_exact(allowlist, matched.as_str()) {
            continue;
        }

        let candidate = needs_normalized_candidate(rule).then(|| {
            normalize_candidate(matched.as_str(), rule.normalization)
        });
        if !validate_for_kind(rule.validator, candidate.as_deref()) {
            continue;
        }
        if let (Some(allowlist), Some(candidate)) = (rule.deterministic_allowlist, candidate.as_ref()) {
            if allowlist.contains(candidate) {
                continue;
            }
        }

        let mut priority = rule.priority;
        if let Some(context) = rule.deterministic_context {
            let window = extract_context_window(text, matched.start(), matched.end(), context.window);
            let (score, suppressed, _) = apply_context_policy(
                &window,
                i16::try_from(rule.priority).unwrap_or(i16::MAX),
                &context.policy,
            );
            if suppressed {
                continue;
            }
            priority = score.max(0) as u16;
        }

        if hits.len() >= max_hits {
            *hit_limit_reached = true;
            break;
        }

        hits.push(DetectionHit {
            rule_id: rule.rule_id.to_string(),
            category: rule.category,
            json_pointer: json_pointer.to_string(),
            start: matched.start(),
            end: matched.end(),
            action: rule.action,
            priority,
            replacement_template: rule.replacement_template.cloned(),
        });
    }
}

fn builtin_rule_context<'a>(
    rule: &'a BuiltinRule,
    profile: &PolicyProfile,
) -> RuleMatchContext<'a> {
    RuleMatchContext {
        rule_id: rule.rule_id,
        category: rule.category,
        action: profile.category_actions.action_for(rule.category),
        priority: rule.priority,
        replacement_template: None,
        matcher: &rule.matcher,
        validator: rule.validator,
        normalization: rule.normalization,
        deterministic_context: None,
        deterministic_allowlist: None,
        field_gate: rule.field_gate.as_ref(),
    }
}

fn custom_rule_context<'a>(rule: &'a CompiledCustomRule) -> RuleMatchContext<'a> {
    let (validator, normalization) = match rule.deterministic.as_ref().map(|meta| &meta.rule) {
        Some(DeterministicRuleKind::Pattern { validator, normalization, .. }) => {
            (*validator, *normalization)
        }
        _ => (
            DeterministicValidatorKind::None,
            DeterministicNormalizationMode::Preserve,
        ),
    };

    RuleMatchContext {
        rule_id: &rule.rule_id,
        category: rule.category,
        action: rule.action,
        priority: rule.priority,
        replacement_template: rule.replacement_template.as_ref(),
        matcher: &rule.matcher,
        validator,
        normalization,
        deterministic_context: rule.deterministic_context.as_ref(),
        deterministic_allowlist: rule.deterministic_allowlist.as_ref(),
        field_gate: None,
    }
}

fn needs_normalized_candidate(rule: RuleMatchContext<'_>) -> bool {
    rule.validator != DeterministicValidatorKind::None || rule.deterministic_allowlist.is_some()
}

fn validate_for_kind(kind: DeterministicValidatorKind, candidate: Option<&str>) -> bool {
    match kind {
        DeterministicValidatorKind::None => true,
        DeterministicValidatorKind::Luhn => {
            candidate.map(|value| validate_candidate(ValidatorKind::Luhn, value)).unwrap_or(false)
        }
    }
}

fn normalize_candidate(candidate: &str, mode: DeterministicNormalizationMode) -> String {
    match mode {
        DeterministicNormalizationMode::Preserve => candidate.to_string(),
        DeterministicNormalizationMode::Lowercase => candidate.to_lowercase(),
        DeterministicNormalizationMode::AlnumLowercase => candidate
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect(),
    }
}

fn compile_context_policy(
    metadata: Option<&DeterministicRuleMetadata>,
) -> Option<CompiledContextPolicy> {
    match metadata.map(|meta| &meta.rule) {
        Some(DeterministicRuleKind::Pattern { context: Some(context), .. }) => {
            Some(CompiledContextPolicy {
                policy: to_context_policy(context),
                window: context.window,
            })
        }
        _ => None,
    }
}

fn compile_deterministic_allowlist(
    profile: &PolicyProfile,
    rule_id: &str,
    metadata: Option<&DeterministicRuleMetadata>,
) -> Option<BTreeSet<String>> {
    let metadata = metadata?;
    if metadata.allowlist_exact.is_empty() {
        return None;
    }

    let mut dropped_entries = 0usize;
    let mut set = BTreeSet::new();
    match &metadata.rule {
        DeterministicRuleKind::Pattern { normalization, .. } => {
            for value in &metadata.allowlist_exact {
                let normalized = normalize_candidate(value, *normalization);
                if normalized.is_empty() {
                    dropped_entries += 1;
                } else {
                    set.insert(normalized);
                }
            }
        }
        DeterministicRuleKind::DenylistExact => {
            for value in &metadata.allowlist_exact {
                let normalized = normalize_exact_value(value);
                if normalized.is_empty() {
                    dropped_entries += 1;
                } else {
                    set.insert(normalized);
                }
            }
        }
    }

    if dropped_entries > 0 {
        tracing::warn!(
            profile_id = %profile.profile_id,
            rule_id = %rule_id,
            dropped_entries,
            "deterministic allowlist contains empty or normalization-empty entries"
        );
    }

    (!set.is_empty()).then_some(set)
}

fn to_context_policy(context: &crate::types::DeterministicContextPolicy) -> ContextPolicy {
    ContextPolicy {
        positive_terms: context.positive_terms.iter().map(|term| term.to_lowercase()).collect(),
        negative_terms: context.negative_terms.iter().map(|term| term.to_lowercase()).collect(),
        score_boost: context.score_boost,
        score_penalty: context.score_penalty,
        suppress_on_negative: context.suppress_on_negative,
    }
}

// Regex match offsets are UTF-8 boundary-safe, so slicing by start/end is valid here.
fn extract_context_window(text: &str, start: usize, end: usize, window: u8) -> String {
    let before = trim_to_chars_start(&text[..start], usize::from(window));
    let after = trim_to_chars_end(&text[end..], usize::from(window));
    [before, text[start..end].to_string(), after].concat()
}

fn trim_to_chars_start(text: &str, max_chars: usize) -> String {
    if text.chars().nth(max_chars).is_none() {
        return text.to_string();
    }
    let cut_at = text
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    text[cut_at..].to_string()
}

fn trim_to_chars_end(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn builtin_rules() -> &'static [BuiltinRule] {
    static BUILTIN: OnceLock<Vec<BuiltinRule>> = OnceLock::new();
    BUILTIN
        .get_or_init(|| {
            BUILTIN_RULES
                .iter()
                .map(|spec| BuiltinRule {
                    rule_id: spec.rule_id,
                    category: spec.category,
                    priority: spec.priority,
                    matcher: RegexBuilder::new(spec.pattern)
                        .size_limit(REGEX_SIZE_LIMIT_BYTES)
                        .build()
                        .expect("built-in regex patterns must compile"),
                    validator: spec.validator,
                    normalization: spec.normalization,
                    field_gate: spec.field_gate.map(compile_field_gate),
                })
                .collect()
        })
        .as_slice()
}

fn compile_field_gate(spec: BuiltinFieldGateSpec) -> CompiledFieldGate {
    CompiledFieldGate {
        json_pointer_suffixes: spec
            .json_pointer_suffixes
            .iter()
            .map(|suffix| suffix.to_string())
            .collect(),
    }
}

impl CompiledFieldGate {
    fn matches_json_pointer(&self, json_pointer: &str) -> bool {
        self.json_pointer_suffixes
            .iter()
            .any(|suffix| json_pointer.ends_with(suffix))
    }
}

#[cfg(test)]
#[path = "runtime_rules_tests.rs"]
mod tests;
