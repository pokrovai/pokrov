use crate::types::EvaluationMode;

pub fn is_execution_enabled(mode: EvaluationMode) -> bool {
    matches!(mode, EvaluationMode::Enforce)
}

#[cfg(test)]
mod tests {
    use crate::types::EvaluationMode;

    use super::is_execution_enabled;

    #[test]
    fn returns_false_for_dry_run_mode() {
        assert!(!is_execution_enabled(EvaluationMode::DryRun));
    }

    #[test]
    fn returns_true_for_enforce_mode() {
        assert!(is_execution_enabled(EvaluationMode::Enforce));
    }
}
