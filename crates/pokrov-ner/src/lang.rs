/// Detects script language from character distribution.
/// Returns "ru" if Cyrillic characters dominate, "en" if ASCII alphabetic
/// characters are present, or "unknown" otherwise.
pub fn detect_language(text: &str) -> String {
    let mut cyrillic_count = 0usize;
    let mut ascii_count = 0usize;

    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            ascii_count += 1;
        } else if matches!(c, '\u{0400}'..='\u{04FF}') {
            cyrillic_count += 1;
        }
    }

    if cyrillic_count > 0 {
        "ru".to_string()
    } else if ascii_count > 0 {
        "en".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detection_russian() {
        assert_eq!(detect_language("Иван Петров"), "ru");
    }

    #[test]
    fn language_detection_english() {
        assert_eq!(detect_language("John Smith"), "en");
    }

    #[test]
    fn language_detection_unknown_when_no_alpha() {
        assert_eq!(detect_language("   "), "unknown");
    }

    #[test]
    fn language_detection_prefers_cyrillic() {
        assert_eq!(detect_language("Привет John"), "ru");
    }
}
