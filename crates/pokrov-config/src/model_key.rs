/// Normalizes model ids and aliases using ASCII case folding.
/// Non-ASCII symbols are preserved intentionally for deterministic v1 behavior.
pub fn normalize_model_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::normalize_model_key;

    #[test]
    fn applies_ascii_case_folding_and_trim() {
        assert_eq!(normalize_model_key("  OPENAI/GPT-4O-MINI  "), "openai/gpt-4o-mini");
    }

    #[test]
    fn preserves_non_ascii_codepoints() {
        assert_eq!(
            normalize_model_key("  Модель/Τεστ  "),
            "Модель/Τεστ"
        );
    }
}
