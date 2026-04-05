use axum::{
    extract::{rejection::JsonRejection, Extension, State},
    http::HeaderMap,
    Json,
};
use pokrov_core::types::{
    AuditSummary, DegradedSummary, EvaluateError, EvaluateRequest, EvaluationMode, ExecutedSummary,
    ExplainSummary, PathClass, PolicyAction,
};
use pokrov_config::GatewayAuthMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{app::AppState, auth::parse_bearer_token, error::ApiError};
use crate::app::{GatewayAuthContext, GatewayAuthMechanism};

/// HTTP payload for the sanitization evaluate route.
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateHttpRequest {
    pub profile_id: String,
    pub mode: EvaluationMode,
    #[serde(default)]
    pub path_class: PathClass,
    #[serde(default)]
    pub effective_language: Option<String>,
    pub payload: Value,
}

/// HTTP response for the sanitization evaluate route.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluateHttpResponse {
    pub request_id: String,
    pub profile_id: String,
    pub mode: EvaluationMode,
    pub final_action: PolicyAction,
    pub executed: ExecutedSummary,
    pub degraded: DegradedSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitized_payload: Option<Value>,
    pub explain: ExplainSummary,
    pub audit: AuditSummary,
}

/// Handles metadata-safe evaluation requests for the sanitization API.
pub async fn handle_evaluate(
    State(state): State<AppState>,
    Extension(request_id): Extension<String>,
    Extension(gateway_auth): Extension<GatewayAuthContext>,
    headers: HeaderMap,
    body: Result<Json<EvaluateHttpRequest>, JsonRejection>,
) -> Result<Json<EvaluateHttpResponse>, ApiError> {
    let body = body
        .map(|Json(body)| body)
        .map_err(|rejection| map_json_rejection(request_id.clone(), rejection))?;

    if !gateway_auth.authenticated {
        return Err(ApiError::gateway_unauthorized(request_id.clone()));
    }

    if matches!(state.auth.gateway_auth_mode, GatewayAuthMode::ApiKey) {
        let token = parse_bearer_token(&headers).ok_or_else(|| {
            ApiError::unauthorized(request_id.clone(), "missing bearer authorization")
        })?;

        if !state.sanitization.is_authorized(token, &body.profile_id) {
            return Err(ApiError::unauthorized(
                request_id.clone(),
                "invalid API key or profile binding",
            ));
        }
    } else if matches!(gateway_auth.auth_mechanism, Some(GatewayAuthMechanism::Bearer)) {
        // In mTLS gateway modes Authorization is upstream-only; bearer gateway auth must be disabled.
        return Err(ApiError::gateway_unauthorized(request_id.clone()));
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
            effective_language: body
                .effective_language
                .unwrap_or_else(|| "en".to_string()),
            entity_scope_filters: Vec::new(),
            recognizer_family_filters: Vec::new(),
            allowlist_additions: Vec::new(),
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
        degraded: result.degraded,
        sanitized_payload: result.transform.sanitized_payload,
        explain: result.explain,
        audit: result.audit,
    }))
}

fn map_evaluate_error(request_id: String, error: EvaluateError) -> ApiError {
    match error {
        EvaluateError::InvalidProfile(message) => ApiError::invalid_profile(request_id, message),
        EvaluateError::InvalidInput(message) => ApiError::invalid_request(request_id, message),
        EvaluateError::RuntimeFailure(message) => ApiError::internal(request_id, message),
    }
}

fn map_json_rejection(request_id: String, _rejection: JsonRejection) -> ApiError {
    ApiError::invalid_request(request_id, "invalid request body")
}
