/// Supported deterministic validators for high-confidence recognizers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorKind {
    Luhn,
}

/// Runs one deterministic validator against the normalized candidate value.
pub fn validate_candidate(kind: ValidatorKind, candidate: &str) -> bool {
    match kind {
        ValidatorKind::Luhn => luhn_valid(candidate),
    }
}

fn luhn_valid(candidate: &str) -> bool {
    // v1 deterministic PAN validation accepts only ASCII digits to keep matching deterministic
    // across providers and avoid locale-sensitive Unicode numeral handling.
    let digits = candidate.chars().filter(|ch| ch.is_ascii_digit()).collect::<Vec<_>>();
    if digits.len() < 13 {
        return false;
    }

    let mut checksum = 0u32;
    let mut doubled = false;
    for digit in digits.iter().rev() {
        let mut value = digit.to_digit(10).unwrap_or_default();
        if doubled {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        checksum += value;
        doubled = !doubled;
    }
    checksum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::{validate_candidate, ValidatorKind};

    #[test]
    fn luhn_rejects_invalid_candidate() {
        assert!(!validate_candidate(ValidatorKind::Luhn, "4111 1111 1111 1112"));
    }

    #[test]
    fn luhn_rejects_candidates_shorter_than_pan_floor() {
        assert!(!validate_candidate(ValidatorKind::Luhn, "79927398713"));
        assert!(!validate_candidate(ValidatorKind::Luhn, "799273987130"));
    }

    #[test]
    fn luhn_accepts_valid_candidate() {
        assert!(validate_candidate(ValidatorKind::Luhn, "4111 1111 1111 1111"));
    }
}
