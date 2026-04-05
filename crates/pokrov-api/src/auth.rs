use axum::http::{header, HeaderMap, HeaderName};
use pokrov_config::IdentitySource;
use sha2::{Digest, Sha256};

use crate::app::GatewayAuthMechanism;

const X_POKROV_API_KEY: HeaderName = HeaderName::from_static("x-pokrov-api-key");
const X_POKROV_CLIENT_ID: HeaderName = HeaderName::from_static("x-pokrov-client-id");
const X_INGRESS_IDENTITY: HeaderName = HeaderName::from_static("x-ingress-identity");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GatewayCredential<'a> {
    pub(crate) token: &'a str,
    pub(crate) mechanism: GatewayAuthMechanism,
}

pub(crate) fn parse_gateway_credential(headers: &HeaderMap) -> Option<GatewayCredential<'_>> {
    if let Some(token) = parse_header_token(headers, &X_POKROV_API_KEY) {
        return Some(GatewayCredential { token, mechanism: GatewayAuthMechanism::ApiKey });
    }

    parse_bearer_token(headers)
        .map(|token| GatewayCredential { token, mechanism: GatewayAuthMechanism::Bearer })
}

pub(crate) fn parse_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    header.strip_prefix("Bearer ").map(str::trim).filter(|token| !token.is_empty())
}

pub(crate) fn parse_header_token_by_name<'a>(
    headers: &'a HeaderMap,
    header_name: &str,
) -> Option<&'a str> {
    let name = HeaderName::from_bytes(header_name.as_bytes()).ok()?;
    parse_header_token(headers, &name)
}

pub(crate) fn parse_spiffe_identity_from_mesh_header(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.starts_with("spiffe://") {
        return Some(trimmed);
    }

    // Istio XFCC usually contains key-value pairs and may include URI=spiffe://...
    for segment in trimmed.split(';') {
        let part = segment.trim();
        if let Some(value) = part.strip_prefix("URI=") {
            let uri = value.trim().trim_matches('"');
            if uri.starts_with("spiffe://") {
                return Some(uri);
            }
        }
    }

    None
}

pub(crate) fn parse_identity_from_headers(headers: &HeaderMap) -> (Option<&str>, Option<&str>) {
    (
        parse_header_token(headers, &X_POKROV_CLIENT_ID),
        parse_header_token(headers, &X_INGRESS_IDENTITY),
    )
}

pub(crate) fn resolve_identity_from_sources<'a>(
    resolution_order: &[IdentitySource],
    header_identity: Option<&'a str>,
    ingress_identity: Option<&'a str>,
    gateway_auth_subject: Option<&'a str>,
) -> Option<&'a str> {
    for source in resolution_order {
        match source {
            IdentitySource::GatewayAuthSubject => {
                if let Some(identity) = gateway_auth_subject {
                    return Some(identity);
                }
            }
            IdentitySource::XPokrovClientId => {
                if let Some(identity) = header_identity {
                    return Some(identity);
                }
            }
            IdentitySource::IngressIdentity => {
                if let Some(identity) = ingress_identity {
                    return Some(identity);
                }
            }
        }
    }

    None
}

pub(crate) fn fingerprint_gateway_auth_subject(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let truncated = &digest[..16];
    let mut value = String::with_capacity(35);
    value.push_str("gw_");
    for byte in truncated {
        use std::fmt::Write as _;
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}

fn parse_header_token<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim).filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use axum::http::{header, HeaderMap, HeaderValue};

    use crate::{app::GatewayAuthMechanism, auth::parse_gateway_credential};

    use pokrov_config::IdentitySource;

    use super::{
        fingerprint_gateway_auth_subject, parse_bearer_token, parse_header_token_by_name,
        parse_identity_from_headers, parse_spiffe_identity_from_mesh_header,
        resolve_identity_from_sources,
    };

    #[test]
    fn parse_bearer_token_normalizes_prefix_and_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer   token-123   "));

        assert_eq!(parse_bearer_token(&headers), Some("token-123"));
    }

    #[test]
    fn parse_gateway_credential_prefers_x_pokrov_api_key_over_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert("x-pokrov-api-key", HeaderValue::from_static("gw-key"));
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer provider-key"));

        let credential = parse_gateway_credential(&headers).expect("credential should resolve");
        assert_eq!(credential.token, "gw-key");
        assert_eq!(credential.mechanism, GatewayAuthMechanism::ApiKey);
    }

    #[test]
    fn parse_header_token_by_name_reads_trimmed_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-custom-auth", HeaderValue::from_static(" mesh-subject "));

        assert_eq!(parse_header_token_by_name(&headers, "x-custom-auth"), Some("mesh-subject"));
    }

    #[test]
    fn parse_spiffe_identity_from_mesh_header_supports_plain_and_xfcc_forms() {
        assert_eq!(
            parse_spiffe_identity_from_mesh_header("spiffe://cluster.local/ns/default/sa/agent"),
            Some("spiffe://cluster.local/ns/default/sa/agent")
        );
        assert_eq!(
            parse_spiffe_identity_from_mesh_header(
                "By=spiffe://cluster.local/ns/istio-system/sa/ingress;URI=spiffe://cluster.local/ns/default/sa/app"
            ),
            Some("spiffe://cluster.local/ns/default/sa/app")
        );
    }

    #[test]
    fn parse_identity_from_headers_returns_trimmed_values() {
        let mut headers = HeaderMap::new();
        headers.insert("x-pokrov-client-id", HeaderValue::from_static(" tenant-a "));
        headers.insert("x-ingress-identity", HeaderValue::from_static(" ingress-1 "));

        let (client_id, ingress_id) = parse_identity_from_headers(&headers);
        assert_eq!(client_id, Some("tenant-a"));
        assert_eq!(ingress_id, Some("ingress-1"));
    }

    #[test]
    fn resolve_identity_from_sources_uses_configured_order() {
        let selected = resolve_identity_from_sources(
            &[IdentitySource::IngressIdentity, IdentitySource::XPokrovClientId],
            Some("tenant-a"),
            Some("ingress-a"),
            Some("gw_subject"),
        );

        assert_eq!(selected, Some("ingress-a"));
    }

    #[test]
    fn resolve_identity_from_sources_supports_gateway_auth_subject() {
        let selected = resolve_identity_from_sources(
            &[IdentitySource::GatewayAuthSubject, IdentitySource::XPokrovClientId],
            Some("tenant-a"),
            Some("ingress-a"),
            Some("gw_subject"),
        );

        assert_eq!(selected, Some("gw_subject"));
    }

    #[test]
    fn fingerprint_gateway_auth_subject_returns_stable_identifier() {
        let first = fingerprint_gateway_auth_subject("gateway-key");
        let second = fingerprint_gateway_auth_subject("gateway-key");

        assert_eq!(first, second);
        assert!(first.starts_with("gw_"));
        assert_eq!(first.len(), 35);
        assert_eq!(first, "gw_8cb1c6bc8952bb3def9c7ff05b13fafc");
    }
}
