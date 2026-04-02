use uuid::Uuid;

pub fn normalize_request_id(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.len() <= 128)
        .filter(|value| value.is_ascii())
        .map(str::to_string)
}

pub fn normalize_or_generate_request_id(input: Option<&str>) -> String {
    normalize_request_id(input).unwrap_or_else(|| Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_or_generate_request_id, normalize_request_id};

    #[test]
    fn keeps_non_empty_header_value() {
        let request_id = normalize_request_id(Some(" external-id-1 "));
        assert_eq!(request_id.as_deref(), Some("external-id-1"));
    }

    #[test]
    fn rejects_empty_header_value() {
        let request_id = normalize_request_id(Some("   "));
        assert!(request_id.is_none());
    }

    #[test]
    fn generates_uuid_when_missing() {
        let request_id = normalize_or_generate_request_id(None);
        assert!(!request_id.is_empty());
        assert_eq!(request_id.len(), 36);
    }
}
