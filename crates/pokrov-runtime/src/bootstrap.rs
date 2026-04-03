use std::{
    error::Error,
    fmt,
    future::IntoFuture,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use pokrov_api::app::{
    build_router, AppState, LlmProxyState, McpProxyState, RateLimitState, ResolvedApiKeyBinding,
    SanitizationState,
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
use tokio::{
    net::TcpListener,
    sync::{oneshot, watch},
    task::JoinHandle,
};
use time::OffsetDateTime;
use tracing::{error, info, warn};

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
            Self::Sanitization(error) => write!(f, "failed to initialize sanitization engine: {error}"),
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
            Self::InvalidArguments(_) | Self::LlmProxy(_) | Self::McpProxy(_) | Self::Security(_) => {
                None
            }
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
                return Err(BootstrapError::InvalidArguments(format!(
                    "unknown argument: {arg}"
                )));
            }
        }
    }

    if config_path.is_none() && release_evidence_output.is_none() {
        return Err(BootstrapError::InvalidArguments(
            "expected --config <path> or --release-evidence-output <path>".to_string(),
        ));
    }

    Ok(BootstrapArgs {
        config_path,
        release_evidence_output,
        release_id,
        evidence_artifacts,
    })
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
    init_json_observability(config.logging.level.as_str());

    let listener = bind_listener(&config).await?;
    let addr = listener.local_addr().map_err(BootstrapError::Bind)?;
    info!(action = "startup", addr = %addr, "runtime listener bound");

    let lifecycle = Arc::new(RuntimeLifecycle::new());
    let metrics = Arc::new(RuntimeMetricsRegistry::default());

    run_with_listener(config, listener, lifecycle, metrics, async { wait_for_shutdown_signal().await })
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
        artifacts.push(ArtifactChecksum {
            path: "placeholder".to_string(),
            sha256: "0".repeat(64),
        });
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
            "Populate performance/security/operational sections from validated test outputs".to_string(),
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
        Some(Arc::new(
            SanitizationEngine::new(config.evaluator_config()).map_err(BootstrapError::Sanitization)?,
        ))
    } else {
        None
    };

    let resolved_keys = resolve_api_key_bindings(&config)?;
    let policy_loaded = !config.sanitization.enabled || evaluator.is_some();
    lifecycle.set_config_loaded(policy_loaded).await;
    lifecycle
        .set_llm_routes_loaded(!is_llm_enabled(&config))
        .await;
    lifecycle
        .set_mcp_routes_loaded(!is_mcp_enabled(&config))
        .await;

    let llm_handler = build_llm_handler(&config, evaluator.clone(), metrics.clone())?;
    let llm_enabled = is_llm_enabled(&config);
    let llm_routes_loaded = llm_handler
        .as_ref()
        .map(LLMProxyHandler::routes_loaded)
        .unwrap_or(false);
    lifecycle
        .set_llm_routes_loaded(!llm_enabled || llm_routes_loaded)
        .await;
    let mcp_handler = build_mcp_handler(&config, evaluator.clone(), metrics.clone())?;
    let mcp_enabled = is_mcp_enabled(&config);
    let mcp_routes_loaded = mcp_handler
        .as_ref()
        .map(McpProxyHandler::routes_loaded)
        .unwrap_or(false);
    lifecycle
        .set_mcp_routes_loaded(!mcp_enabled || mcp_routes_loaded)
        .await;

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
        },
        mcp: McpProxyState {
            enabled: mcp_enabled,
            handler: mcp_handler.map(Arc::new),
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

    let serve = axum::serve(listener, app).with_graceful_shutdown(graceful).into_future();
    let mut serve = std::pin::pin!(serve);
    let serve_result = tokio::select! {
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
    };

    if serve_result.is_ok() && *shutdown_started_rx.borrow() {
        lifecycle.wait_for_drain(drain_timeout).await;
        mark_runtime_stopped(&lifecycle, &metrics).await;
    }

    serve_result
}

fn resolve_api_key_bindings(config: &RuntimeConfig) -> Result<Vec<ResolvedApiKeyBinding>, BootstrapError> {
    let mut bindings = Vec::new();
    let mut unresolved_bindings = 0usize;

    for binding in &config.security.api_keys {
        let Some(secret_ref) = SecretRef::parse(&binding.key) else {
            continue;
        };

        let secret = match secret_ref {
            SecretRef::Env(ref name) => std::env::var(name).ok(),
            SecretRef::File(ref path) => std::fs::read_to_string(path)
                .ok()
                .map(|content| content.trim().to_string()),
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

        bindings.push(ResolvedApiKeyBinding {
            key: secret,
            profile: binding.profile.clone(),
        });
    }

    if config.security.fail_on_unresolved_api_keys && unresolved_bindings > 0 {
        return Err(BootstrapError::Security(format!(
            "failed to resolve {unresolved_bindings} API key binding(s)"
        )));
    }

    Ok(bindings)
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

    let resolved_provider_keys = resolve_llm_provider_keys(
        llm_config,
        config.security.fail_on_unresolved_provider_keys,
    )?;
    let routes = ProviderRouteTable::from_config(llm_config, &resolved_provider_keys)
        .map_err(|error| BootstrapError::LlmProxy(error.to_string()))?;

    let handler = LLMProxyHandler::new(evaluator, metrics, routes)
        .map_err(|error| BootstrapError::LlmProxy(error.to_string()))?;

    Ok(Some(handler))
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
            SecretRef::File(ref path) => std::fs::read_to_string(path)
                .ok()
                .map(|content| content.trim().to_string()),
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
    use super::parse_args;

    #[test]
    fn parse_args_rejects_unknown_argument() {
        let args = vec![
            "--config".to_string(),
            "config.yaml".to_string(),
            "--unknown".to_string(),
        ];

        let error = parse_args(&args).expect_err("unknown argument must fail parsing");
        assert!(error.to_string().contains("unknown argument: --unknown"));
    }
}
