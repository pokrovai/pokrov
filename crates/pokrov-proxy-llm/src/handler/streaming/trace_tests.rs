//! Feature-gated evidence that the buffered response trace sees the
//! post-policy, pre-restore body while the client receives restored bytes,
//! and that the live passthrough route never adds a response payload trace.

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use futures_util::StreamExt;
use pokrov_config::{
    model::{
        LlmConfig, LlmDefaultsConfig, LlmProviderAuthConfig, LlmProviderConfig, LlmRouteConfig,
        ResponseMetadataMode,
    },
    UpstreamAuthMode,
};
use pokrov_core::{
    rehydrate::{RehydrationMap, TokenDeriver},
    types::{
        CategoryActions, CustomRule, DetectionCategory, EvaluationMode, EvaluatorConfig,
        PolicyAction, PolicyProfile,
    },
    SanitizationEngine,
};
use pokrov_metrics::hooks::NoopRuntimeMetricsHooks;
use serde_json::Value;

use crate::{
    handler::LLMProxyHandler,
    routing::ProviderRouteTable,
    stream::{rehydrate_chat_sse_stream, ChatPolicyBody},
    trace::LlmPayloadTraceSink,
    types::{LLMProxyBody, RouteResolution, SseBodyStream, UpstreamCredentialOrigin},
};

const TOKEN_KEY: &[u8] = b"buffered-chat-trace-test-key";
const TOKEN_KEY_TEXT: &str = "buffered-chat-trace-test-key";
const ORIGINAL: &str = "acme\"corp";
const CHAT_ENDPOINT: &str = "/v1/chat/completions";
const TRACE_MODEL: &str = "gpt-4o-mini";

static TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn trace_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let sequence = TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!("pokrov-llm-response-trace-{}-{stamp}-{sequence}.ndjson", std::process::id()))
}

fn route() -> RouteResolution {
    RouteResolution {
        provider_id: "openai".to_string(),
        provider_profile_id: None,
        base_url: "http://127.0.0.1:1".to_string(),
        effective_upstream_path: "/chat/completions".to_string(),
        canonical_model: "gpt-4o-mini".to_string(),
        resolved_via_alias: false,
        api_key: None,
        timeout_ms: 30_000,
        retry_budget: 0,
        output_sanitization: true,
        stream_sanitization_max_buffer_bytes: 1024 * 1024,
    }
}

fn handler_with_trace(path: &Path) -> LLMProxyHandler {
    let routes = ProviderRouteTable::from_config(
        &LlmConfig {
            providers: Vec::new(),
            routes: Vec::new(),
            defaults: LlmDefaultsConfig {
                profile_id: "strict".to_string(),
                output_sanitization: true,
                stream_sanitization_max_buffer_bytes: 1024 * 1024,
            },
        },
        &BTreeMap::new(),
    )
    .expect("empty route table should build");

    LLMProxyHandler::new(
        None,
        Arc::new(NoopRuntimeMetricsHooks),
        routes,
        ResponseMetadataMode::Enabled,
        Some(
            LlmPayloadTraceSink::new(path.to_str().expect("trace path should be utf-8"))
                .expect("trace sink should open"),
        ),
    )
    .expect("handler should build")
}

fn event(arguments: &str) -> String {
    let payload = serde_json::json!({
        "id": "chatcmpl-trace",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": 0,
                "id": "call_trace",
                "type": "function",
                "function": {"name": "lookup", "arguments": arguments}
            }]},
            "finish_reason": null
        }]
    });
    format!("data: {payload}\n\n")
}

fn token_pair() -> (RehydrationMap, String) {
    let deriver = TokenDeriver::new(TOKEN_KEY);
    let mut map = RehydrationMap::new();
    let token = map.token_for(&deriver, ORIGINAL);
    (map, token)
}

fn read_trace(path: &Path) -> Vec<Value> {
    let content = std::fs::read_to_string(path).expect("trace file should be readable");
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("trace line should be JSON"))
        .collect()
}

fn assembled_arguments(body: &str) -> String {
    let mut assembled = String::new();
    for line in body.lines() {
        let Some(data) = line.strip_prefix("data:") else { continue };
        let Ok(payload) = serde_json::from_str::<Value>(data.trim()) else { continue };
        let Some(choices) = payload.get("choices").and_then(Value::as_array) else { continue };
        for choice in choices {
            let Some(calls) = choice
                .get("delta")
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for call in calls {
                if let Some(arguments) = call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(Value::as_str)
                {
                    assembled.push_str(arguments);
                }
            }
        }
    }
    assembled
}

#[tokio::test]
async fn buffered_finalizer_traces_pre_restore_body_and_restores_client_body() {
    let path = trace_path();
    let handler = handler_with_trace(&path);
    let (map, token) = token_pair();
    let split = token.len() / 2;
    let pre_restore = format!(
        "{}{}",
        event(&format!("{{\"q\":\"{}", &token[..split])),
        event(&format!("{}\"}}", &token[split..])),
    );

    let (client_body, report) = handler.finalize_buffered_sse(
        "req-trace",
        &route(),
        CHAT_ENDPOINT,
        pre_restore.clone(),
        move |body| {
            rehydrate_chat_sse_stream(
                ChatPolicyBody {
                    body,
                    source_barriers: Vec::new(),
                    rule_hits_total: 0,
                    final_action: PolicyAction::Allow,
                },
                map,
            )
        },
    );
    assert_eq!(report.restored, 1, "split token must restore exactly once");
    assert!(!client_body.contains("__PKV_"), "client body must not leak tokens");
    let arguments: Value = serde_json::from_str(&assembled_arguments(&client_body))
        .expect("restored inner arguments should stay valid JSON");
    assert_eq!(arguments["q"], ORIGINAL);

    let records = read_trace(&path);
    let traced: Vec<&Value> =
        records.iter().filter(|record| record["event"] == "llm_final_response_payload").collect();
    assert_eq!(traced.len(), 1, "buffered finalizer emits exactly one response trace");
    let payload = traced[0]["payload"].as_str().expect("trace payload should be a string");
    assert!(payload.contains(&token[..split]));
    assert!(payload.contains(&token[split..]));
    assert!(payload.contains("__PKV_"));
    assert!(!payload.contains(ORIGINAL), "trace must not see the restored original");
    assert_eq!(payload, &pre_restore);

    // The live passthrough route emits no response payload trace; only the
    // buffered finalizer above may produce `llm_final_response_payload`.
    let before_live = read_trace(&path).len();
    let stream: SseBodyStream = futures_util::stream::once(async {
        Ok::<_, reqwest::Error>(bytes::Bytes::from_static(b"data: [DONE]\n\n"))
    })
    .boxed();
    let (live_map, _) = token_pair();
    let audit = handler.stream_audit(
        CHAT_ENDPOINT,
        "req-live",
        "strict",
        "gpt-4o-mini",
        &route(),
        PolicyAction::Allow,
        0,
        200,
        Instant::now(),
        1,
        UpstreamAuthMode::Static,
        UpstreamCredentialOrigin::Config,
        0,
    );
    let live = handler.rehydrating_byte_stream(stream, live_map, audit);
    let items: Vec<_> = live.collect().await;
    assert!(items.iter().all(|item| item.is_ok()), "live restoration should succeed");
    let after_live = read_trace(&path).len();
    assert_eq!(before_live, after_live, "live route must not add a response trace");

    drop(handler);
    let _ = std::fs::remove_file(path);
}

fn trace_engine() -> SanitizationEngine {
    let profile = PolicyProfile {
        profile_id: "custom".to_string(),
        mode_default: EvaluationMode::Enforce,
        category_actions: CategoryActions {
            secrets: PolicyAction::Allow,
            pii: PolicyAction::Allow,
            corporate_markers: PolicyAction::Allow,
            custom: PolicyAction::Allow,
        },
        mask_visible_suffix: 4,
        max_hits_per_request: 4096,
        custom_rules: vec![CustomRule {
            rule_id: "custom.tool_marker".to_string(),
            category: DetectionCategory::Custom,
            pattern: ORIGINAL.to_string(),
            action: PolicyAction::Replace,
            priority: 100,
            replacement_template: Some("[PKV_TOKEN]".to_string()),
            enabled: true,
            deterministic: None,
        }],
        custom_rules_enabled: true,
        ner_enabled: false,
    };
    SanitizationEngine::new(EvaluatorConfig {
        default_profile: "custom".to_string(),
        profiles: BTreeMap::from([("custom".to_string(), profile)]),
        rehydration_key: Some(TOKEN_KEY_TEXT.to_string()),
    })
    .expect("trace engine should build")
}

fn live_handler_with_trace(path: &Path, provider_base: &str) -> LLMProxyHandler {
    let config = LlmConfig {
        providers: vec![LlmProviderConfig {
            id: "live".to_string(),
            base_url: provider_base.to_string(),
            profile_id: None,
            upstream_path: None,
            auth: LlmProviderAuthConfig::default(),
            timeout_ms: 30_000,
            retry_budget: 0,
            enabled: true,
        }],
        routes: vec![LlmRouteConfig {
            model: TRACE_MODEL.to_string(),
            provider_id: "live".to_string(),
            aliases: Vec::new(),
            output_sanitization: Some(false),
            enabled: true,
        }],
        defaults: LlmDefaultsConfig {
            profile_id: "custom".to_string(),
            output_sanitization: false,
            stream_sanitization_max_buffer_bytes: 1024 * 1024,
        },
    };
    let routes = ProviderRouteTable::from_config(&config, &BTreeMap::new())
        .expect("live route table should build");
    LLMProxyHandler::new(
        Some(Arc::new(trace_engine())),
        Arc::new(NoopRuntimeMetricsHooks),
        routes,
        ResponseMetadataMode::Enabled,
        Some(
            LlmPayloadTraceSink::new(path.to_str().expect("trace path should be utf-8"))
                .expect("trace sink should open"),
        ),
    )
    .expect("handler should build")
}

/// Minimal one-shot HTTP/1.1 SSE provider so the real live handler route can
/// run without test-only dependencies.
fn start_sse_provider(body: String) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("provider listener should bind");
    let address = listener.local_addr().expect("provider address");
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("provider should accept");
        let mut request = [0u8; 8192];
        let _ = socket.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        socket.write_all(response.as_bytes()).expect("provider should write response");
        socket.flush().expect("provider should flush response");
    });
    (format!("http://{address}/v1"), handle)
}

#[tokio::test]
async fn live_handler_route_emits_no_response_payload_trace() {
    let path = trace_path();
    let (_, token) = token_pair();
    let provider_body =
        format!("{}{}", event(&format!("{{\"q\":\"{token}\"}}")), "data: [DONE]\n\n",);
    let (provider_base, server) = start_sse_provider(provider_body);
    let handler = live_handler_with_trace(&path, &provider_base);

    let response = handler
        .handle_chat_completion(
            "req-live-handler".to_string(),
            serde_json::json!({
                "model": TRACE_MODEL,
                "stream": true,
                "messages": [{
                    "role": "user",
                    "content": format!("restore {ORIGINAL}")
                }],
                "metadata": {"profile": "custom"}
            }),
            "custom",
            UpstreamAuthMode::Static,
            None,
        )
        .await
        .expect("live handler request should succeed");
    let LLMProxyBody::SseStream(mut stream) = response.body else {
        panic!("live route must return an SSE stream");
    };
    let mut live_body = Vec::new();
    while let Some(item) = stream.next().await {
        live_body.extend_from_slice(&item.expect("live chunk should succeed"));
    }
    let live_body = String::from_utf8(live_body).expect("live SSE body should be utf-8");
    assert!(!live_body.contains("__PKV_"), "live restoration must run");
    assert!(live_body.contains("acme"), "restored original must reach the live client");

    let records = read_trace(&path);
    assert!(
        records.iter().any(|record| {
            record["request_id"] == "req-live-handler" && record["event"] == "llm_upstream_payload"
        }),
        "the trace sink must be active on the live request"
    );
    assert!(
        records.iter().all(|record| {
            !(record["request_id"] == "req-live-handler"
                && record["event"] == "llm_final_response_payload")
        }),
        "the live handler route must not emit a response payload trace"
    );

    server.join().expect("provider thread should exit");
    drop(handler);
    let _ = std::fs::remove_file(path);
}
