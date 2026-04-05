use serde_json::Value;

pub fn visit_string_leaves(value: &Value, visitor: &mut dyn FnMut(&str, &str)) {
    let mut pointer = String::new();
    visit_recursive(value, &mut pointer, visitor);
}

pub fn map_string_leaves(
    value: &Value,
    mapper: &mut dyn FnMut(&str, &str) -> String,
) -> (Value, u32) {
    let mut transformed_fields = 0;
    let mut pointer = String::new();
    let mapped = map_recursive(value, &mut pointer, mapper, &mut transformed_fields);
    (mapped, transformed_fields)
}

fn visit_recursive(value: &Value, pointer: &mut String, visitor: &mut dyn FnMut(&str, &str)) {
    match value {
        Value::String(text) => visitor(pointer, text),
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                let base_len = pointer.len();
                push_pointer_segment(pointer, &idx.to_string());
                visit_recursive(item, pointer, visitor);
                pointer.truncate(base_len);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                let base_len = pointer.len();
                push_pointer_segment(pointer, key);
                visit_recursive(item, pointer, visitor);
                pointer.truncate(base_len);
            }
        }
        _ => {}
    }
}

fn map_recursive(
    value: &Value,
    pointer: &mut String,
    mapper: &mut dyn FnMut(&str, &str) -> String,
    transformed_fields: &mut u32,
) -> Value {
    match value {
        Value::String(text) => {
            let transformed = mapper(pointer, text);
            if transformed != *text {
                *transformed_fields += 1;
            }
            Value::String(transformed)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let base_len = pointer.len();
                    push_pointer_segment(pointer, &idx.to_string());
                    let mapped = map_recursive(item, pointer, mapper, transformed_fields);
                    pointer.truncate(base_len);
                    mapped
                })
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| {
                    let base_len = pointer.len();
                    push_pointer_segment(pointer, key);
                    let mapped = map_recursive(item, pointer, mapper, transformed_fields);
                    pointer.truncate(base_len);
                    (key.clone(), mapped)
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn push_pointer_segment(pointer: &mut String, raw_segment: &str) {
    pointer.push('/');
    for ch in raw_segment.chars() {
        match ch {
            '~' => pointer.push_str("~0"),
            '/' => pointer.push_str("~1"),
            other => pointer.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{map_string_leaves, visit_string_leaves};

    #[test]
    fn maps_only_string_leaves_and_preserves_shape() {
        let input = json!({
            "a": "secret",
            "nested": {
                "b": 42,
                "c": ["alpha", true, {"x": "beta"}]
            }
        });

        let (mapped, transformed_fields) =
            map_string_leaves(&input, &mut |_ptr, text| format!("***{text}***"));

        assert_eq!(mapped["nested"]["b"], 42);
        assert_eq!(mapped["nested"]["c"][1], true);
        assert_eq!(mapped["a"], "***secret***");
        assert_eq!(mapped["nested"]["c"][2]["x"], "***beta***");
        assert_eq!(transformed_fields, 3);
    }

    #[test]
    fn traverses_json_pointer_paths_deterministically() {
        let input = json!({"messages": [{"content": "a"}, {"content": "b"}]});
        let mut seen = Vec::new();

        visit_string_leaves(&input, &mut |pointer, _| seen.push(pointer.to_string()));

        assert_eq!(seen, vec!["/messages/0/content", "/messages/1/content"]);
    }
}
