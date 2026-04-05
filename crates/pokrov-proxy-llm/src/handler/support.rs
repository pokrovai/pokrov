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

pub(super) struct ResponseMetadataContext<'a> {
    pub(super) request_id: &'a str,
    pub(super) profile_id: &'a str,
    pub(super) provider_id: &'a str,
    pub(super) final_action: PolicyAction,
    pub(super) total_hits: u32,
    pub(super) sanitized_input: bool,
    pub(super) sanitized_output: bool,
    pub(super) estimated_token_units: u32,
}

pub(super) fn mode_as_str(mode: UpstreamAuthMode) -> &'static str {
    match mode {
        UpstreamAuthMode::Static => "static",
        UpstreamAuthMode::Passthrough => "passthrough",
    }
}

pub(super) fn attach_pokrov_metadata(
    context: ResponseMetadataContext<'_>,
    payload: &mut Value,
) -> Result<(), LLMProxyError> {
    let object = payload.as_object_mut().ok_or_else(|| {
        LLMProxyError::upstream_error(
            context.request_id,
            Some(context.provider_id.to_string()),
            "upstream JSON response must be an object",
        )
    })?;

    object.insert(
        "pokrov".to_string(),
        serde_json::to_value(LLMResponseMetadata {
            profile: context.profile_id.to_string(),
            sanitized_input: context.sanitized_input,
            sanitized_output: context.sanitized_output,
            action: context.final_action,
            rule_hits: context.total_hits,
            estimated_token_units: context.estimated_token_units,
            observed_token_units: None,
            provider: Some(context.provider_id.to_string()),
        })
        .map_err(|error| {
            LLMProxyError::upstream_error(
                context.request_id,
                Some(context.provider_id.to_string()),
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

    object.insert("request_id".to_string(), Value::String(request_id.to_string()));
    Ok(())
}

pub(super) fn max_action(left: PolicyAction, right: PolicyAction) -> PolicyAction {
    if right.strictness_rank() > left.strictness_rank() {
        right
    } else {
        left
    }
}
