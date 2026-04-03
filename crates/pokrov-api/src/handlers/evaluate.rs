use axum::{
    extract::{rejection::JsonRejection, Extension, State},
    http::{header, HeaderMap},
    Json,
};
use pokrov_core::types::{
    AuditSummary, EvaluateError, EvaluateRequest, EvaluationMode, ExplainSummary, PathClass,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{app::AppState, error::ApiError};

#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateHttpRequest {
    pub profile_id: String,
    pub mode: EvaluationMode,
    #[serde(default)]
    pub path_class: PathClass,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluateHttpResponse {
    pub request_id: String,
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub final_action: pokrov_core::types::PolicyAction,
    pub executed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitized_payload: Option<Value>,
    pub explain: ExplainSummary,
    pub audit: AuditSummary,
}

pub async fn handle_evaluate(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    headers: HeaderMap,
    body: Result<Json<EvaluateHttpRequest>, JsonRejection>,
) -> Result<Json<EvaluateHttpResponse>, ApiError> {
    let body = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;

    let token = parse_bearer_token(&headers)
        .ok_or_else(|| ApiError::unauthorized(request_id.clone(), "missing bearer authorization"))?;

    if !state.sanitization.is_authorized(token, &body.profile_id) {
        return Err(ApiError::unauthorized(
            request_id.clone(),
            "invalid API key or profile binding",
        ));
    }

    let evaluator = state
        .sanitization
        .evaluator
        .as_ref()
        .ok_or_else(|| ApiError::invalid_profile(request_id.clone(), "sanitization is not configured"))?;

    let result = evaluator
        .evaluate(EvaluateRequest {
            request_id: request_id.clone(),
            profile_id: body.profile_id.clone(),
            mode: body.mode,
            payload: body.payload,
            path_class: body.path_class,
        })
        .map_err(|error| map_evaluate_error(request_id.clone(), error))?;

    state.metrics.on_rule_hits(result.decision.rule_hits_total);
    state.metrics.on_payload_transformed(result.transform.transformed_fields_count);
    if result.transform.blocked {
        state.metrics.on_evaluation_blocked();
    }

    tracing::info!(
        component = "sanitization",
        action = "evaluate",
        request_id = %result.request_id,
        profile_id = %result.profile_id,
        final_action = ?result.decision.final_action,
        rule_hits_total = result.decision.rule_hits_total
    );

    Ok(Json(EvaluateHttpResponse {
        request_id: result.request_id,
        profile_id: result.profile_id,
        mode: result.mode,
        final_action: result.decision.final_action,
        executed: result.executed,
        sanitized_payload: result.transform.sanitized_payload,
        explain: result.explain,
        audit: result.audit,
    }))
}

fn parse_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    header.strip_prefix("Bearer ").map(str::trim).filter(|token| !token.is_empty())
}

fn map_evaluate_error(request_id: String, error: EvaluateError) -> ApiError {
    match error {
        EvaluateError::InvalidProfile(message) => ApiError::invalid_profile(request_id, message),
        EvaluateError::InvalidRequest(message) => ApiError::invalid_request(request_id, message),
        EvaluateError::InvalidProfileConfig(message) => ApiError::invalid_profile(request_id, message),
    }
}

fn map_json_rejection(request_id: String, rejection: JsonRejection) -> ApiError {
    ApiError::invalid_request(request_id, rejection.body_text())
}
