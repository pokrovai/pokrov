use std::{
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
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
    mode: MockProviderMode,
    requests: Arc<Mutex<Vec<CapturedChatRequest>>>,
    hits: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
pub enum MockProviderMode {
    Json { status: u16, body: Value },
    Sse { status: u16, body: String },
}

pub struct MockProviderHandle {
    pub base_url: String,
    requests: Arc<Mutex<Vec<CapturedChatRequest>>>,
    hits: Arc<AtomicUsize>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockProviderHandle {
    pub fn request_count(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    pub async fn captured_requests(&self) -> Vec<Value> {
        self.requests.lock().await.iter().map(|request| request.body.clone()).collect()
    }

    pub async fn captured_authorization_headers(&self) -> Vec<Option<String>> {
        self.requests.lock().await.iter().map(|request| request.authorization.clone()).collect()
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.task.await;
    }
}

pub async fn start_mock_provider(mode: MockProviderMode) -> MockProviderHandle {
    let requests = Arc::new(Mutex::new(Vec::<CapturedChatRequest>::new()));
    let hits = Arc::new(AtomicUsize::new(0));
    let state = MockState { mode, requests: requests.clone(), hits: hits.clone() };

    let app =
        Router::new().route("/v1/chat/completions", post(mock_chat_completions)).with_state(state);

    let listener =
        TcpListener::bind("127.0.0.1:0").await.expect("mock provider listener should bind");
    let addr = listener.local_addr().expect("mock provider listener should expose local addr");

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        let _ = server.await;
    });

    MockProviderHandle {
        base_url: format!("http://{addr}/v1"),
        requests,
        hits,
        shutdown_tx: Some(shutdown_tx),
        task,
    }
}

async fn mock_chat_completions(
    State(state): State<MockState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    state.hits.fetch_add(1, Ordering::Relaxed);
    state.requests.lock().await.push(CapturedChatRequest {
        body,
        authorization: headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
    });

    match state.mode {
        MockProviderMode::Json { status, ref body } => {
            (StatusCode::from_u16(status).unwrap_or(StatusCode::OK), Json(body.clone()))
                .into_response()
        }
        MockProviderMode::Sse { status, ref body } => {
            let mut response = axum::response::Response::new(Body::from(body.clone()));
            *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/event-stream"),
            );
            response
        }
    }
}

#[derive(Debug, Clone)]
struct CapturedChatRequest {
    body: Value,
    authorization: Option<String>,
}

pub fn write_key_file(value: &str) -> PathBuf {
    let mut file = NamedTempFile::new().expect("key file should be created");
    file.write_all(value.as_bytes()).expect("key file should be written");
    file.into_temp_path().keep().expect("key file path should persist")
}

pub fn write_runtime_config(content: &str) -> PathBuf {
    let mut file = NamedTempFile::new().expect("config file should be created");
    file.write_all(content.as_bytes()).expect("config file should be written");
    file.into_temp_path().keep().expect("config file path should persist")
}
