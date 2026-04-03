use axum::{
    body::Body,
    extract::{rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use pokrov_proxy_llm::types::LLMProxyBody;
use serde_json::Value;

use crate::{app::AppState, error::ApiError};

pub async fn handle_chat_completions(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Response, ApiError> {
    let payload = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;

    let token = parse_bearer_token(&headers)
        .ok_or_else(|| ApiError::unauthorized(request_id.clone(), "missing bearer authorization"))?;

    let api_key_profile = state.sanitization.profile_for_token(token).ok_or_else(|| {
        ApiError::unauthorized(request_id.clone(), "invalid API key or profile binding")
    })?;

    let handler = state.llm.handler.clone().ok_or_else(|| {
        ApiError::invalid_request(request_id.clone(), "llm proxy is not configured")
    })?;

    let response = handler
        .handle_chat_completion(request_id.clone(), payload, &api_key_profile)
        .await
        .map_err(ApiError::from_llm_proxy)?;

    match response.body {
        LLMProxyBody::Json(body) => Ok((response.status, Json(body)).into_response()),
        LLMProxyBody::Sse(body) => {
            let mut sse_response = Response::new(Body::from(body));
            *sse_response.status_mut() = response.status;
            sse_response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            sse_response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            );
            sse_response.headers_mut().insert(
                header::CONNECTION,
                HeaderValue::from_static("keep-alive"),
            );

            Ok(sse_response)
        }
        LLMProxyBody::SseStream(stream) => {
            let mut sse_response = Response::new(Body::from_stream(stream));
            *sse_response.status_mut() = response.status;
            sse_response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            sse_response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            );
            sse_response.headers_mut().insert(
                header::CONNECTION,
                HeaderValue::from_static("keep-alive"),
            );

            Ok(sse_response)
        }
    }
}

fn parse_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn map_json_rejection(request_id: String, rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::BytesRejection(_) => {
            ApiError::payload_too_large(request_id, "request body exceeds configured size limit")
        }
        _ => ApiError::invalid_request(request_id, "invalid request body"),
    }
}
