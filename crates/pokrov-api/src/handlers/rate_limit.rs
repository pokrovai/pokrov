use crate::app::{AppState, RateLimitDecision, RateLimitReason};
use serde_json::{Number, Value};

pub(super) async fn evaluate_and_record_rate_limit(
    state: &AppState,
    route: &'static str,
    token: &str,
    profile_id: &str,
    estimated_units: u32,
) -> Option<RateLimitDecision> {
    let limiter = state.rate_limit.limiter.as_ref()?;
    let decision = limiter.evaluate(token, profile_id, estimated_units).await;
    if !matches!(decision.reason, RateLimitReason::WithinBudget) {
        state.metrics.on_rate_limit_event(
            route,
            match decision.reason {
                RateLimitReason::RequestBudgetExhausted => "requests",
                RateLimitReason::TokenBudgetExhausted => "token_units",
                RateLimitReason::WithinBudget => "requests",
            },
            if decision.allowed { "dry_run" } else { "blocked" },
            profile_id,
        );
    }

    Some(decision)
}

pub(super) fn estimate_json_token_units(payload: &Value) -> u32 {
    ((estimate_json_size_bytes(payload) / 4) as u32).max(1)
}

fn estimate_json_size_bytes(value: &Value) -> usize {
    match value {
        Value::Null => 4,
        Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => estimate_number_bytes(number),
        Value::String(text) => escaped_string_size_bytes(text),
        Value::Array(values) => {
            let mut bytes: usize = 2;
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    bytes = bytes.saturating_add(1);
                }
                bytes = bytes.saturating_add(estimate_json_size_bytes(item));
            }
            bytes
        }
        Value::Object(map) => {
            let mut bytes: usize = 2;
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    bytes = bytes.saturating_add(1);
                }
                bytes = bytes
                    .saturating_add(escaped_string_size_bytes(key))
                    .saturating_add(1)
                    .saturating_add(estimate_json_size_bytes(item));
            }
            bytes
        }
    }
}

fn escaped_string_size_bytes(text: &str) -> usize {
    let mut bytes: usize = 2;
    for ch in text.chars() {
        bytes = bytes.saturating_add(match ch {
            '"' | '\\' => 2,
            '\u{08}' | '\u{0C}' | '\n' | '\r' | '\t' => 2,
            ch if ch <= '\u{1F}' => 6,
            _ => ch.len_utf8(),
        });
    }
    bytes
}

fn estimate_number_bytes(number: &Number) -> usize {
    if let Some(value) = number.as_u64() {
        return decimal_digits(value);
    }

    if let Some(value) = number.as_i64() {
        let sign = if value < 0 { 1 } else { 0 };
        return sign + decimal_digits(value.unsigned_abs());
    }

    // serde_json uses compact float formatting; 24 bytes safely covers finite f64 text.
    24
}

fn decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::estimate_json_token_units;

    #[test]
    fn estimate_matches_serialized_length_for_common_payloads() {
        let samples = [
            json!({"path":"src/main.rs"}),
            json!({"a":[1,true,null,"value"]}),
            json!({"nested":{"tool":"read_file","args":{"path":"README.md"}}}),
        ];

        for sample in samples {
            let expected = ((serde_json::to_vec(&sample).expect("payload should serialize").len() / 4) as u32)
                .max(1);
            assert_eq!(estimate_json_token_units(&sample), expected);
        }
    }
}
