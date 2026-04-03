use pokrov_config::model::{McpToolPolicy, ToolArgumentConstraints};
use serde_json::Value;
use std::path::Component;

use crate::{
    errors::McpProxyError,
    types::{ToolValidationResult, ValidationStage, ValidationViolation},
};

/// Validates MCP tool arguments against schema and policy constraints before upstream execution.
pub fn validate_tool_arguments(
    request_id: &str,
    server: &str,
    tool: &str,
    arguments: &Value,
    policy: Option<&McpToolPolicy>,
) -> Result<ToolValidationResult, McpProxyError> {
    if !arguments.is_object() {
        return Err(McpProxyError::invalid_request(
            request_id,
            "arguments must be a JSON object",
        ));
    }

    let Some(policy) = policy else {
        return Ok(ToolValidationResult {
            valid: true,
            stage: ValidationStage::Constraints,
            violations: Vec::new(),
        });
    };

    if let Some(schema) = policy.argument_schema.as_ref() {
        let schema_violations = validate_schema(schema, arguments);
        if !schema_violations.is_empty() {
            return Err(McpProxyError::argument_validation_failed(
                request_id,
                server,
                tool,
                schema_violations.len() as u32,
            ));
        }
    }

    let violations = validate_constraints(&policy.argument_constraints, arguments);
    if !violations.is_empty() {
        return Err(McpProxyError::argument_validation_failed(
            request_id,
            server,
            tool,
            violations.len() as u32,
        ));
    }

    Ok(ToolValidationResult {
        valid: true,
        stage: ValidationStage::Constraints,
        violations: Vec::new(),
    })
}

fn validate_schema(schema: &Value, arguments: &Value) -> Vec<ValidationViolation> {
    let mut violations = Vec::new();
    let disallow_additional = schema
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .map(|value| !value)
        .unwrap_or(false);

    if let Some(expected_type) = schema.get("type").and_then(Value::as_str) {
        if !validate_value_type(arguments, expected_type) {
            violations.push(violation(
                "schema_type_mismatch",
                "$",
                "arguments do not match schema root type",
            ));
        }
    }

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) {
            if arguments.get(key).is_none() {
                violations.push(violation(
                    "schema_required_missing",
                    format!("$.{key}"),
                    "required key is missing",
                ));
            }
        }
    }

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, prop_schema) in properties {
            let Some(value) = arguments.get(key) else {
                continue;
            };

            if let Some(expected_type) = prop_schema.get("type").and_then(Value::as_str) {
                if !validate_value_type(value, expected_type) {
                    violations.push(violation(
                        "schema_property_type_mismatch",
                        format!("$.{key}"),
                        "value does not match schema property type",
                    ));
                }
            }
        }

    }

    if disallow_additional {
        let schema_properties = schema.get("properties").and_then(Value::as_object);
        if let Some(object) = arguments.as_object() {
            for key in object.keys() {
                let key_is_allowed = schema_properties
                    .map(|properties| properties.contains_key(key))
                    .unwrap_or(false);
                if !key_is_allowed {
                    violations.push(violation(
                        "schema_additional_property",
                        format!("$.{key}"),
                        "unexpected property is not allowed",
                    ));
                }
            }
        }
    }

    violations
}

fn validate_constraints(
    constraints: &ToolArgumentConstraints,
    arguments: &Value,
) -> Vec<ValidationViolation> {
    let mut violations = Vec::new();

    if let Some(object) = arguments.as_object() {
        for key in &constraints.required_keys {
            if !object.contains_key(key) {
                violations.push(violation(
                    "required_key_missing",
                    format!("$.{key}"),
                    "required key is missing",
                ));
            }
        }

        for key in &constraints.forbidden_keys {
            if object.contains_key(key) {
                violations.push(violation(
                    "forbidden_key_present",
                    format!("$.{key}"),
                    "forbidden key is present",
                ));
            }
        }
    }

    if let Some(max_depth) = constraints.max_depth {
        let depth = json_depth(arguments, 1);
        if depth > max_depth {
            violations.push(violation(
                "max_depth_exceeded",
                "$",
                format!("maximum depth exceeded: {depth}>{max_depth}"),
            ));
        }
    }

    if let Some(max_string_length) = constraints.max_string_length {
        collect_long_strings(arguments, "$", max_string_length, &mut violations);
    }

    if !constraints.allowed_path_prefixes.is_empty() {
        collect_disallowed_paths(
            arguments,
            None,
            "$",
            &constraints.allowed_path_prefixes,
            &mut violations,
        );
    }

    violations
}

fn collect_long_strings(
    value: &Value,
    path: &str,
    max_string_length: usize,
    violations: &mut Vec<ValidationViolation>,
) {
    match value {
        Value::String(text) => {
            if text.len() > max_string_length {
                violations.push(violation(
                    "max_string_length_exceeded",
                    path,
                    format!("string length exceeds configured limit {max_string_length}"),
                ));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_long_strings(item, &format!("{path}[{index}]"), max_string_length, violations);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                collect_long_strings(item, &format!("{path}.{key}"), max_string_length, violations);
            }
        }
        _ => {}
    }
}

fn collect_disallowed_paths(
    value: &Value,
    current_key: Option<&str>,
    path: &str,
    allowed_prefixes: &[String],
    violations: &mut Vec<ValidationViolation>,
) {
    match value {
        Value::String(candidate_path) => {
            let is_path_key = current_key
                .map(|key| key == "path" || key.ends_with("_path"))
                .unwrap_or(false);

            if is_path_key && !path_matches_allowed_prefixes(candidate_path, allowed_prefixes) {
                violations.push(violation(
                    "path_prefix_not_allowed",
                    path,
                    "path value is outside allowlisted prefixes",
                ));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_disallowed_paths(
                    item,
                    current_key,
                    &format!("{path}[{index}]"),
                    allowed_prefixes,
                    violations,
                );
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                collect_disallowed_paths(
                    item,
                    Some(key),
                    &format!("{path}.{key}"),
                    allowed_prefixes,
                    violations,
                );
            }
        }
        _ => {}
    }
}

fn path_matches_allowed_prefixes(candidate_path: &str, allowed_prefixes: &[String]) -> bool {
    let Some(candidate_segments) = normalized_path_segments(candidate_path) else {
        return false;
    };

    allowed_prefixes.iter().any(|prefix| {
        let Some(prefix_segments) = normalized_path_segments(prefix) else {
            return false;
        };
        !prefix_segments.is_empty() && candidate_segments.starts_with(&prefix_segments)
    })
}

fn normalized_path_segments(path: &str) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    for component in std::path::Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => segments.push(segment.to_string_lossy().into_owned()),
            Component::ParentDir => {
                if segments.pop().is_none() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(segments)
}

fn validate_value_type(value: &Value, expected_type: &str) -> bool {
    match expected_type {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn json_depth(value: &Value, current_depth: u8) -> u8 {
    match value {
        Value::Array(items) => items
            .iter()
            .map(|item| json_depth(item, current_depth.saturating_add(1)))
            .max()
            .unwrap_or(current_depth),
        Value::Object(map) => map
            .values()
            .map(|item| json_depth(item, current_depth.saturating_add(1)))
            .max()
            .unwrap_or(current_depth),
        _ => current_depth,
    }
}

fn violation(code: impl Into<String>, path: impl Into<String>, message: impl Into<String>) -> ValidationViolation {
    ValidationViolation {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use pokrov_config::model::{McpToolPolicy, ToolArgumentConstraints};

    use super::validate_tool_arguments;
    use crate::errors::McpErrorCode;

    #[test]
    fn rejects_arguments_with_forbidden_keys() {
        let policy = McpToolPolicy {
            enabled: true,
            argument_schema: None,
            argument_constraints: ToolArgumentConstraints {
                required_keys: vec!["path".to_string()],
                forbidden_keys: vec!["command".to_string()],
                max_depth: None,
                max_string_length: None,
                allowed_path_prefixes: Vec::new(),
            },
            output_sanitization: Some(true),
        };

        let error = validate_tool_arguments(
            "req-1",
            "repo-tools",
            "read_file",
            &serde_json::json!({"path": "src/lib.rs", "command": "cat /etc/passwd"}),
            Some(&policy),
        )
        .expect_err("forbidden key must fail validation");

        assert_eq!(error.code(), McpErrorCode::ArgumentValidationFailed);
    }

    #[test]
    fn accepts_arguments_when_constraints_are_satisfied() {
        let policy = McpToolPolicy {
            enabled: true,
            argument_schema: None,
            argument_constraints: ToolArgumentConstraints {
                required_keys: vec!["path".to_string()],
                forbidden_keys: vec!["command".to_string()],
                max_depth: Some(4),
                max_string_length: Some(128),
                allowed_path_prefixes: vec!["src/".to_string()],
            },
            output_sanitization: Some(true),
        };

        let result = validate_tool_arguments(
            "req-2",
            "repo-tools",
            "read_file",
            &serde_json::json!({"path": "src/lib.rs"}),
            Some(&policy),
        )
        .expect("valid payload must pass validation");

        assert!(result.valid);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn rejects_additional_properties_when_schema_disallows_them_without_properties_map() {
        let policy = McpToolPolicy {
            enabled: true,
            argument_schema: Some(serde_json::json!({
                "type": "object",
                "additionalProperties": false
            })),
            argument_constraints: ToolArgumentConstraints {
                required_keys: Vec::new(),
                forbidden_keys: Vec::new(),
                max_depth: None,
                max_string_length: None,
                allowed_path_prefixes: Vec::new(),
            },
            output_sanitization: Some(true),
        };

        let error = validate_tool_arguments(
            "req-3",
            "repo-tools",
            "read_file",
            &serde_json::json!({"path": "src/lib.rs"}),
            Some(&policy),
        )
        .expect_err("schema with additionalProperties=false must reject unexpected keys");

        assert_eq!(error.code(), McpErrorCode::ArgumentValidationFailed);
    }

    #[test]
    fn rejects_traversal_path_that_lexically_escapes_allowed_prefix() {
        let policy = McpToolPolicy {
            enabled: true,
            argument_schema: None,
            argument_constraints: ToolArgumentConstraints {
                required_keys: vec!["path".to_string()],
                forbidden_keys: Vec::new(),
                max_depth: None,
                max_string_length: None,
                allowed_path_prefixes: vec!["src/".to_string()],
            },
            output_sanitization: Some(true),
        };

        let error = validate_tool_arguments(
            "req-4",
            "repo-tools",
            "read_file",
            &serde_json::json!({"path": "src/../secrets.txt"}),
            Some(&policy),
        )
        .expect_err("path traversal form must not pass allowlisted path prefix checks");

        assert_eq!(error.code(), McpErrorCode::ArgumentValidationFailed);
    }
}
