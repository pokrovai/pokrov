use regex::{Regex, RegexBuilder};

/// Compiles deterministic regex patterns at startup.
pub fn compile_pattern(expression: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(expression).size_limit(1024 * 1024).build()
}

#[cfg(test)]
mod tests {
    use super::compile_pattern;

    #[test]
    fn compiles_patterns_and_finds_matches() {
        let matcher = compile_pattern(r"(?i)token\s+[a-z0-9-]{4,}").expect("pattern must compile");
        let matched = matcher.find("TOKEN sk-test-1234").expect("pattern should detect token");
        assert_eq!(matched.as_str(), "TOKEN sk-test-1234");
    }
}
