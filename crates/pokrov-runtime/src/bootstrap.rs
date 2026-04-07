use std::{
    convert::Infallible, error::Error, fmt, future::IntoFuture, io::BufReader, net::SocketAddr,
    path::PathBuf, sync::Arc, time::Duration,
};

use axum::http::Request;
use axum::response::Response;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use pokrov_api::app::{
    build_router, AppState, AuthState, LlmProxyState, McpProxyState, RateLimitState,
    ResolvedApiKeyBinding, SanitizationState, VerifiedClientCertIdentity,
};
use pokrov_api::middleware::rate_limit::RateLimiter;
use pokrov_config::{
    error::ConfigError,
    loader::load_runtime_config,
    model::{LlmConfig, RuntimeConfig, SecretRef},
};
use pokrov_core::{types::EvaluateError, SanitizationEngine};
use pokrov_metrics::{
    hooks::{LifecycleEvent, RuntimeMetricsHooks},
    registry::RuntimeMetricsRegistry,
};
use pokrov_proxy_llm::{handler::LLMProxyHandler, routing::ProviderRouteTable};
use pokrov_proxy_mcp::handler::McpProxyHandler;
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    server::WebPkiClientVerifier,
    RootCertStore, ServerConfig,
};
use serde::Serialize;
use time::OffsetDateTime;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{oneshot, watch},
    task::{JoinHandle, JoinSet},
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tower::ServiceExt;
use tracing::{error, info, warn};
use x509_parser::prelude::parse_x509_certificate;

use crate::{
    lifecycle::{RuntimeLifecycle, SharedRuntimeLifecycle},
    observability::{init_json_observability, log_lifecycle_event},
    release_evidence::{
        collect_artifact_checksums, write_release_evidence, ArtifactChecksum, GateStatus,
        OperationalEvidence, PerformanceEvidence, ReleaseEvidence, SecurityEvidence,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapArgs {
    pub config_path: Option<PathBuf>,
    pub release_evidence_output: Option<PathBuf>,
    pub release_id: Option<String>,
    pub evidence_artifacts: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum BootstrapError {
    InvalidArguments(String),
    Config(ConfigError),
    Sanitization(EvaluateError),
    LlmProxy(String),
    McpProxy(String),
    Security(String),
    EvidenceIo(std::io::Error),
    Bind(std::io::Error),
    Serve(std::io::Error),
    Join(tokio::task::JoinError),
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArguments(message) => write!(f, "invalid arguments: {message}"),
            Self::Config(error) => write!(f, "{error}"),
            Self::Sanitization(error) => {
                write!(f, "failed to initialize sanitization engine: {error}")
            }
            Self::LlmProxy(message) => write!(f, "failed to initialize llm proxy: {message}"),
            Self::McpProxy(message) => write!(f, "failed to initialize mcp proxy: {message}"),
            Self::Security(message) => write!(f, "security bootstrap failed: {message}"),
            Self::EvidenceIo(error) => write!(f, "failed to write release evidence: {error}"),
            Self::Bind(error) => write!(f, "failed to bind listener: {error}"),
            Self::Serve(error) => write!(f, "runtime server failed: {error}"),
            Self::Join(error) => write!(f, "runtime task failed: {error}"),
        }
    }
}

impl Error for BootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Sanitization(error) => Some(error),
            Self::EvidenceIo(error) => Some(error),
            Self::Bind(error) => Some(error),
            Self::Serve(error) => Some(error),
            Self::Join(error) => Some(error),
            Self::InvalidArguments(_)
            | Self::LlmProxy(_)
            | Self::McpProxy(_)
            | Self::Security(_) => None,
        }
    }
}

pub fn parse_args(args: &[String]) -> Result<BootstrapArgs, BootstrapError> {
    let mut iter = args.iter();
    let mut config_path = None;
    let mut release_evidence_output = None;
    let mut release_id = None;
    let mut evidence_artifacts = Vec::new();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => {
                let path = iter.next().ok_or_else(|| {
                    BootstrapError::InvalidArguments("expected --config <path>".to_string())
                })?;
                if config_path.is_some() {
                    return Err(BootstrapError::InvalidArguments(
                        "--config must be provided only once".to_string(),
                    ));
                }
                config_path = Some(PathBuf::from(path));
            }
            "--release-evidence-output" => {
                let path = iter.next().ok_or_else(|| {
                    BootstrapError::InvalidArguments(
                        "expected --release-evidence-output <path>".to_string(),
                    )
                })?;
                release_evidence_output = Some(PathBuf::from(path));
            }
            "--release-id" => {
                let value = iter.next().ok_or_else(|| {
                    BootstrapError::InvalidArguments("expected --release-id <value>".to_string())
                })?;
                release_id = Some(value.to_string());
            }
            "--artifact" => {
                let path = iter.next().ok_or_else(|| {
                    BootstrapError::InvalidArguments("expected --artifact <path>".to_string())
                })?;
                evidence_artifacts.push(PathBuf::from(path));
            }
            _ => {
                return Err(BootstrapError::InvalidArguments(format!("unknown argument: {arg}")));
            }
        }
    }

    if config_path.is_none() && release_evidence_output.is_none() {
        return Err(BootstrapError::InvalidArguments(
            "expected --config <path> or --release-evidence-output <path>".to_string(),
        ));
    }

    Ok(BootstrapArgs { config_path, release_evidence_output, release_id, evidence_artifacts })
}

pub async fn run(args: BootstrapArgs) -> Result<(), BootstrapError> {
    if args.release_evidence_output.is_some() {
        return generate_release_evidence(args);
    }

    let config_path = args
        .config_path
        .as_ref()
        .ok_or_else(|| BootstrapError::InvalidArguments("expected --config <path>".to_string()))?;
    let config = load_runtime_config(config_path).map_err(BootstrapError::Config)?;
    validate_llm_payload_trace_mode(&config)?;
    init_json_observability(config.logging.level.as_str());

    let listener = bind_listener(&config).await?;
    let addr = listener.local_addr().map_err(BootstrapError::Bind)?;
    info!(action = "startup", addr = %addr, "runtime listener bound");

    let lifecycle = Arc::new(RuntimeLifecycle::new());
    let metrics = Arc::new(RuntimeMetricsRegistry::default());

    run_with_listener(config, listener, lifecycle, metrics, async {
        wait_for_shutdown_signal().await
    })
    .await
}

fn generate_release_evidence(args: BootstrapArgs) -> Result<(), BootstrapError> {
    let output_path = args.release_evidence_output.as_ref().ok_or_else(|| {
        BootstrapError::InvalidArguments("expected --release-evidence-output <path>".to_string())
    })?;
    let release_id = args
        .release_id
        .unwrap_or_else(|| format!("release-{}", OffsetDateTime::now_utc().unix_timestamp()));
    let git_commit = resolve_git_commit();
    let mut artifacts =
        collect_artifact_checksums(&args.evidence_artifacts).map_err(BootstrapError::EvidenceIo)?;
    if artifacts.is_empty() {
        artifacts
            .push(ArtifactChecksum { path: "placeholder".to_string(), sha256: "0".repeat(64) });
    }

    let evidence = ReleaseEvidence::build(
        release_id,
        git_commit,
        "manual".to_string(),
        PerformanceEvidence {
            runs: 3,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            throughput_rps: 0.0,
            startup_seconds: 0.0,
            pass: false,
        },
        SecurityEvidence {
            invalid_auth: GateStatus::Fail,
            rate_limit_abuse: GateStatus::Fail,
            log_safety: GateStatus::Fail,
            secret_handling: GateStatus::Fail,
            pass: false,
        },
        OperationalEvidence {
            metrics_coverage_percent: 0,
            readiness_behavior: GateStatus::Fail,
            graceful_shutdown_behavior: GateStatus::Fail,
            observability_behavior: GateStatus::Fail,
            pass: false,
        },
        artifacts,
        vec![
            "Evidence file was generated before verification steps were provided".to_string(),
            "Populate performance/security/operational sections from validated test outputs"
                .to_string(),
        ],
    );

    write_release_evidence(output_path, &evidence).map_err(BootstrapError::EvidenceIo)
}

pub struct RuntimeHandle {
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), BootstrapError>>,
}

impl RuntimeHandle {
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn shutdown(mut self) -> Result<(), BootstrapError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        self.task.await.map_err(BootstrapError::Join)?
    }
}

pub async fn spawn_runtime_for_tests(
    config_path: PathBuf,
) -> Result<RuntimeHandle, BootstrapError> {
    let config = load_runtime_config(&config_path).map_err(BootstrapError::Config)?;
    let listener = bind_listener(&config).await?;
    let addr = listener.local_addr().map_err(BootstrapError::Bind)?;

    let lifecycle = Arc::new(RuntimeLifecycle::new());
    let readiness_lifecycle = lifecycle.clone();
    let metrics = Arc::new(RuntimeMetricsRegistry::default());
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        run_with_listener(config, listener, lifecycle, metrics, async {
            let _ = shutdown_rx.await;
        })
        .await
    });

    wait_until_runtime_started(readiness_lifecycle).await;

    Ok(RuntimeHandle { addr, shutdown_tx: Some(shutdown_tx), task })
}

async fn bind_listener(config: &RuntimeConfig) -> Result<TcpListener, BootstrapError> {
    let addr = format!("{}:{}", config.server.host, config.server.port);
    TcpListener::bind(addr).await.map_err(BootstrapError::Bind)
}

async fn run_with_listener<S>(
    config: RuntimeConfig,
    listener: TcpListener,
    lifecycle: SharedRuntimeLifecycle,
    metrics: Arc<RuntimeMetricsRegistry>,
    shutdown_signal: S,
) -> Result<(), BootstrapError>
where
    S: std::future::Future<Output = ()> + Send + 'static,
{
    metrics.on_lifecycle_event(LifecycleEvent::Starting);
    log_lifecycle_event("runtime", "lifecycle_transition", None, "starting");

    let evaluator = if config.sanitization.enabled {
        let engine = SanitizationEngine::new(config.evaluator_config())
            .map_err(BootstrapError::Sanitization)?;

        #[cfg(feature = "ner")]
        let engine = init_ner_engine(engine, &config)?;

        Some(Arc::new(engine))
    } else {
        None
    };

    let resolved_keys = resolve_api_key_bindings(&config)?;
    let policy_loaded = !config.sanitization.enabled || evaluator.is_some();
    let auth_loaded = !config.identity.resolution_order.is_empty();
    lifecycle.set_config_loaded(policy_loaded && auth_loaded).await;
    lifecycle.set_llm_routes_loaded(!is_llm_enabled(&config)).await;
    lifecycle.set_mcp_routes_loaded(!is_mcp_enabled(&config)).await;

    let llm_handler = build_llm_handler(&config, evaluator.clone(), metrics.clone())?;
    let llm_enabled = is_llm_enabled(&config);
    let llm_routes_loaded =
        llm_handler.as_ref().map(LLMProxyHandler::routes_loaded).unwrap_or(false);
    lifecycle.set_llm_routes_loaded(!llm_enabled || llm_routes_loaded).await;
    let mcp_handler = build_mcp_handler(&config, evaluator.clone(), metrics.clone())?;
    let mcp_enabled = is_mcp_enabled(&config);
    let mcp_routes_loaded =
        mcp_handler.as_ref().map(McpProxyHandler::routes_loaded).unwrap_or(false);
    lifecycle.set_mcp_routes_loaded(!mcp_enabled || mcp_routes_loaded).await;
    let model_catalog_payload =
        llm_handler.as_ref().map(build_model_catalog_payload).transpose()?.map(Arc::new);

    let app_state = AppState {
        lifecycle: lifecycle.clone(),
        metrics: metrics.clone(),
        metrics_registry: metrics.clone(),
        sanitization: SanitizationState {
            enabled: config.sanitization.enabled,
            evaluator,
            api_key_bindings: Arc::new(resolved_keys),
        },
        rate_limit: RateLimitState {
            enabled: config.rate_limit.enabled,
            limiter: if config.rate_limit.enabled {
                Some(Arc::new(RateLimiter::new(
                    config.rate_limit.default_profile.clone(),
                    config.rate_limit.profiles.clone(),
                )))
            } else {
                None
            },
        },
        llm: LlmProxyState {
            enabled: llm_enabled,
            handler: llm_handler.map(Arc::new),
            model_catalog_payload,
            response_metadata_mode: config.response_envelope.pokrov_metadata.mode,
        },
        mcp: McpProxyState {
            enabled: mcp_enabled,
            handler: mcp_handler.map(Arc::new),
            response_metadata_mode: config.response_envelope.pokrov_metadata.mode,
        },
        auth: AuthState {
            upstream_auth_mode: config.auth.upstream_auth_mode,
            allow_single_bearer_passthrough: config.auth.allow_single_bearer_passthrough,
            gateway_auth_mode: config.auth.gateway_auth_mode,
            internal_mtls_identity_header: config.auth.internal_mtls.identity_header.clone(),
            internal_mtls_require_header: config.auth.internal_mtls.require_header,
            mesh_identity_header: config.auth.mesh.identity_header.clone(),
            mesh_required_spiffe_trust_domain: config
                .auth
                .mesh
                .required_spiffe_trust_domain
                .clone(),
            mesh_require_header: config.auth.mesh.require_header,
            identity_resolution_order: Arc::new(config.identity.resolution_order.clone()),
            identity_profile_bindings: Arc::new(config.identity.profile_bindings.clone()),
            identity_rate_limit_bindings: Arc::new(config.identity.rate_limit_bindings.clone()),
            fallback_policy_profile: config.identity.fallback_policy_profile.clone(),
            required_for_policy: config.identity.required_for_policy,
            required_for_rate_limit: config.identity.required_for_rate_limit,
        },
    };

    let app = build_router(app_state);
    lifecycle.mark_ready().await;
    metrics.on_lifecycle_event(LifecycleEvent::Ready);
    log_lifecycle_event("runtime", "lifecycle_transition", None, "ready");

    let drain_timeout = Duration::from_millis(config.shutdown.drain_timeout_ms);
    let grace_period = Duration::from_millis(config.shutdown.grace_period_ms);
    let (shutdown_started_tx, mut shutdown_started_rx) = watch::channel(false);
    let graceful = {
        let lifecycle = lifecycle.clone();
        let metrics = metrics.clone();
        async move {
            shutdown_signal.await;
            let _ = shutdown_started_tx.send(true);
            lifecycle.mark_draining().await;
            metrics.on_lifecycle_event(LifecycleEvent::Draining);
            log_lifecycle_event("runtime", "lifecycle_transition", None, "draining");
        }
    };

    let serve_result = if config.server.tls.enabled {
        serve_tls_with_graceful_shutdown(
            listener,
            app,
            &config,
            &mut shutdown_started_rx,
            graceful,
            grace_period,
        )
        .await
    } else {
        let serve = axum::serve(listener, app).with_graceful_shutdown(graceful).into_future();
        let mut serve = std::pin::pin!(serve);
        tokio::select! {
            result = &mut serve => map_serve_result(result),
            changed = shutdown_started_rx.changed() => {
                if changed.is_ok() && *shutdown_started_rx.borrow() {
                    match tokio::time::timeout(grace_period, &mut serve).await {
                        Ok(result) => map_serve_result(result),
                        Err(_) => {
                            lifecycle.wait_for_drain(drain_timeout).await;
                            mark_runtime_stopped(&lifecycle, &metrics).await;
                            let timeout_error = std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                format!(
                                    "graceful shutdown exceeded grace_period_ms={}",
                                    config.shutdown.grace_period_ms
                                ),
                            );
                            error!(
                                action = "shutdown",
                                grace_period_ms = config.shutdown.grace_period_ms,
                                "runtime graceful shutdown exceeded configured grace period"
                            );
                            Err(BootstrapError::Serve(timeout_error))
                        }
                    }
                } else {
                    map_serve_result(serve.await)
                }
            }
        }
    };

    if serve_result.is_ok() && *shutdown_started_rx.borrow() {
        lifecycle.wait_for_drain(drain_timeout).await;
        mark_runtime_stopped(&lifecycle, &metrics).await;
    }

    serve_result
}

fn resolve_api_key_bindings(
    config: &RuntimeConfig,
) -> Result<Vec<ResolvedApiKeyBinding>, BootstrapError> {
    let mut bindings = Vec::new();
    let mut unresolved_bindings = 0usize;

    for binding in &config.security.api_keys {
        let Some(secret_ref) = SecretRef::parse(&binding.key) else {
            continue;
        };

        let secret = match secret_ref {
            SecretRef::Env(ref name) => std::env::var(name).ok(),
            SecretRef::File(ref path) => {
                std::fs::read_to_string(path).ok().map(|content| content.trim().to_string())
            }
        };

        let Some(secret) = secret else {
            unresolved_bindings += 1;
            warn!(
                component = "runtime",
                action = "api_key_binding_skipped",
                profile = %binding.profile,
                "failed to resolve API key reference; binding skipped"
            );
            continue;
        };

        bindings.push(ResolvedApiKeyBinding { key: secret, profile: binding.profile.clone() });
    }

    if config.security.fail_on_unresolved_api_keys && unresolved_bindings > 0 {
        return Err(BootstrapError::Security(format!(
            "failed to resolve {unresolved_bindings} API key binding(s)"
        )));
    }

    Ok(bindings)
}

async fn serve_tls_with_graceful_shutdown<S>(
    listener: TcpListener,
    app: axum::Router,
    config: &RuntimeConfig,
    shutdown_started_rx: &mut watch::Receiver<bool>,
    shutdown_signal: S,
    grace_period: Duration,
) -> Result<(), BootstrapError>
where
    S: std::future::Future<Output = ()> + Send + 'static,
{
    let tls_acceptor = build_tls_acceptor(config)?;
    let identity_header_name = config.auth.internal_mtls.identity_header.clone();
    let mut shutdown_task = tokio::spawn(shutdown_signal);
    let mut connection_tasks = JoinSet::new();

    loop {
        tokio::select! {
            _ = &mut shutdown_task => {
                break;
            }
            changed = shutdown_started_rx.changed() => {
                if changed.is_ok() && *shutdown_started_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (socket, remote_addr) = accepted.map_err(BootstrapError::Serve)?;
                let tls_acceptor = tls_acceptor.clone();
                let app = app.clone();
                let identity_header_name = identity_header_name.clone();
                connection_tasks.spawn(async move {
                    if let Err(error) = handle_tls_connection(
                        socket,
                        remote_addr,
                        tls_acceptor,
                        app,
                        identity_header_name,
                    ).await {
                        error!(
                            action = "tls_connection_failed",
                            remote_addr = %remote_addr,
                            error = %error,
                            "failed to serve tls connection"
                        );
                    }
                });
            }
        }
    }

    let wait_connections = async {
        while let Some(join_result) = connection_tasks.join_next().await {
            if let Err(join_error) = join_result {
                error!(
                    action = "tls_connection_join_failed",
                    error = %join_error,
                    "tls connection task failed"
                );
            }
        }
    };

    match tokio::time::timeout(grace_period, wait_connections).await {
        Ok(_) => Ok(()),
        Err(_) => {
            let timeout_error = std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "graceful shutdown exceeded grace_period_ms={}",
                    config.shutdown.grace_period_ms
                ),
            );
            error!(
                action = "shutdown",
                grace_period_ms = config.shutdown.grace_period_ms,
                "runtime graceful shutdown exceeded configured grace period"
            );
            Err(BootstrapError::Serve(timeout_error))
        }
    }
}

async fn handle_tls_connection(
    socket: TcpStream,
    remote_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    app: axum::Router,
    identity_header_name: String,
) -> Result<(), std::io::Error> {
    let tls_stream = tls_acceptor.accept(socket).await.map_err(std::io::Error::other)?;
    let verified_subject = match extract_peer_subject(&tls_stream) {
        Ok(subject) => subject,
        Err(error) => {
            warn!(
                action = "tls_peer_identity_parse_failed",
                remote_addr = %remote_addr,
                error = %error,
                "failed to parse peer certificate subject"
            );
            None
        }
    };
    let io = TokioIo::new(tls_stream);
    let service = service_fn(move |mut request: Request<hyper::body::Incoming>| {
        let app = app.clone();
        let verified_subject = verified_subject.clone();
        let identity_header_name = identity_header_name.clone();
        async move {
            if let Ok(header_name) =
                axum::http::header::HeaderName::from_bytes(identity_header_name.as_bytes())
            {
                request.headers_mut().remove(&header_name);
            }
            if let Some(subject) = verified_subject.clone() {
                request.extensions_mut().insert(VerifiedClientCertIdentity { subject });
            }

            let request = request.map(axum::body::Body::new);
            let response = app.oneshot(request).await;
            let response: Response = match response {
                Ok(response) => response,
                Err(error) => match error {},
            };
            Ok::<Response, Infallible>(response)
        }
    });

    hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .await
        .map_err(std::io::Error::other)?;
    info!(
        action = "tls_connection_closed",
        remote_addr = %remote_addr,
        "tls connection closed"
    );
    Ok(())
}

fn build_tls_acceptor(config: &RuntimeConfig) -> Result<TlsAcceptor, BootstrapError> {
    install_rustls_crypto_provider_once();

    let cert_path = config.server.tls.cert_file.as_deref().ok_or_else(|| {
        BootstrapError::Security(
            "server.tls.cert_file must be configured when tls is enabled".to_string(),
        )
    })?;
    let key_path = config.server.tls.key_file.as_deref().ok_or_else(|| {
        BootstrapError::Security(
            "server.tls.key_file must be configured when tls is enabled".to_string(),
        )
    })?;
    let cert_chain = load_cert_chain(cert_path)?;
    let private_key = load_private_key(key_path)?;

    let server_config = if config.server.tls.require_client_cert {
        let client_ca_path = config.server.tls.client_ca_file.as_deref().ok_or_else(|| {
            BootstrapError::Security(
                "server.tls.client_ca_file must be configured when client cert is required"
                    .to_string(),
            )
        })?;
        let client_roots = load_root_store(client_ca_path)?;
        let verifier = WebPkiClientVerifier::builder(client_roots).build().map_err(|error| {
            BootstrapError::Security(format!("invalid mTLS verifier config: {error}"))
        })?;
        ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(cert_chain, private_key)
            .map_err(|error| {
                BootstrapError::Security(format!("invalid tls cert/key pair: {error}"))
            })?
    } else {
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .map_err(|error| {
                BootstrapError::Security(format!("invalid tls cert/key pair: {error}"))
            })?
    };

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

fn install_rustls_crypto_provider_once() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

fn load_cert_chain(path: &str) -> Result<Vec<CertificateDer<'static>>, BootstrapError> {
    let file = std::fs::File::open(path).map_err(BootstrapError::Bind)?;
    let mut reader = BufReader::new(file);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::Bind)?;
    if certs.is_empty() {
        return Err(BootstrapError::Security(format!(
            "tls certificate file contains no certificates: {path}"
        )));
    }
    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, BootstrapError> {
    let file = std::fs::File::open(path).map_err(BootstrapError::Bind)?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader).map_err(BootstrapError::Bind)?.ok_or_else(|| {
        BootstrapError::Security(format!("tls private key is missing in file: {path}"))
    })
}

fn load_root_store(path: &str) -> Result<Arc<RootCertStore>, BootstrapError> {
    let certs = load_cert_chain(path)?;
    let mut roots = RootCertStore::empty();
    let (_, rejected) = roots.add_parsable_certificates(certs);
    if rejected > 0 {
        return Err(BootstrapError::Security(format!(
            "client ca file contains {rejected} unparsable certificate(s): {path}"
        )));
    }
    Ok(Arc::new(roots))
}

fn extract_peer_subject(stream: &TlsStream<TcpStream>) -> Result<Option<String>, String> {
    let (_, server_conn) = stream.get_ref();
    let Some(cert) = server_conn.peer_certificates().and_then(|certs| certs.first()) else {
        return Ok(None);
    };

    parse_peer_subject(cert.as_ref()).map(Some)
}

fn parse_peer_subject(certificate_der: &[u8]) -> Result<String, String> {
    let (_, parsed) = parse_x509_certificate(certificate_der)
        .map_err(|error| format!("invalid x509 certificate: {error}"))?;
    Ok(parsed.subject().to_string())
}

#[cfg(feature = "ner")]
fn init_ner_engine(
    engine: SanitizationEngine,
    config: &RuntimeConfig,
) -> Result<SanitizationEngine, BootstrapError> {
    use pokrov_core::ner_adapter::{NerAdapter, NerAdapterConfig, NerFailMode};

    let Some(ner_config) = &config.ner else {
        return Ok(engine);
    };
    if !ner_config.enabled {
        return Ok(engine);
    }

    let num_models = ner_config.models.len();

    let ner_engine = pokrov_ner::NerEngine::new(pokrov_ner::NerConfig {
        models: ner_config
            .models
            .iter()
            .map(|m| pokrov_ner::model::NerModelBinding {
                language: m.language.clone(),
                model_path: std::path::PathBuf::from(&m.model_path),
                tokenizer_path: std::path::PathBuf::from(&m.tokenizer_path),
                priority: m.priority,
            })
            .collect(),
        default_language: ner_config.default_language.clone(),
        fallback_language: ner_config.fallback_language.clone(),
        timeout_ms: ner_config.timeout_ms,
        max_seq_length: ner_config.max_seq_length,
        confidence_threshold: ner_config.confidence_threshold,
        execution: ner_config.execution,
        merge_strategy: ner_config.merge_strategy,
    })
    .map_err(|e| {
        BootstrapError::Sanitization(EvaluateError::RuntimeFailure(format!(
            "NER engine init failed: {e}"
        )))
    })?;

    let adapter = Arc::new(NerAdapter::new(
        ner_engine,
        NerAdapterConfig {
            enabled: true,
            fail_mode: NerFailMode::FailOpen,
            entity_types: vec![
                pokrov_ner::NerEntityType::Person,
                pokrov_ner::NerEntityType::Organization,
            ],
            timeout_ms: ner_config.timeout_ms,
            execution_mode: ner_config.execution,
            num_models,
        },
    ));

    let mut ner_profile_types =
        std::collections::HashMap::<String, Vec<pokrov_ner::NerEntityType>>::new();
    let mut ner_profile_fail_modes = std::collections::HashMap::<String, NerFailMode>::new();
    for (profile_id, profile_cfg) in &ner_config.profiles {
        let fail_mode = match profile_cfg.fail_mode {
            pokrov_config::model::NerFailMode::FailOpen => NerFailMode::FailOpen,
            pokrov_config::model::NerFailMode::FailClosed => NerFailMode::FailClosed,
        };
        ner_profile_fail_modes.insert(profile_id.clone(), fail_mode);

        let types: Vec<pokrov_ner::NerEntityType> = profile_cfg
            .entity_types
            .iter()
            .filter_map(|s| serde_json::from_value(serde_json::json!(s)).ok())
            .collect();
        if !types.is_empty() {
            ner_profile_types.insert(profile_id.clone(), types);
        }
    }

    let mut engine = engine
        .with_ner(adapter)
        .with_ner_profiles(ner_profile_types)
        .with_ner_fail_modes(ner_profile_fail_modes)
        .with_ner_llm_skip_filter(ner_config.skip_llm_tools_and_system);

    if !ner_config.skip_fields.is_empty() {
        let patterns: Result<Vec<regex::Regex>, _> = ner_config
            .skip_fields
            .iter()
            .map(|p| {
                regex::Regex::new(p).map_err(|e| {
                    BootstrapError::Sanitization(EvaluateError::RuntimeFailure(format!(
                        "NER skip_fields pattern invalid: {e}"
                    )))
                })
            })
            .collect();
        engine = engine.with_ner_skip_fields(patterns?);
    }

    if !ner_config.strip_values.is_empty() {
        let patterns: Result<Vec<regex::Regex>, _> = ner_config
            .strip_values
            .iter()
            .map(|p| {
                regex::Regex::new(p).map_err(|e| {
                    BootstrapError::Sanitization(EvaluateError::RuntimeFailure(format!(
                        "NER strip_values pattern invalid: {e}"
                    )))
                })
            })
            .collect();
        engine = engine.with_ner_strip_values(patterns?);
    }

    if !ner_config.exclude_entity_patterns.is_empty() {
        let patterns: Result<Vec<regex::Regex>, _> = ner_config
            .exclude_entity_patterns
            .iter()
            .map(|p| {
                regex::Regex::new(p).map_err(|e| {
                    BootstrapError::Sanitization(EvaluateError::RuntimeFailure(format!(
                        "NER exclude_entity_patterns pattern invalid: {e}"
                    )))
                })
            })
            .collect();
        engine = engine.with_ner_exclude_entity_patterns(patterns?);
    }

    Ok(engine)
}

fn is_llm_enabled(config: &RuntimeConfig) -> bool {
    config.llm.is_some()
}

fn is_mcp_enabled(config: &RuntimeConfig) -> bool {
    config.mcp.is_some()
}

fn build_llm_handler(
    config: &RuntimeConfig,
    evaluator: Option<Arc<SanitizationEngine>>,
    metrics: Arc<RuntimeMetricsRegistry>,
) -> Result<Option<LLMProxyHandler>, BootstrapError> {
    let Some(llm_config) = config.llm.as_ref() else {
        return Ok(None);
    };

    let resolved_provider_keys =
        resolve_llm_provider_keys(llm_config, config.security.fail_on_unresolved_provider_keys)?;
    let routes = ProviderRouteTable::from_config(llm_config, &resolved_provider_keys)
        .map_err(|error| BootstrapError::LlmProxy(error.to_string()))?;

    let metadata_mode = config.response_envelope.pokrov_metadata.mode;
    #[cfg(feature = "llm_payload_trace")]
    let payload_trace_sink = build_llm_payload_trace_sink(config)?;

    let handler = LLMProxyHandler::new(
        evaluator,
        metrics,
        routes,
        metadata_mode,
        #[cfg(feature = "llm_payload_trace")]
        payload_trace_sink,
    )
    .map_err(|error| BootstrapError::LlmProxy(error.to_string()))?;

    Ok(Some(handler))
}

#[cfg(not(feature = "llm_payload_trace"))]
fn validate_llm_payload_trace_mode(config: &RuntimeConfig) -> Result<(), BootstrapError> {
    if !config.observability.llm_payload_trace.enabled {
        return Ok(());
    }

    Err(BootstrapError::Security(
        "observability.llm_payload_trace.enabled requires runtime feature 'llm_payload_trace'"
            .to_string(),
    ))
}

#[cfg(feature = "llm_payload_trace")]
fn validate_llm_payload_trace_mode(config: &RuntimeConfig) -> Result<(), BootstrapError> {
    if !config.observability.llm_payload_trace.enabled {
        return Ok(());
    }

    if !cfg!(debug_assertions) {
        return Err(BootstrapError::Security(
            "observability.llm_payload_trace.enabled is forbidden in release builds".to_string(),
        ));
    }

    Ok(())
}

#[cfg(feature = "llm_payload_trace")]
fn build_llm_payload_trace_sink(
    config: &RuntimeConfig,
) -> Result<Option<pokrov_proxy_llm::trace::LlmPayloadTraceSink>, BootstrapError> {
    if !config.observability.llm_payload_trace.enabled {
        return Ok(None);
    }

    let sink = pokrov_proxy_llm::trace::LlmPayloadTraceSink::new(
        &config.observability.llm_payload_trace.output_path,
    )
    .map_err(|error| {
        BootstrapError::Security(format!(
            "failed to initialize llm payload trace sink at '{}': {error}",
            config.observability.llm_payload_trace.output_path
        ))
    })?;

    info!(
        component = "runtime",
        action = "llm_payload_trace_enabled",
        output_path = %config.observability.llm_payload_trace.output_path,
        "llm payload trace sink enabled for debug build"
    );

    Ok(Some(sink))
}

#[derive(Serialize)]
struct PrebuiltModelsPayload<'a> {
    object: &'static str,
    data: Vec<PrebuiltModelsEntry<'a>>,
}

#[derive(Serialize)]
struct PrebuiltModelsEntry<'a> {
    id: &'a str,
    object: &'static str,
    created: u64,
    owned_by: &'static str,
}

fn build_model_catalog_payload(handler: &LLMProxyHandler) -> Result<Vec<u8>, BootstrapError> {
    let data = handler
        .model_catalog()
        .iter()
        .map(|entry| PrebuiltModelsEntry {
            id: entry.id.as_str(),
            object: "model",
            created: 0,
            owned_by: "pokrov",
        })
        .collect::<Vec<_>>();
    let payload = PrebuiltModelsPayload { object: "list", data };
    serde_json::to_vec(&payload).map_err(|error| BootstrapError::LlmProxy(error.to_string()))
}

fn build_mcp_handler(
    config: &RuntimeConfig,
    evaluator: Option<Arc<SanitizationEngine>>,
    metrics: Arc<RuntimeMetricsRegistry>,
) -> Result<Option<McpProxyHandler>, BootstrapError> {
    let Some(mcp_config) = config.mcp.clone() else {
        return Ok(None);
    };

    let handler = McpProxyHandler::new(evaluator, metrics, mcp_config)
        .map_err(|error| BootstrapError::McpProxy(error.to_string()))?;

    Ok(Some(handler))
}

fn resolve_llm_provider_keys(
    config: &LlmConfig,
    fail_on_unresolved_provider_keys: bool,
) -> Result<std::collections::BTreeMap<String, String>, BootstrapError> {
    let mut keys = std::collections::BTreeMap::new();
    let mut unresolved_provider_keys = 0usize;

    for provider in &config.providers {
        let Some(secret_ref) = SecretRef::parse(&provider.auth.api_key) else {
            continue;
        };

        let secret = match secret_ref {
            SecretRef::Env(ref name) => std::env::var(name).ok(),
            SecretRef::File(ref path) => {
                std::fs::read_to_string(path).ok().map(|content| content.trim().to_string())
            }
        };

        let Some(secret) = secret else {
            unresolved_provider_keys += 1;
            warn!(
                component = "runtime",
                action = "llm_provider_key_skipped",
                provider_id = %provider.id,
                "failed to resolve provider auth reference; provider skipped"
            );
            continue;
        };

        keys.insert(provider.id.clone(), secret);
    }

    if fail_on_unresolved_provider_keys && unresolved_provider_keys > 0 {
        return Err(BootstrapError::Security(format!(
            "failed to resolve {unresolved_provider_keys} llm provider key binding(s)"
        )));
    }

    Ok(keys)
}

fn resolve_git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0000000".to_string())
}

fn map_serve_result(result: Result<(), std::io::Error>) -> Result<(), BootstrapError> {
    result.map_err(|error| {
        error!(action = "shutdown", error = %error, "runtime serve failed");
        BootstrapError::Serve(error)
    })
}

async fn mark_runtime_stopped(
    lifecycle: &SharedRuntimeLifecycle,
    metrics: &Arc<RuntimeMetricsRegistry>,
) {
    lifecycle.mark_stopped().await;
    metrics.on_lifecycle_event(LifecycleEvent::Stopped);
    log_lifecycle_event("runtime", "lifecycle_transition", None, "stopped");
    info!(action = "shutdown", "runtime lifecycle moved to stopped");
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate_signal) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate_signal.recv() => {}
                }
            }
            Err(sigterm_error) => {
                error!(
                    action = "shutdown",
                    error = %sigterm_error,
                    "failed to install SIGTERM handler; using SIGINT-only shutdown signal"
                );
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn wait_until_runtime_started(lifecycle: SharedRuntimeLifecycle) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while lifecycle.state().await == crate::lifecycle::RuntimeState::Starting {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, parse_peer_subject};

    #[test]
    fn parse_args_rejects_unknown_argument() {
        let args = vec!["--config".to_string(), "config.yaml".to_string(), "--unknown".to_string()];

        let error = parse_args(&args).expect_err("unknown argument must fail parsing");
        assert!(error.to_string().contains("unknown argument: --unknown"));
    }

    #[test]
    fn parse_peer_subject_rejects_invalid_der() {
        let error = parse_peer_subject(b"not-a-certificate")
            .expect_err("invalid DER input must fail subject parsing");
        assert!(error.contains("invalid x509 certificate"));
    }
}
