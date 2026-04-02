use std::{error::Error, fmt, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use pokrov_api::app::{build_router, AppState};
use pokrov_config::{error::ConfigError, loader::load_runtime_config, model::RuntimeConfig};
use pokrov_metrics::{
    hooks::{LifecycleEvent, RuntimeMetricsHooks},
    registry::RuntimeMetricsRegistry,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tracing::{error, info};

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
    Bind(std::io::Error),
    Serve(std::io::Error),
    Join(tokio::task::JoinError),
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArguments(message) => write!(f, "invalid arguments: {message}"),
            Self::Config(error) => write!(f, "{error}"),
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
            Self::Bind(error) => Some(error),
            Self::Serve(error) => Some(error),
            Self::Join(error) => Some(error),
            Self::InvalidArguments(_) => None,
        }
    }
}

pub fn parse_args(args: &[String]) -> Result<BootstrapArgs, BootstrapError> {
    let mut iter = args.iter();
    let mut config_path = None;

    while let Some(arg) = iter.next() {
        if arg == "--config" {
            config_path = iter.next().map(PathBuf::from);
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

    run_with_listener(config, listener, lifecycle, metrics, async {
        let _ = tokio::signal::ctrl_c().await;
    })
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
    let metrics = Arc::new(RuntimeMetricsRegistry::default());
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        run_with_listener(config, listener, lifecycle, metrics, async {
            let _ = shutdown_rx.await;
        })
        .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

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
    lifecycle.set_config_loaded(true).await;

    let app_state = AppState { lifecycle: lifecycle.clone(), metrics: metrics.clone() };

    let app = build_router(app_state);
    lifecycle.mark_ready().await;
    metrics.on_lifecycle_event(LifecycleEvent::Ready);
    log_lifecycle_event("runtime", "lifecycle_transition", None, "ready");

    let drain_timeout = Duration::from_millis(config.shutdown.drain_timeout_ms);
    let graceful = {
        let lifecycle = lifecycle.clone();
        let metrics = metrics.clone();
        async move {
            shutdown_signal.await;
            lifecycle.mark_draining().await;
            metrics.on_lifecycle_event(LifecycleEvent::Draining);
            log_lifecycle_event("runtime", "lifecycle_transition", None, "draining");
            lifecycle.wait_for_drain(drain_timeout).await;
            lifecycle.mark_stopped().await;
            metrics.on_lifecycle_event(LifecycleEvent::Stopped);
            log_lifecycle_event("runtime", "lifecycle_transition", None, "stopped");
            info!(action = "shutdown", "runtime lifecycle moved to stopped");
        }
    };

    axum::serve(listener, app).with_graceful_shutdown(graceful).await.map_err(|error| {
        error!(action = "shutdown", error = %error, "runtime serve failed");
        BootstrapError::Serve(error)
    })
}
