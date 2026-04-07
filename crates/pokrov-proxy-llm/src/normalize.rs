use std::collections::BTreeMap;

use serde_json::Value;

use crate::{
    errors::LLMProxyError,
    types::{ContentBlock, LLMMessage, LLMRequestEnvelope, MessageContent, ALLOWED_ROLES},
};

pub fn normalize_request(
    request_id: &str,
    payload: Value,
) -> Result<LLMRequestEnvelope, LLMProxyError> {
    let object = payload.as_object().ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, "request body must be a JSON object")
    })?;

    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LLMProxyError::invalid_request(request_id, "field 'model' must be a non-empty string")
        })?
        .to_string();

    let messages_value = object.get("messages").ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, "field 'messages' is required")
    })?;

    let messages = normalize_messages(request_id, messages_value)?;

    let stream = object.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let metadata_tags = parse_metadata_tags(object.get("metadata"));
    let profile_hint = metadata_tags.get("profile").cloned();

    Ok(LLMRequestEnvelope {
        request_id: request_id.to_string(),
        model,
        messages,
        stream,
        profile_hint,
        metadata_tags,
        original_payload: payload,
    })
}

pub fn normalize_responses_payload(
    request_id: &str,
    payload: Value,
) -> Result<Value, LLMProxyError> {
    let object = payload.as_object().ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, "request body must be a JSON object")
    })?;

    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LLMProxyError::invalid_request(request_id, "field 'model' must be a non-empty string")
        })?;
    let stream = object.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let input = object
        .get("input")
        .ok_or_else(|| LLMProxyError::invalid_request(request_id, "field 'input' is required"))?;
    let messages = normalize_responses_input(request_id, input)?;

    let mut mapped = serde_json::Map::from_iter([
        ("model".to_string(), Value::String(model.to_string())),
        ("stream".to_string(), Value::Bool(stream)),
        ("messages".to_string(), Value::Array(messages)),
    ]);

    if let Some(metadata) = object.get("metadata").and_then(Value::as_object) {
        mapped.insert("metadata".to_string(), Value::Object(metadata.clone()));
    }

    Ok(Value::Object(mapped))
}

pub fn resolve_profile_id(
    profile_hint: Option<&str>,
    api_key_profile: &str,
    provider_profile: Option<&str>,
    default_profile: &str,
) -> String {
    if let Some(hint) = profile_hint.map(str::trim).filter(|value| !value.is_empty()) {
        if is_known_profile(hint) {
            return hint.to_string();
        }
    }

    if let Some(provider_profile) = provider_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if is_known_profile(provider_profile) {
            return provider_profile.to_string();
        }
    }

    if is_known_profile(api_key_profile) {
        return api_key_profile.to_string();
    }

    default_profile.to_string()
}

pub fn estimate_token_units(payload: &Value) -> u32 {
    estimate_token_units_inner(payload).max(1)
}

fn normalize_messages(request_id: &str, value: &Value) -> Result<Vec<LLMMessage>, LLMProxyError> {
    let messages = value.as_array().ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, "field 'messages' must be an array")
    })?;

    if messages.is_empty() {
        return Err(LLMProxyError::invalid_request(
            request_id,
            "field 'messages' must contain at least one item",
        ));
    }

    messages
        .iter()
        .enumerate()
        .map(|(index, item)| normalize_message(request_id, index, item))
        .collect()
}

fn normalize_responses_input(request_id: &str, input: &Value) -> Result<Vec<Value>, LLMProxyError> {
    match input {
        Value::String(text) => Ok(vec![serde_json::json!({
            "role": "user",
            "content": text,
        })]),
        Value::Object(message) => {
            normalize_responses_message_object(request_id, message).map(|item| vec![item])
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Err(LLMProxyError::invalid_request(
                    request_id,
                    "field 'input' must contain at least one item",
                ));
            }

            items
                .iter()
                .enumerate()
                .map(|(idx, item)| match item {
                    Value::String(text) => Ok(serde_json::json!({
                        "role": "user",
                        "content": text,
                    })),
                    Value::Object(message) => {
                        normalize_responses_message_object(request_id, message)
                    }
                    _ => Err(LLMProxyError::invalid_request(
                        request_id,
                        format!("input[{idx}] is not supported in minimal responses subset"),
                    )),
                })
                .collect()
        }
        _ => Err(LLMProxyError::invalid_request(
            request_id,
            "field 'input' is not supported in minimal responses subset",
        )),
    }
}

fn normalize_responses_message_object(
    request_id: &str,
    message: &serde_json::Map<String, Value>,
) -> Result<Value, LLMProxyError> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LLMProxyError::invalid_request(request_id, "input message requires non-empty 'role'")
        })?;
    let content = message.get("content").ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, "input message requires 'content'")
    })?;

    match content {
        Value::String(_) | Value::Array(_) => {}
        _ => {
            return Err(LLMProxyError::invalid_request(
                request_id,
                "input message 'content' must be a string or array",
            ));
        }
    }

    Ok(serde_json::json!({
        "role": role,
        "content": content,
    }))
}

fn normalize_message(
    request_id: &str,
    index: usize,
    value: &Value,
) -> Result<LLMMessage, LLMProxyError> {
    let object = value.as_object().ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, format!("messages[{index}] must be an object"))
    })?;

    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LLMProxyError::invalid_request(
                request_id,
                format!("messages[{index}].role is required"),
            )
        })?
        .to_string();

    if !ALLOWED_ROLES.contains(&role.as_str()) {
        return Err(LLMProxyError::invalid_request(
            request_id,
            format!("messages[{index}].role must be one of system|user|assistant|tool"),
        ));
    }

    let content = object.get("content").ok_or_else(|| {
        LLMProxyError::invalid_request(request_id, format!("messages[{index}].content is required"))
    })?;

    let content = normalize_content(request_id, index, content)?;

    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok(LLMMessage { role, content, name })
}

fn normalize_content(
    request_id: &str,
    message_index: usize,
    value: &Value,
) -> Result<MessageContent, LLMProxyError> {
    if let Some(text) = value.as_str() {
        return Ok(MessageContent::Text(text.to_string()));
    }

    let blocks = value.as_array().ok_or_else(|| {
        LLMProxyError::invalid_request(
            request_id,
            format!("messages[{message_index}].content must be a string or array"),
        )
    })?;

    let mut normalized_blocks = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        let block_object = block.as_object().ok_or_else(|| {
            LLMProxyError::invalid_request(
                request_id,
                format!("messages[{message_index}].content[{index}] must be an object"),
            )
        })?;

        let block_type = block_object
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                LLMProxyError::invalid_request(
                    request_id,
                    format!("messages[{message_index}].content[{index}].type is required"),
                )
            })?
            .to_string();

        let text = block_object.get("text").and_then(Value::as_str).map(str::to_string);
        let json = block_object.get("json").cloned();

        normalized_blocks.push(ContentBlock { block_type, text, json });
    }

    Ok(MessageContent::Blocks(normalized_blocks))
}

fn parse_metadata_tags(value: Option<&Value>) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    let Some(metadata) = value.and_then(Value::as_object) else {
        return tags;
    };

    for (key, value) in metadata {
        if let Some(value) = value.as_str() {
            tags.insert(key.clone(), value.to_string());
        }
    }

    tags
}

fn is_known_profile(profile: &str) -> bool {
    matches!(profile, "minimal" | "strict" | "custom")
}

fn estimate_token_units_inner(payload: &Value) -> u32 {
    match payload {
        Value::Null | Value::Bool(_) => 0,
        Value::Number(_) => 1,
        Value::String(text) => {
            // The estimator stays deterministic and cheap on the hot path by
            // using character length instead of provider-specific tokenization.
            (text.chars().count() as u32 / 4).max(1)
        }
        Value::Array(values) => values
            .iter()
            .fold(0u32, |acc, value| acc.saturating_add(estimate_token_units_inner(value))),
        Value::Object(map) => map
            .values()
            .fold(0u32, |acc, value| acc.saturating_add(estimate_token_units_inner(value))),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        estimate_token_units, normalize_request, normalize_responses_payload, resolve_profile_id,
    };

    #[test]
    fn normalize_valid_request_and_resolve_profile_precedence() {
        let payload = serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}],
            "metadata": {"profile": "minimal"}
        });

        let normalized = normalize_request("req-1", payload).expect("request should normalize");
        assert_eq!(normalized.model, "gpt-4o-mini");
        assert_eq!(normalized.messages.len(), 1);

        let profile =
            resolve_profile_id(normalized.profile_hint.as_deref(), "strict", None, "custom");
        assert_eq!(profile, "minimal");
    }

    #[test]
    fn resolve_profile_falls_back_when_hint_is_invalid() {
        let profile = resolve_profile_id(Some("unknown"), "strict", None, "custom");
        assert_eq!(profile, "strict");
    }

    #[test]
    fn resolve_profile_uses_provider_profile_before_default() {
        let profile = resolve_profile_id(None, "unknown", Some("custom"), "strict");
        assert_eq!(profile, "custom");
    }

    #[test]
    fn resolve_profile_uses_provider_profile_before_api_key_profile() {
        let profile = resolve_profile_id(None, "strict", Some("minimal"), "custom");
        assert_eq!(profile, "minimal");
    }

    #[test]
    fn estimates_token_units_from_nested_json_strings() {
        let payload = json!({
            "messages": [
                {"content": "abcd"},
                {"content": ["abcdefgh", {"text": "abcdefghijkl"}]}
            ]
        });

        let units = estimate_token_units(&payload);
        assert_eq!(units, 6);
    }

    #[test]
    fn estimate_token_units_has_deterministic_minimum_fallback() {
        assert_eq!(estimate_token_units(&json!(null)), 1);
        assert_eq!(estimate_token_units(&json!({"n": null, "flag": true, "arr": [0, false]})), 1);
    }

    #[test]
    fn maps_minimal_responses_payload_to_chat_completions_shape() {
        let payload = json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "input": "hello",
            "metadata": {"profile":"strict"},
        });

        let mapped = normalize_responses_payload("req-1", payload).expect("payload should map");
        assert_eq!(mapped["model"], "gpt-4o-mini");
        assert_eq!(mapped["stream"], false);
        assert_eq!(mapped["messages"][0]["role"], "user");
        assert_eq!(mapped["messages"][0]["content"], "hello");
        assert_eq!(mapped["metadata"]["profile"], "strict");
    }

    #[test]
    fn rejects_unsupported_responses_input_item() {
        let payload = json!({
            "model": "gpt-4o-mini",
            "input": [42],
        });

        let error =
            normalize_responses_payload("req-2", payload).expect_err("unsupported item must fail");
        assert_eq!(error.code().as_str(), "invalid_request");
    }
}
