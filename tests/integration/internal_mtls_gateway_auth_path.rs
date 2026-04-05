use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, SanType};
use reqwest::StatusCode;
use tempfile::NamedTempFile;

use crate::llm_proxy_test_support::{
    start_mock_provider, write_key_file, write_runtime_config, MockProviderMode,
};

#[tokio::test]
async fn internal_mtls_gateway_allows_request_after_real_tls_handshake_and_client_cert_validation()
{
    let provider = start_mock_provider(MockProviderMode::Json {
        status: 200,
        body: serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop"
            }]
        }),
    })
    .await;

    let generated = generate_test_certificates();
    let server_cert_path = write_temp_file(&generated.server_cert_pem);
    let server_key_path = write_temp_file(&generated.server_key_pem);
    let client_ca_path = write_temp_file(&generated.ca_cert_pem);
    let provider_key_path = write_key_file("provider-static-fallback");
    let config_path = write_runtime_config(&format!(
        r#"
server:
  host: 127.0.0.1
  port: 0
  tls:
    enabled: true
    cert_file: {server_cert}
    key_file: {server_key}
    client_ca_file: {client_ca}
    require_client_cert: true
logging:
  level: info
  format: json
shutdown:
  drain_timeout_ms: 300
  grace_period_ms: 900
security:
  api_keys: []
auth:
  upstream_auth_mode: passthrough
  gateway_auth_mode: internal_mtls
  internal_mtls:
    identity_header: x-pokrov-client-cert-subject
    require_header: true
identity:
  resolution_order:
    - gateway_auth_subject
sanitization:
  enabled: false
llm:
  providers:
    - id: openai
      base_url: {provider_base}
      auth:
        api_key: file:{provider_key}
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: false
"#,
        server_cert = server_cert_path.display(),
        server_key = server_key_path.display(),
        client_ca = client_ca_path.display(),
        provider_key = provider_key_path.display(),
        provider_base = provider.base_url,
    ));

    let handle = pokrov_runtime::bootstrap::spawn_runtime_for_tests(config_path)
        .await
        .expect("runtime should start");
    let client_identity_pem =
        format!("{}\n{}", generated.client_cert_pem, generated.client_key_pem);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .add_root_certificate(
            reqwest::Certificate::from_pem(generated.ca_cert_pem.as_bytes())
                .expect("ca certificate must parse"),
        )
        .identity(
            reqwest::Identity::from_pem(client_identity_pem.as_bytes())
                .expect("client identity must parse"),
        )
        .build()
        .expect("client should build");

    let response = client
        .post(format!(
            "{}/v1/chat/completions",
            handle.base_url().replacen("http://", "https://", 1)
        ))
        .header("authorization", "Bearer provider-byok-key")
        .json(&serde_json::json!({
            "model": "gpt-4o-mini",
            "stream": false,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("request should complete");

    let status = response.status();
    let body_text = response.text().await.expect("response body should read");
    assert_eq!(status, StatusCode::OK, "unexpected body: {body_text}");
    let forwarded_auth = provider.captured_authorization_headers().await;
    assert_eq!(forwarded_auth[0].as_deref(), Some("Bearer provider-byok-key"));

    drop(client);
    handle.shutdown().await.expect("shutdown should succeed");
    provider.shutdown().await;
}

struct GeneratedCertificates {
    ca_cert_pem: String,
    server_cert_pem: String,
    server_key_pem: String,
    client_cert_pem: String,
    client_key_pem: String,
}

fn generate_test_certificates() -> GeneratedCertificates {
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name.push(DnType::CommonName, "pokrov-test-ca");
    let ca_key = KeyPair::generate().expect("ca key should generate");
    let ca_cert = ca_params.self_signed(&ca_key).expect("ca certificate should generate");

    let mut server_params = CertificateParams::new(vec!["localhost".to_string()])
        .expect("server certificate params should build");
    server_params.subject_alt_names.push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    server_params.distinguished_name.push(DnType::CommonName, "localhost");
    let server_key = KeyPair::generate().expect("server key should generate");
    let server_cert = server_params
        .signed_by(&server_key, &ca_cert, &ca_key)
        .expect("server certificate should sign");

    let mut client_params = CertificateParams::new(vec![]).expect("client params should build");
    client_params.distinguished_name.push(DnType::CommonName, "pokrov-test-client");
    let client_key = KeyPair::generate().expect("client key should generate");
    let client_cert = client_params
        .signed_by(&client_key, &ca_cert, &ca_key)
        .expect("client certificate should sign");

    GeneratedCertificates {
        ca_cert_pem: ca_cert.pem(),
        server_cert_pem: server_cert.pem(),
        server_key_pem: server_key.serialize_pem(),
        client_cert_pem: client_cert.pem(),
        client_key_pem: client_key.serialize_pem(),
    }
}

fn write_temp_file(content: &str) -> std::path::PathBuf {
    let mut file = NamedTempFile::new().expect("temp file should be created");
    file.write_all(content.as_bytes()).expect("temp file should be written");
    file.into_temp_path().keep().expect("temp file path should persist")
}
