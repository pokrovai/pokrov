/// Profile-scoped context policy applied after deterministic candidate matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPolicy {
    pub positive_terms: Vec<String>,
    pub negative_terms: Vec<String>,
    pub score_boost: i16,
    pub score_penalty: i16,
    pub suppress_on_negative: bool,
    pub require_positive_match: bool,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            positive_terms: default_positive_terms(),
            negative_terms: default_negative_terms(),
            score_boost: 10,
            score_penalty: 10,
            suppress_on_negative: false,
            require_positive_match: false,
        }
    }
}

/// Applies lexical context scoring and suppression metadata.
pub fn apply_context_policy(
    text: &str,
    base_score: i16,
    context: &ContextPolicy,
) -> (i16, bool, Vec<String>) {
    let lower = text.to_lowercase();
    let mut score = base_score;
    let mut reason_codes = Vec::new();

    let positive_present = context.positive_terms.iter().any(|term| lower.contains(term));
    if positive_present {
        score += context.score_boost;
        reason_codes.push("context_positive_boost".to_string());
    }
    if context.require_positive_match && !positive_present {
        reason_codes.push("context_positive_required_missing".to_string());
        return (score, true, reason_codes);
    }

    let negative_present = context.negative_terms.iter().any(|term| lower.contains(term));
    if negative_present {
        score -= context.score_penalty;
        reason_codes.push("context_negative_penalty".to_string());
    }

    let suppressed = negative_present && context.suppress_on_negative;
    if suppressed {
        reason_codes.push("context_negative_suppressed".to_string());
    }

    (score, suppressed, reason_codes)
}

fn default_positive_terms() -> Vec<String> {
    vec!["token".to_string(), "secret".to_string()]
}

fn default_negative_terms() -> Vec<String> {
    vec!["example".to_string(), "demo".to_string()]
}

#[cfg(test)]
mod tests {
    use super::{apply_context_policy, ContextPolicy};

    #[test]
    fn default_negative_context_downscores_without_suppression() {
        let policy = ContextPolicy::default();
        let (score, suppressed, reasons) = apply_context_policy("demo token", 50, &policy);
        assert_eq!(score, 50);
        assert!(!suppressed);
        assert!(reasons.contains(&"context_positive_boost".to_string()));
        assert!(reasons.contains(&"context_negative_penalty".to_string()));
    }

    #[test]
    fn explicit_negative_suppression_is_opt_in() {
        let policy = ContextPolicy { suppress_on_negative: true, ..ContextPolicy::default() };
        let (_, suppressed, reasons) = apply_context_policy("demo value", 40, &policy);
        assert!(suppressed);
        assert!(reasons.contains(&"context_negative_suppressed".to_string()));
    }

    #[test]
    fn require_positive_match_suppresses_when_context_missing() {
        let policy = ContextPolicy {
            positive_terms: vec!["phone".to_string()],
            require_positive_match: true,
            ..ContextPolicy::default()
        };
        let (_, suppressed, reasons) = apply_context_policy("+79001234567", 40, &policy);
        assert!(suppressed);
        assert!(reasons.contains(&"context_positive_required_missing".to_string()));
    }
}
