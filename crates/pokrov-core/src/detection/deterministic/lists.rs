use std::collections::BTreeSet;

/// Normalizes list-control values for deterministic exact-match comparisons.
pub fn normalize_exact_value(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Builds the normalized allowlist set used by request-scoped suppression.
pub fn build_allowlist_set(entries: &[String]) -> BTreeSet<String> {
    entries
        .iter()
        .map(|entry| normalize_exact_value(entry))
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// Checks exact normalized suppression without substring matching.
pub fn is_allowlisted_exact(allowlist: &BTreeSet<String>, raw_match: &str) -> bool {
    let normalized = normalize_exact_value(raw_match);
    !normalized.is_empty() && allowlist.contains(&normalized)
}

#[cfg(test)]
mod tests {
    use super::{build_allowlist_set, is_allowlisted_exact, normalize_exact_value};

    #[test]
    fn normalizes_whitespace_and_case() {
        assert_eq!(normalize_exact_value("  SK-Test-1  "), "sk-test-1");
        assert_eq!(normalize_exact_value("a   b"), "a b");
    }

    #[test]
    fn exact_comparison_does_not_match_substrings() {
        let allowlist = build_allowlist_set(&["user@example.com".to_string()]);
        assert!(is_allowlisted_exact(&allowlist, "USER@EXAMPLE.COM"));
        assert!(!is_allowlisted_exact(&allowlist, "prefix user@example.com suffix"));
    }
}
