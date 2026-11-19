//! Marker accounting for malformed tool-call values forwarded verbatim.

use serde_json::Value;

use crate::rehydrate::{count_unrestored_markers, RehydrationMap};

/// Counts complete `__PKV_` markers in `function.arguments` strings of a
/// bypassed malformed tool-call value. Never restores or retains payloads.
pub(crate) fn count_bypassed_argument_markers(value: &Value, map: &RehydrationMap) -> u32 {
    match value {
        Value::Array(entries) => entries
            .iter()
            .map(|entry| count_bypassed_argument_markers(entry, map))
            .fold(0, u32::saturating_add),
        Value::Object(entry) => match entry.get("function") {
            Some(Value::Object(function)) => function
                .get("arguments")
                .and_then(Value::as_str)
                .map_or(0, |text| count_unrestored_markers(text.as_bytes(), map)),
            _ => 0,
        },
        _ => 0,
    }
}
