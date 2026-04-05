use regex::Regex;

/// Deterministic pattern candidate used for stable precedence ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternCandidate {
    pub recognizer_id: String,
    pub pattern_id: String,
    pub start: usize,
    pub end: usize,
    pub score: i16,
    pub family_priority: u16,
}

/// Candidate normalization modes for deterministic exact matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationMode {
    Preserve,
    Lowercase,
    AlnumLowercase,
}

/// Compiles deterministic regex patterns at startup.
pub fn compile_pattern(expression: &str) -> Result<Regex, regex::Error> {
    Regex::new(expression)
}

/// Normalizes extracted values without changing span semantics.
pub fn normalize_value(raw: &str, mode: NormalizationMode) -> String {
    match mode {
        NormalizationMode::Preserve => raw.to_string(),
        NormalizationMode::Lowercase => raw.to_lowercase(),
        NormalizationMode::AlnumLowercase => {
            raw.chars().filter(|ch| ch.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
        }
    }
}

/// Produces deterministic ordering for same-span candidates.
pub fn stable_sort_candidates(candidates: &mut [PatternCandidate]) {
    candidates.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| right.end.cmp(&left.end))
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| right.family_priority.cmp(&left.family_priority))
            .then_with(|| left.recognizer_id.cmp(&right.recognizer_id))
            .then_with(|| left.pattern_id.cmp(&right.pattern_id))
    });
}

#[cfg(test)]
mod tests {
    use super::{
        compile_pattern, normalize_value, stable_sort_candidates, NormalizationMode,
        PatternCandidate,
    };

    #[test]
    fn compiles_patterns_and_normalizes_matches() {
        let matcher = compile_pattern(r"(?i)token\s+[a-z0-9-]{4,}").expect("pattern must compile");
        let matched = matcher.find("TOKEN sk-test-1234").expect("pattern should detect token");
        let normalized = normalize_value(matched.as_str(), NormalizationMode::AlnumLowercase);
        assert_eq!(normalized, "tokensktest1234");
    }

    #[test]
    fn sort_order_is_stable_for_equal_spans() {
        let mut candidates = vec![
            PatternCandidate {
                recognizer_id: "r2".to_string(),
                pattern_id: "p2".to_string(),
                start: 0,
                end: 10,
                score: 80,
                family_priority: 10,
            },
            PatternCandidate {
                recognizer_id: "r1".to_string(),
                pattern_id: "p1".to_string(),
                start: 0,
                end: 10,
                score: 80,
                family_priority: 10,
            },
        ];

        stable_sort_candidates(&mut candidates);
        assert_eq!(candidates[0].recognizer_id, "r1");
        stable_sort_candidates(&mut candidates);
        assert_eq!(candidates[0].recognizer_id, "r1");
    }
}
