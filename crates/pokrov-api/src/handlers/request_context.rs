use axum::http::HeaderMap;

use crate::{
    app::{AppState, GatewayAuthContext, GatewayAuthMechanism},
    auth::{
        fingerprint_gateway_auth_subject, parse_bearer_token, parse_gateway_credential,
        parse_identity_from_headers, resolve_identity_from_sources,
    },
    error::ApiError,
};

/// Resolved auth and identity context reused by request handlers that share gateway and passthrough flow.
pub(super) struct ResolvedRequestContext {
    pub(super) mode_label: &'static str,
    pub(super) rate_limit_key: String,
    pub(super) profile_id: String,
    pub(super) rate_limit_profile: String,
    pub(super) upstream_credential: Option<String>,
}

pub(super) struct RequestContextHooks {
    pub(super) on_auth_stage: fn(&AppState, &'static str, &'static str, &'static str),
    pub(super) emit_auth_stage: fn(&str, &'static str, &'static str, &'static str, &'static str),
    pub(super) map_error: Option<fn(ApiError) -> ApiError>,
}

pub(super) enum UpstreamCredentialRequirement {
    Required,
    Optional,
}

pub(super) fn passthrough_error(error: ApiError) -> ApiError {
    error
}

/// Resolves gateway auth, identity profile, rate-limit profile, and upstream credential in one place.
#[allow(clippy::result_large_err)]
pub(super) fn resolve_request_context(
    state: &AppState,
    headers: &HeaderMap,
    gateway_auth: &GatewayAuthContext,
    request_id: &str,
    endpoint: &'static str,
    upstream_credential_requirement: UpstreamCredentialRequirement,
    hooks: &RequestContextHooks,
) -> Result<ResolvedRequestContext, ApiError> {
    let mode_label = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => "static",
        pokrov_config::UpstreamAuthMode::Passthrough => "passthrough",
    };
    let map_error = hooks.map_error.unwrap_or(passthrough_error);

    if !gateway_auth.authenticated {
        (hooks.on_auth_stage)(state, mode_label, "gateway_auth", "fail");
        (hooks.emit_auth_stage)(request_id, endpoint, mode_label, "gateway_auth", "fail");
        return Err(map_error(ApiError::gateway_unauthorized(request_id.to_string())));
    }

    (hooks.on_auth_stage)(state, mode_label, "gateway_auth", "pass");
    (hooks.emit_auth_stage)(request_id, endpoint, mode_label, "gateway_auth", "pass");

    let gateway_credential = parse_gateway_credential(headers);
    let (header_identity, ingress_identity) = parse_identity_from_headers(headers);
    let gateway_auth_subject = gateway_auth.auth_subject.clone().unwrap_or_else(|| {
        gateway_credential
            .as_ref()
            .map(|credential| fingerprint_gateway_auth_subject(credential.token))
            .unwrap_or_else(|| "gateway_authenticated".to_string())
    });
    let client_identity = resolve_identity_from_sources(
        state.auth.identity_resolution_order.as_slice(),
        header_identity,
        ingress_identity,
        Some(gateway_auth_subject.as_str()),
    )
    .unwrap_or(gateway_auth_subject.as_str());

    let gateway_profile = gateway_credential
        .as_ref()
        .and_then(|gateway| state.sanitization.profile_for_token(gateway.token));
    let profile_id = state
        .auth
        .identity_profile_bindings
        .get(client_identity)
        .cloned()
        .or_else(|| gateway_profile.clone())
        .or_else(|| state.auth.fallback_policy_profile.clone())
        .unwrap_or_else(|| "strict".to_string());
    let rate_limit_profile = state
        .auth
        .identity_rate_limit_bindings
        .get(client_identity)
        .cloned()
        .or(gateway_profile)
        .unwrap_or_else(|| profile_id.clone());

    let upstream_credential = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => None,
        pokrov_config::UpstreamAuthMode::Passthrough => {
            // Local-only endpoints (for example, model catalog discovery) must not require provider credentials.
            if matches!(upstream_credential_requirement, UpstreamCredentialRequirement::Optional) {
                return Ok(ResolvedRequestContext {
                    mode_label,
                    rate_limit_key: client_identity.to_string(),
                    profile_id,
                    rate_limit_profile,
                    upstream_credential: None,
                });
            }

            let missing_upstream = || {
                (hooks.on_auth_stage)(state, mode_label, "upstream_credentials", "fail");
                (hooks.emit_auth_stage)(
                    request_id,
                    endpoint,
                    mode_label,
                    "upstream_credentials",
                    "fail",
                );
                map_error(ApiError::upstream_credential_missing(request_id.to_string()))
            };

            let effective_gateway_mechanism = gateway_auth
                .auth_mechanism
                .or(gateway_credential.as_ref().map(|credential| credential.mechanism));
            let credential = match effective_gateway_mechanism {
                Some(GatewayAuthMechanism::ApiKey)
                | Some(GatewayAuthMechanism::InternalMtls)
                | Some(GatewayAuthMechanism::MeshMtls)
                | None => {
                    parse_bearer_token(headers).map(str::to_string).ok_or_else(missing_upstream)?
                }
                Some(GatewayAuthMechanism::Bearer) => {
                    if !state.auth.allow_single_bearer_passthrough {
                        (hooks.on_auth_stage)(state, mode_label, "upstream_credentials", "fail");
                        (hooks.emit_auth_stage)(
                            request_id,
                            endpoint,
                            mode_label,
                            "upstream_credentials",
                            "fail",
                        );
                        return Err(map_error(
                            ApiError::passthrough_requires_api_key_gateway_auth(
                                request_id.to_string(),
                            ),
                        ));
                    }

                    // Compatibility mode allows single-bearer flow when explicitly enabled.
                    gateway_credential
                        .map(|credential| credential.token.to_string())
                        .ok_or_else(missing_upstream)?
                }
            };
            (hooks.on_auth_stage)(state, mode_label, "upstream_credentials", "pass");
            (hooks.emit_auth_stage)(
                request_id,
                endpoint,
                mode_label,
                "upstream_credentials",
                "pass",
            );
            Some(credential)
        }
    };

    Ok(ResolvedRequestContext {
        mode_label,
        rate_limit_key: client_identity.to_string(),
        profile_id,
        rate_limit_profile,
        upstream_credential,
    })
}
