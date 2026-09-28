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
use futures_util::StreamExt;
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
    Json {
        status: u16,
        body: Value,
    },
    Sse {
        status: u16,
        body: String,
    },
    /// Emits the response as separate HTTP chunks with an inter-chunk delay,
    /// so stream-boundary behavior (e.g. tokens split mid-chunk) is
    /// exercised deterministically.
    SseChunked {
        status: u16,
        chunks: Vec<String>,
        delay_ms: u64,
    },
    /// Streams the given chunks and then fails the response body mid-flight
    /// so the client observes a truncated stream; exercises upstream-error
    /// tail flushing on the passthrough path. `delay_ms` separates the last
    /// data chunk from the error so response headers reliably flush first.
    SseAbort {
        status: u16,
        chunks: Vec<String>,
        delay_ms: u64,
    },
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
        MockProviderMode::SseChunked { status, ref chunks, delay_ms } => {
            let chunks = chunks.clone();
            let stream = futures_util::stream::iter(chunks.into_iter().enumerate()).then(
                move |(index, chunk)| async move {
                    if index > 0 && delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    Ok::<_, std::io::Error>(bytes::Bytes::from(chunk))
                },
            );
            let mut response = axum::response::Response::new(Body::from_stream(stream));
            *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/event-stream"),
            );
            response
        }
        MockProviderMode::SseAbort { status, ref chunks, delay_ms } => {
            let chunks = chunks.clone();
            let data = futures_util::stream::iter(
                chunks.into_iter().map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::from(chunk))),
            );
            // A body-stream error makes hyper abort the chunked response
            // without a terminating chunk — the proxy sees a stream error. The
            // delay lets headers and the first chunk flush to the client first.
            let stream = data.chain(futures_util::stream::once(async move {
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                Err::<bytes::Bytes, _>(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "mock upstream abort",
                ))
            }));
            let mut response = axum::response::Response::new(Body::from_stream(stream));
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
