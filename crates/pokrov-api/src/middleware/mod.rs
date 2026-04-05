use axum::{
    body::Body,
    extract::State,
    http::{header::HeaderName, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::time::Instant;
use uuid::Uuid;

use crate::app::{
    AppState, ClientIdentity, GatewayAuthContext, IdentityEvidence, RuntimeStateView,
    VerifiedClientCertIdentity,
};
use crate::auth::{
    fingerprint_gateway_auth_subject, parse_gateway_credential, parse_header_token_by_name,
    parse_identity_from_headers, parse_spiffe_identity_from_mesh_header,
    resolve_identity_from_sources,
};
use crate::middleware::request_id::normalize_or_generate_request_id;

pub mod rate_limit;
pub mod request_id;

const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub async fn request_id_middleware(
    _state: State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = normalize_or_generate_request_id(
        request.headers().get(&X_REQUEST_ID).and_then(|value| value.to_str().ok()),
    );

    request.extensions_mut().insert(request_id.clone());
    let mut response = next.run(request).await;

    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(X_REQUEST_ID, header_value);
    }

    response
}

pub async fn active_requests_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id =
        request.extensions().get::<String>().cloned().unwrap_or_else(|| Uuid::new_v4().to_string());
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let auth_mode = match state.auth.upstream_auth_mode {
        pokrov_config::UpstreamAuthMode::Static => "static",
        pokrov_config::UpstreamAuthMode::Passthrough => "passthrough",
    };
    let verified_client_cert = request.extensions().get::<VerifiedClientCertIdentity>();
    let gateway_auth =
        resolve_gateway_auth_context(&state, request.headers(), verified_client_cert);
    let client_identity = resolve_client_identity(&state, request.headers(), &gateway_auth);
    let policy_profile = resolve_policy_profile(
        &state,
        request.headers(),
        client_identity.as_ref(),
        gateway_auth.authenticated,
    );
    request.extensions_mut().insert(gateway_auth.clone());
    if let Some(identity) = client_identity.clone() {
        request.extensions_mut().insert(identity);
    }
    let runtime_state = state.lifecycle.state();

    if matches!(runtime_state, RuntimeStateView::Draining | RuntimeStateView::Stopped)
        && path != "/ready"
    {
        tracing::info!(
            component = "runtime",
            action = "request_rejected",
            request_id = %request_id,
            method = %method,
            path = %path,
            reason = "runtime_draining"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    state.lifecycle.on_request_started();
    state.metrics.on_request_started();
    let started = Instant::now();
    tracing::info!(
        component = "runtime",
        action = "auth_stage",
        request_id = %request_id,
        stage = "gateway_auth",
        decision = if gateway_auth.authenticated { "pass" } else { "fail" },
        auth_mode = auth_mode
    );
    tracing::info!(
        component = "runtime",
        action = "request_started",
        request_id = %request_id,
        method = %method,
        path = %path,
        auth_mode = auth_mode,
        client_identity = ?client_identity.as_ref().map(|identity| identity.id.as_str()),
        gateway_auth = gateway_auth.authenticated
    );

    let response = next.run(request).await;
    let status_code = response.status().as_u16();
    let decision = if (200..300).contains(&status_code) {
        "allowed"
    } else if status_code == 429 || status_code == 403 {
        "blocked"
    } else {
        "errored"
    };
    let route = normalize_route(&path);
    let path_class = classify_path(&path);

    state.lifecycle.on_request_finished();
    state.metrics.on_request_finished();
    state.metrics.on_request_outcome(route, path_class, status_code, decision);
    state.metrics.on_request_duration_seconds(
        route,
        path_class,
        decision,
        started.elapsed().as_secs_f64(),
    );
    if status_code == 429 {
        state.metrics.on_blocked_request(
            route,
            "rate_limit",
            policy_profile.as_deref().unwrap_or("custom"),
        );
    }
    if status_code == 403 {
        state.metrics.on_blocked_request(
            route,
            "policy",
            policy_profile.as_deref().unwrap_or("custom"),
        );
    }
    tracing::info!(
        component = "runtime",
        action = "request_finished",
        request_id = %request_id,
        method = %method,
        path = %path,
        status_code
    );
    response
}

fn resolve_policy_profile(
    state: &AppState,
    headers: &HeaderMap,
    client_identity: Option<&ClientIdentity>,
    gateway_authenticated: bool,
) -> Option<String> {
    if let Some(identity) = client_identity {
        if let Some(profile) = state.auth.identity_profile_bindings.get(&identity.id) {
            return Some(profile.clone());
        }
    }

    if state.auth.required_for_policy && !gateway_authenticated {
        return state.auth.fallback_policy_profile.clone();
    }

    if matches!(state.auth.gateway_auth_mode, pokrov_config::GatewayAuthMode::ApiKey) {
        if let Some(gateway) = parse_gateway_credential(headers) {
            if let Some(profile) = state.sanitization.profile_for_token(gateway.token) {
                return Some(profile);
            }
        }
    }

    state.auth.fallback_policy_profile.clone()
}

fn resolve_gateway_auth_context(
    state: &AppState,
    headers: &HeaderMap,
    verified_client_cert: Option<&VerifiedClientCertIdentity>,
) -> GatewayAuthContext {
    match state.auth.gateway_auth_mode {
        pokrov_config::GatewayAuthMode::ApiKey => {
            let Some(credential) = parse_gateway_credential(headers) else {
                return GatewayAuthContext {
                    authenticated: false,
                    auth_subject: None,
                    auth_mechanism: None,
                    failure_reason: Some("missing_gateway_auth"),
                };
            };

            if state.sanitization.profile_for_token(credential.token).is_some() {
                GatewayAuthContext {
                    authenticated: true,
                    auth_subject: Some(fingerprint_gateway_auth_subject(credential.token)),
                    auth_mechanism: Some(credential.mechanism),
                    failure_reason: None,
                }
            } else {
                GatewayAuthContext {
                    authenticated: false,
                    auth_subject: None,
                    auth_mechanism: Some(credential.mechanism),
                    failure_reason: Some("invalid_gateway_auth"),
                }
            }
        }
        pokrov_config::GatewayAuthMode::InternalMtls => resolve_internal_mtls_auth_context(
            headers,
            state.auth.internal_mtls_identity_header.as_str(),
            verified_client_cert,
        ),
        pokrov_config::GatewayAuthMode::MeshMtls => {
            let raw = parse_header_token_by_name(headers, &state.auth.mesh_identity_header);
            let subject = raw.and_then(parse_spiffe_identity_from_mesh_header);
            if let Some(identity) = subject {
                if validate_mesh_spiffe_identity(
                    identity,
                    state.auth.mesh_required_spiffe_trust_domain.as_deref(),
                ) {
                    GatewayAuthContext {
                        authenticated: true,
                        auth_subject: Some(identity.to_string()),
                        auth_mechanism: Some(crate::app::GatewayAuthMechanism::MeshMtls),
                        failure_reason: None,
                    }
                } else {
                    GatewayAuthContext {
                        authenticated: false,
                        auth_subject: None,
                        auth_mechanism: Some(crate::app::GatewayAuthMechanism::MeshMtls),
                        failure_reason: Some("invalid_gateway_auth"),
                    }
                }
            } else if !state.auth.mesh_require_header {
                if matches!(
                    state.auth.upstream_auth_mode,
                    pokrov_config::UpstreamAuthMode::Passthrough
                ) {
                    return GatewayAuthContext {
                        authenticated: false,
                        auth_subject: None,
                        auth_mechanism: Some(crate::app::GatewayAuthMechanism::MeshMtls),
                        failure_reason: Some("missing_gateway_auth"),
                    };
                }
                GatewayAuthContext {
                    authenticated: true,
                    auth_subject: Some("mesh_mtls_authenticated".to_string()),
                    auth_mechanism: Some(crate::app::GatewayAuthMechanism::MeshMtls),
                    failure_reason: None,
                }
            } else {
                GatewayAuthContext {
                    authenticated: false,
                    auth_subject: None,
                    auth_mechanism: Some(crate::app::GatewayAuthMechanism::MeshMtls),
                    failure_reason: Some("missing_gateway_auth"),
                }
            }
        }
    }
}

fn validate_mesh_spiffe_identity(identity: &str, required_trust_domain: Option<&str>) -> bool {
    let Some(uri) = identity.strip_prefix("spiffe://") else {
        return false;
    };
    let (trust_domain, path) = uri.split_once('/').unwrap_or((uri, ""));
    if trust_domain.trim().is_empty() {
        return false;
    }
    if let Some(required) = required_trust_domain {
        if trust_domain != required.trim() {
            return false;
        }
    }

    // Reject path traversal segments in SPIFFE IDs before resolving mesh identity.
    for segment in path.split('/') {
        if segment == "." || segment == ".." {
            return false;
        }
    }

    true
}

fn resolve_internal_mtls_auth_context(
    headers: &HeaderMap,
    identity_header_name: &str,
    verified_client_cert: Option<&VerifiedClientCertIdentity>,
) -> GatewayAuthContext {
    if let Some(identity) = verified_client_cert {
        return GatewayAuthContext {
            authenticated: true,
            auth_subject: Some(identity.subject.clone()),
            auth_mechanism: Some(crate::app::GatewayAuthMechanism::InternalMtls),
            failure_reason: None,
        };
    }

    let header_present = parse_header_token_by_name(headers, identity_header_name).is_some();
    GatewayAuthContext {
        authenticated: false,
        auth_subject: None,
        auth_mechanism: Some(crate::app::GatewayAuthMechanism::InternalMtls),
        failure_reason: Some(if header_present {
            "invalid_gateway_auth"
        } else {
            "missing_gateway_auth"
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::app::VerifiedClientCertIdentity;
    use axum::http::{HeaderMap, HeaderValue};

    use super::{resolve_internal_mtls_auth_context, validate_mesh_spiffe_identity};

    #[test]
    fn internal_mtls_rejects_spoofed_header_without_verified_identity() {
        let mut headers = HeaderMap::new();
        headers.insert("x-pokrov-client-cert-subject", HeaderValue::from_static("CN=spoofed"));

        let auth =
            resolve_internal_mtls_auth_context(&headers, "x-pokrov-client-cert-subject", None);
        assert!(!auth.authenticated);
        assert_eq!(auth.failure_reason, Some("invalid_gateway_auth"));
    }

    #[test]
    fn internal_mtls_accepts_verified_transport_identity() {
        let headers = HeaderMap::new();
        let verified = VerifiedClientCertIdentity { subject: "CN=runtime-verified".to_string() };

        let auth = resolve_internal_mtls_auth_context(
            &headers,
            "x-pokrov-client-cert-subject",
            Some(&verified),
        );
        assert!(auth.authenticated);
        assert_eq!(auth.auth_subject.as_deref(), Some("CN=runtime-verified"));
        assert_eq!(auth.failure_reason, None);
    }

    #[test]
    fn mesh_spiffe_validation_requires_exact_trust_domain_boundary() {
        assert!(validate_mesh_spiffe_identity(
            "spiffe://cluster.local/ns/default/sa/app",
            Some("cluster.local"),
        ));
        assert!(!validate_mesh_spiffe_identity(
            "spiffe://cluster.local.evil/ns/default/sa/app",
            Some("cluster.local"),
        ));
    }

    #[test]
    fn mesh_spiffe_validation_rejects_path_traversal_segments() {
        assert!(!validate_mesh_spiffe_identity(
            "spiffe://cluster.local/ns/default/../sa/app",
            Some("cluster.local"),
        ));
    }
}

fn resolve_client_identity(
    state: &AppState,
    headers: &HeaderMap,
    gateway_auth: &GatewayAuthContext,
) -> Option<ClientIdentity> {
    let (header_identity, ingress_identity) = parse_identity_from_headers(headers);
    let identity = resolve_identity_from_sources(
        state.auth.identity_resolution_order.as_slice(),
        header_identity,
        ingress_identity,
        gateway_auth.auth_subject.as_deref(),
    )?;
    let source = if Some(identity) == gateway_auth.auth_subject.as_deref() {
        IdentityEvidence::GatewayAuth
    } else if Some(identity) == header_identity {
        IdentityEvidence::Header
    } else {
        IdentityEvidence::IngressContext
    };

    Some(ClientIdentity {
        id: identity.to_string(),
        source,
        profile_hint: state.auth.identity_profile_bindings.get(identity).cloned(),
    })
}

fn normalize_route(path: &str) -> &'static str {
    if path.starts_with("/v1/mcp/tools/") && path.ends_with("/invoke") {
        return "/v1/mcp/tools/{toolName}/invoke";
    }

    match path {
        "/health" => "/health",
        "/ready" => "/ready",
        "/metrics" => "/metrics",
        "/v1/sanitize/evaluate" => "/v1/sanitize/evaluate",
        "/v1/chat/completions" => "/v1/chat/completions",
        "/v1/models" => "/v1/models",
        "/v1/responses" => "/v1/responses",
        "/v1/mcp/tool-call" => "/v1/mcp/tool-call",
        _ => "other",
    }
}

fn classify_path(path: &str) -> &'static str {
    match path {
        "/health" | "/ready" | "/metrics" => "runtime",
        "/v1/sanitize/evaluate" => "sanitization",
        "/v1/chat/completions" | "/v1/models" | "/v1/responses" => "llm",
        _ if path.starts_with("/v1/mcp") => "mcp",
        _ => "runtime",
    }
}
