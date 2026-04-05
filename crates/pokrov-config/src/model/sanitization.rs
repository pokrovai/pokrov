use std::collections::BTreeMap;

use pokrov_core::types::{
    CategoryActions, CustomRule, DeterministicContextPolicy, DeterministicNormalizationMode as CoreDeterministicNormalizationMode, DeterministicRuleKind, DeterministicRuleMetadata, DeterministicValidatorKind as CoreDeterministicValidatorKind, EvaluationMode, EvaluatorConfig, PolicyAction, PolicyProfile,
};
use serde::{Deserialize, Serialize};

use super::RuntimeConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_profile_id")]
    pub default_profile: String,
    #[serde(default)]
    pub profiles: SanitizationProfiles,
}

impl Default for SanitizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_profile: default_profile_id(),
            profiles: SanitizationProfiles::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizationProfiles {
    #[serde(default = "default_minimal_profile")]
    pub minimal: SanitizationProfile,
    #[serde(default = "default_strict_profile")]
    pub strict: SanitizationProfile,
    #[serde(default = "default_custom_profile")]
    pub custom: SanitizationProfile,
}

impl Default for SanitizationProfiles {
    fn default() -> Self {
        Self {
            minimal: default_minimal_profile(),
            strict: default_strict_profile(),
            custom: default_custom_profile(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SanitizationProfile {
    #[serde(default = "default_mode")]
    pub mode_default: EvaluationMode,
    pub categories: CategoryActionsConfig,
    #[serde(default = "default_mask_visible_suffix")]
    pub mask_visible_suffix: u8,
    #[serde(default)]
    pub custom_rules: Vec<CustomRuleConfig>,
    #[serde(default)]
    pub deterministic_recognizers: Vec<DeterministicRecognizerConfig>,
    #[serde(default)]
    pub allow_empty_matches: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeterministicRecognizerConfig {
    pub id: String,
    pub category: pokrov_core::types::DetectionCategory,
    pub action: PolicyAction,
    #[serde(default = "default_rule_priority")]
    pub family_priority: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub patterns: Vec<DeterministicPatternConfig>,
    #[serde(default)]
    pub denylist_exact: Vec<String>,
    #[serde(default)]
    pub allowlist_exact: Vec<String>,
    #[serde(default)]
    pub context: Option<DeterministicContextConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeterministicPatternConfig {
    pub id: String,
    pub expression: String,
    #[serde(default = "default_pattern_score")]
    pub base_score: u16,
    #[serde(default)]
    pub validator: Option<DeterministicValidatorConfig>,
    #[serde(default)]
    pub normalization: DeterministicNormalizationMode,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeterministicValidatorConfig {
    pub kind: DeterministicValidatorKind,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeterministicValidatorKind {
    Luhn,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeterministicNormalizationMode {
    #[default]
    Preserve,
    Lowercase,
    AlnumLowercase,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeterministicContextConfig {
    #[serde(default)]
    pub positive_terms: Vec<String>,
    #[serde(default)]
    pub negative_terms: Vec<String>,
    #[serde(default = "default_context_window")]
    pub window: u8,
    #[serde(default)]
    pub suppress_on_negative: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CategoryActionsConfig {
    pub secrets: PolicyAction,
    pub pii: PolicyAction,
    pub corporate_markers: PolicyAction,
    #[serde(default)]
    pub custom: Option<PolicyAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomRuleConfig {
    pub id: String,
    pub category: pokrov_core::types::DetectionCategory,
    pub pattern: String,
    pub action: PolicyAction,
    #[serde(default = "default_rule_priority")]
    pub priority: u16,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl RuntimeConfig {
    pub fn evaluator_config(&self) -> EvaluatorConfig {
        EvaluatorConfig {
            default_profile: self.sanitization.default_profile.clone(),
            profiles: BTreeMap::from([
                (
                    "minimal".to_string(),
                    to_policy_profile("minimal", &self.sanitization.profiles.minimal),
                ),
                (
                    "strict".to_string(),
                    to_policy_profile("strict", &self.sanitization.profiles.strict),
                ),
                (
                    "custom".to_string(),
                    to_policy_profile("custom", &self.sanitization.profiles.custom),
                ),
            ]),
        }
    }
}

fn to_policy_profile(profile_id: &str, profile: &SanitizationProfile) -> PolicyProfile {
    let mut custom_rules = profile
        .custom_rules
        .iter()
        .map(|rule| CustomRule {
            rule_id: rule.id.clone(),
            category: rule.category,
            pattern: rule.pattern.clone(),
            action: rule.action,
            priority: rule.priority,
            replacement_template: rule.replacement.clone(),
            enabled: rule.enabled,
            deterministic: None,
        })
        .collect::<Vec<_>>();
    custom_rules.extend(deterministic_rules(profile));

    PolicyProfile {
        profile_id: profile_id.to_string(),
        mode_default: profile.mode_default,
        category_actions: CategoryActions {
            secrets: profile.categories.secrets,
            pii: profile.categories.pii,
            corporate_markers: profile.categories.corporate_markers,
            custom: profile.categories.custom.unwrap_or(profile.categories.corporate_markers),
        },
        mask_visible_suffix: profile.mask_visible_suffix,
        custom_rules_enabled: true,
        custom_rules,
    }
}

fn deterministic_rules(profile: &SanitizationProfile) -> Vec<CustomRule> {
    let mut rules = Vec::new();

    for recognizer in
        profile.deterministic_recognizers.iter().filter(|recognizer| recognizer.enabled)
    {
        for pattern in &recognizer.patterns {
            let validator_key = pattern
                .validator
                .as_ref()
                .map(|validator| match validator.kind {
                    DeterministicValidatorKind::Luhn => "luhn",
                })
                .unwrap_or("none");
            let normalization_key = match pattern.normalization {
                DeterministicNormalizationMode::Preserve => "preserve",
                DeterministicNormalizationMode::Lowercase => "lowercase",
                DeterministicNormalizationMode::AlnumLowercase => "alnum_lowercase",
            };
            rules.push(CustomRule {
                rule_id: format!(
                    "deterministic.{}.pattern.{}.validator.{}.norm.{}",
                    recognizer.id, pattern.id, validator_key, normalization_key
                ),
                category: recognizer.category,
                pattern: pattern.expression.clone(),
                action: recognizer.action,
                priority: recognizer.family_priority.saturating_add(pattern.base_score),
                replacement_template: None,
                enabled: recognizer.enabled,
                deterministic: Some(DeterministicRuleMetadata {
                    recognizer_id: recognizer.id.clone(),
                    rule: DeterministicRuleKind::Pattern {
                        validator: pattern
                            .validator
                            .as_ref()
                            .map(|validator| match validator.kind {
                                DeterministicValidatorKind::Luhn => {
                                    CoreDeterministicValidatorKind::Luhn
                                }
                            })
                            .unwrap_or(CoreDeterministicValidatorKind::None),
                        normalization: match pattern.normalization {
                            DeterministicNormalizationMode::Preserve => {
                                CoreDeterministicNormalizationMode::Preserve
                            }
                            DeterministicNormalizationMode::Lowercase => {
                                CoreDeterministicNormalizationMode::Lowercase
                            }
                            DeterministicNormalizationMode::AlnumLowercase => {
                                CoreDeterministicNormalizationMode::AlnumLowercase
                            }
                        },
                        context: recognizer.context.as_ref().map(|context| {
                            DeterministicContextPolicy {
                                positive_terms: context.positive_terms.clone(),
                                negative_terms: context.negative_terms.clone(),
                                window: context.window,
                                suppress_on_negative: context.suppress_on_negative,
                            }
                        }),
                    },
                }),
            });
        }

        for (index, value) in recognizer.denylist_exact.iter().enumerate() {
            let escaped = regex::escape(value);
            rules.push(CustomRule {
                rule_id: format!("deterministic.{}.denylist.{index}", recognizer.id),
                category: recognizer.category,
                pattern: format!(r"\A{escaped}\z"),
                action: recognizer.action,
                priority: recognizer.family_priority.saturating_add(1000),
                replacement_template: None,
                enabled: recognizer.enabled,
                deterministic: Some(DeterministicRuleMetadata {
                    recognizer_id: recognizer.id.clone(),
                    rule: DeterministicRuleKind::DenylistExact,
                }),
            });
        }
    }

    rules
}

fn default_true() -> bool {
    true
}

fn default_profile_id() -> String {
    "strict".to_string()
}

fn default_mode() -> EvaluationMode {
    EvaluationMode::Enforce
}

fn default_mask_visible_suffix() -> u8 {
    4
}

fn default_rule_priority() -> u16 {
    100
}

fn default_pattern_score() -> u16 {
    100
}

fn default_context_window() -> u8 {
    32
}

fn default_minimal_profile() -> SanitizationProfile {
    SanitizationProfile {
        mode_default: EvaluationMode::Enforce,
        categories: CategoryActionsConfig {
            secrets: PolicyAction::Mask,
            pii: PolicyAction::Allow,
            corporate_markers: PolicyAction::Allow,
            custom: None,
        },
        mask_visible_suffix: 4,
        custom_rules: Vec::new(),
        deterministic_recognizers: Vec::new(),
        allow_empty_matches: false,
    }
}

fn default_strict_profile() -> SanitizationProfile {
    SanitizationProfile {
        mode_default: EvaluationMode::Enforce,
        categories: CategoryActionsConfig {
            secrets: PolicyAction::Block,
            pii: PolicyAction::Redact,
            corporate_markers: PolicyAction::Mask,
            custom: None,
        },
        mask_visible_suffix: 4,
        custom_rules: Vec::new(),
        deterministic_recognizers: Vec::new(),
        allow_empty_matches: false,
    }
}

fn default_custom_profile() -> SanitizationProfile {
    SanitizationProfile {
        mode_default: EvaluationMode::DryRun,
        categories: CategoryActionsConfig {
            secrets: PolicyAction::Redact,
            pii: PolicyAction::Mask,
            corporate_markers: PolicyAction::Mask,
            custom: None,
        },
        mask_visible_suffix: 4,
        custom_rules: Vec::new(),
        deterministic_recognizers: Vec::new(),
        allow_empty_matches: false,
    }
}

#[cfg(test)]
mod tests {
    use pokrov_core::types::{
        DeterministicNormalizationMode as CoreDeterministicNormalizationMode,
        DeterministicRuleKind, DeterministicValidatorKind as CoreDeterministicValidatorKind,
        PolicyAction,
    };

    use super::{
        to_policy_profile, CategoryActionsConfig, DeterministicContextConfig,
        DeterministicNormalizationMode, DeterministicPatternConfig, DeterministicRecognizerConfig,
        DeterministicValidatorConfig, DeterministicValidatorKind, RuntimeConfig,
        SanitizationProfile,
    };

    #[test]
    fn evaluator_config_uses_explicit_custom_action_when_present_in_yaml() {
        let raw = r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 1000
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      mode_default: enforce
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
      mask_visible_suffix: 4
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
        custom: block
      mask_visible_suffix: 4
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
"#;

        let config: RuntimeConfig =
            serde_yaml::from_str(raw).expect("runtime config with custom category must parse");
        let evaluator = config.evaluator_config();

        let strict = evaluator
            .profiles
            .get("strict")
            .expect("strict profile must exist in evaluator config");
        assert_eq!(strict.category_actions.custom, PolicyAction::Block);
    }

    #[test]
    fn evaluator_config_falls_back_to_corporate_markers_for_custom_action_when_omitted() {
        let raw = r#"
server:
  host: 127.0.0.1
  port: 8080
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 1000
  grace_period_ms: 1000
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
sanitization:
  enabled: true
  default_profile: strict
  profiles:
    minimal:
      mode_default: enforce
      categories:
        secrets: mask
        pii: allow
        corporate_markers: allow
      mask_visible_suffix: 4
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4
    custom:
      mode_default: dry_run
      categories:
        secrets: redact
        pii: mask
        corporate_markers: mask
      mask_visible_suffix: 4
"#;

        let config: RuntimeConfig = serde_yaml::from_str(raw)
            .expect("runtime config without explicit custom category must parse");
        let evaluator = config.evaluator_config();

        let strict = evaluator
            .profiles
            .get("strict")
            .expect("strict profile must exist in evaluator config");
        assert_eq!(strict.category_actions.custom, strict.category_actions.corporate_markers);
    }

    #[test]
    fn deterministic_denylist_exact_rules_are_anchored_to_full_value() {
        let profile = SanitizationProfile {
            mode_default: pokrov_core::types::EvaluationMode::Enforce,
            categories: CategoryActionsConfig {
                secrets: PolicyAction::Block,
                pii: PolicyAction::Redact,
                corporate_markers: PolicyAction::Mask,
                custom: None,
            },
            mask_visible_suffix: 4,
            custom_rules: Vec::new(),
            deterministic_recognizers: vec![DeterministicRecognizerConfig {
                id: "payment_card".to_string(),
                category: pokrov_core::types::DetectionCategory::Secrets,
                action: PolicyAction::Block,
                family_priority: 500,
                enabled: true,
                patterns: Vec::new(),
                denylist_exact: vec!["4111 1111 1111 1111".to_string()],
                allowlist_exact: Vec::new(),
                context: None,
            }],
            allow_empty_matches: false,
        };

        let policy = to_policy_profile("strict", &profile);
        let denylist_rule = policy
            .custom_rules
            .iter()
            .find(|rule| rule.rule_id == "deterministic.payment_card.denylist.0")
            .expect("denylist rule must be materialized");
        assert_eq!(denylist_rule.pattern, r"\A4111 1111 1111 1111\z");
        assert!(matches!(
            denylist_rule.deterministic.as_ref().map(|meta| &meta.rule),
            Some(DeterministicRuleKind::DenylistExact)
        ));
    }

    #[test]
    fn deterministic_pattern_rule_preserves_validator_and_context_metadata() {
        let profile = SanitizationProfile {
            mode_default: pokrov_core::types::EvaluationMode::Enforce,
            categories: CategoryActionsConfig {
                secrets: PolicyAction::Block,
                pii: PolicyAction::Redact,
                corporate_markers: PolicyAction::Mask,
                custom: None,
            },
            mask_visible_suffix: 4,
            custom_rules: Vec::new(),
            deterministic_recognizers: vec![DeterministicRecognizerConfig {
                id: "payment_card".to_string(),
                category: pokrov_core::types::DetectionCategory::Secrets,
                action: PolicyAction::Block,
                family_priority: 600,
                enabled: true,
                patterns: vec![DeterministicPatternConfig {
                    id: "pan".to_string(),
                    expression: "\\b(?:\\d[ -]*?){13,16}\\b".to_string(),
                    base_score: 150,
                    validator: Some(DeterministicValidatorConfig {
                        kind: DeterministicValidatorKind::Luhn,
                    }),
                    normalization: DeterministicNormalizationMode::AlnumLowercase,
                }],
                denylist_exact: Vec::new(),
                allowlist_exact: Vec::new(),
                context: Some(DeterministicContextConfig {
                    positive_terms: vec!["card".to_string()],
                    negative_terms: vec!["demo".to_string()],
                    window: 16,
                    suppress_on_negative: true,
                }),
            }],
            allow_empty_matches: false,
        };

        let policy = to_policy_profile("strict", &profile);
        let pattern_rule = policy
            .custom_rules
            .iter()
            .find(|rule| rule.rule_id.starts_with("deterministic.payment_card.pattern.pan"))
            .expect("pattern rule must be materialized");

        match pattern_rule
            .deterministic
            .as_ref()
            .map(|metadata| &metadata.rule)
            .expect("deterministic metadata must exist")
        {
            DeterministicRuleKind::Pattern { validator, normalization, context } => {
                assert_eq!(*validator, CoreDeterministicValidatorKind::Luhn);
                assert_eq!(*normalization, CoreDeterministicNormalizationMode::AlnumLowercase);
                let context = context.as_ref().expect("context must be preserved");
                assert_eq!(context.window, 16);
                assert!(context.suppress_on_negative);
            }
            DeterministicRuleKind::DenylistExact => {
                panic!("pattern rule must not be encoded as denylist rule");
            }
        }
    }
}
