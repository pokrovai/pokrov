use std::{
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::Value;
use tempfile::NamedTempFile;
use tokio::{
    net::TcpListener,
    sync::{oneshot, Mutex},
};

#[derive(Clone)]
struct MockState {
    mode: MockMcpMode,
    requests: Arc<Mutex<Vec<Value>>>,
    hits: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
pub enum MockMcpMode {
    Json { status: u16, body: Value },
}

pub struct MockMcpHandle {
    pub base_url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    hits: Arc<AtomicUsize>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockMcpHandle {
    pub fn request_count(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    pub async fn captured_requests(&self) -> Vec<Value> {
        self.requests.lock().await.clone()
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.task.await;
    }
}

pub async fn start_mock_mcp_server(mode: MockMcpMode) -> MockMcpHandle {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let hits = Arc::new(AtomicUsize::new(0));
    let state = MockState {
        mode,
        requests: requests.clone(),
        hits: hits.clone(),
    };

    let app = Router::new().route("/tool-call", post(mock_tool_call)).with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock MCP listener should bind");
    let addr = listener
        .local_addr()
        .expect("mock MCP listener should expose local addr");

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        let _ = server.await;
    });

    MockMcpHandle {
        base_url: format!("http://{addr}"),
        requests,
        hits,
        shutdown_tx: Some(shutdown_tx),
        task,
    }
}

async fn mock_tool_call(State(state): State<MockState>, Json(body): Json<Value>) -> impl IntoResponse {
    state.hits.fetch_add(1, Ordering::Relaxed);
    state.requests.lock().await.push(body);

    match state.mode {
        MockMcpMode::Json { status, ref body } => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
            Json(body.clone()),
        )
            .into_response(),
    }
}

pub fn write_key_file(value: &str) -> PathBuf {
    let mut file = NamedTempFile::new().expect("key file should be created");
    file.write_all(value.as_bytes())
        .expect("key file should be written");
    file.into_temp_path().keep().expect("key file path should persist")
}

pub fn write_runtime_config(content: &str) -> PathBuf {
    let mut file = NamedTempFile::new().expect("config file should be created");
    file.write_all(content.as_bytes())
        .expect("config file should be written");
    file.into_temp_path()
        .keep()
        .expect("config file path should persist")
}
