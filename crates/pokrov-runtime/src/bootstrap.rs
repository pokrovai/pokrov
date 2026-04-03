use std::{
    error::Error,
    fmt,
    future::IntoFuture,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use pokrov_api::app::{build_router, AppState, ResolvedApiKeyBinding, SanitizationState};
use pokrov_config::{
    error::ConfigError,
    loader::load_runtime_config,
    model::{RuntimeConfig, SecretRef},
};
use pokrov_core::{types::EvaluateError, SanitizationEngine};
use pokrov_metrics::{
    hooks::{LifecycleEvent, RuntimeMetricsHooks},
    registry::RuntimeMetricsRegistry,
};
use tokio::{
    net::TcpListener,
    sync::{oneshot, watch},
    task::JoinHandle,
};
use tracing::{error, info, warn};

use crate::{
    lifecycle::{RuntimeLifecycle, SharedRuntimeLifecycle},
    observability::{init_json_observability, log_lifecycle_event},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapArgs {
    pub config_path: PathBuf,
}

#[derive(Debug)]
pub enum BootstrapError {
    InvalidArguments(String),
    Config(ConfigError),
    Sanitization(EvaluateError),
    Security(String),
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
            Self::Security(message) => write!(f, "security bootstrap failed: {message}"),
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
            Self::Bind(error) => Some(error),
            Self::Serve(error) => Some(error),
            Self::Join(error) => Some(error),
            Self::InvalidArguments(_) | Self::Security(_) => None,
        }
    }
}

pub fn parse_args(args: &[String]) -> Result<BootstrapArgs, BootstrapError> {
    let mut iter = args.iter();
    let mut config_path = None;

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
            _ => {
                return Err(BootstrapError::InvalidArguments(format!(
                    "unknown argument: {arg}"
                )));
            }
        }
    }

    match config_path {
        Some(config_path) => Ok(BootstrapArgs { config_path }),
        None => Err(BootstrapError::InvalidArguments("expected --config <path>".to_string())),
    }
}

pub async fn run(args: BootstrapArgs) -> Result<(), BootstrapError> {
    let config = load_runtime_config(&args.config_path).map_err(BootstrapError::Config)?;
    init_json_observability(config.logging.level.as_str());

    let listener = bind_listener(&config).await?;
    let addr = listener.local_addr().map_err(BootstrapError::Bind)?;
    info!(action = "startup", addr = %addr, "runtime listener bound");

    let lifecycle = Arc::new(RuntimeLifecycle::new());
    let metrics = Arc::new(RuntimeMetricsRegistry::default());

    run_with_listener(config, listener, lifecycle, metrics, async { wait_for_shutdown_signal().await })
    .await
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

    let app_state = AppState {
        lifecycle: lifecycle.clone(),
        metrics: metrics.clone(),
        sanitization: SanitizationState {
            enabled: config.sanitization.enabled,
            evaluator,
            api_key_bindings: Arc::new(resolved_keys),
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
