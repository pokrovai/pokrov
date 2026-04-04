use pokrov_config::UpstreamAuthMode;
use pokrov_core::types::PolicyAction;
use serde_json::Value;

use crate::{
    errors::LLMProxyError,
    types::{LLMResponseMetadata, UpstreamCredentialOrigin},
};

pub(super) struct TerminalEvent<'a> {
    pub(super) request_id: &'a str,
    pub(super) endpoint: &'a str,
    pub(super) profile_id: &'a str,
    pub(super) provider_id: Option<String>,
    pub(super) model: &'a str,
    pub(super) stream: bool,
    pub(super) final_action: PolicyAction,
    pub(super) rule_hits_total: u32,
    pub(super) blocked: bool,
    pub(super) upstream_status: Option<u16>,
    pub(super) duration_ms: u64,
    pub(super) estimated_token_units: u32,
    pub(super) auth_mode: &'a str,
    pub(super) credential_origin: UpstreamCredentialOrigin,
}

pub(super) fn mode_as_str(mode: UpstreamAuthMode) -> &'static str {
    match mode {
        UpstreamAuthMode::Static => "static",
        UpstreamAuthMode::Passthrough => "passthrough",
    }
}

pub(super) fn attach_pokrov_metadata(
    request_id: &str,
    profile_id: &str,
    provider_id: &str,
    final_action: PolicyAction,
    total_hits: u32,
    sanitized_input: bool,
    sanitized_output: bool,
    estimated_token_units: u32,
    payload: &mut Value,
) -> Result<(), LLMProxyError> {
    let object = payload.as_object_mut().ok_or_else(|| {
        LLMProxyError::upstream_error(
            request_id,
            Some(provider_id.to_string()),
            "upstream JSON response must be an object",
        )
    })?;

    object.insert(
        "pokrov".to_string(),
        serde_json::to_value(LLMResponseMetadata {
            profile: profile_id.to_string(),
            sanitized_input,
            sanitized_output,
            action: final_action,
            rule_hits: total_hits,
            estimated_token_units,
            observed_token_units: None,
            provider: Some(provider_id.to_string()),
        })
        .map_err(|error| {
            LLMProxyError::upstream_error(
                request_id,
                Some(provider_id.to_string()),
                format!("failed to serialize response metadata: {error}"),
            )
        })?,
    );
    Ok(())
}

pub(super) fn attach_request_id(
    request_id: &str,
    provider_id: &str,
    payload: &mut Value,
) -> Result<(), LLMProxyError> {
    let object = payload.as_object_mut().ok_or_else(|| {
        LLMProxyError::upstream_error(
            request_id,
            Some(provider_id.to_string()),
            "upstream JSON response must be an object",
        )
    })?;

    object.insert(
        "request_id".to_string(),
        Value::String(request_id.to_string()),
    );
    Ok(())
}

pub(super) fn max_action(left: PolicyAction, right: PolicyAction) -> PolicyAction {
    if right.strictness_rank() > left.strictness_rank() {
        right
    } else {
        left
    }
}
