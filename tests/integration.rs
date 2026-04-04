#[path = "integration/readiness_shutdown_flow.rs"]
mod readiness_shutdown_flow;
#[path = "common/llm_proxy_test_support.rs"]
pub mod llm_proxy_test_support;
#[path = "common/mcp_test_support.rs"]
pub mod mcp_test_support;
#[path = "common/hardening_test_support.rs"]
pub mod hardening_test_support;
#[path = "integration/request_id_logging_flow.rs"]
mod request_id_logging_flow;
#[path = "integration/llm_proxy_block_path.rs"]
mod llm_proxy_block_path;
#[path = "integration/llm_proxy_body_limit_path.rs"]
mod llm_proxy_body_limit_path;
#[path = "integration/llm_proxy_happy_path.rs"]
mod llm_proxy_happy_path;
#[path = "integration/llm_proxy_output_sanitization_path.rs"]
mod llm_proxy_output_sanitization_path;
#[path = "integration/llm_proxy_routing_path.rs"]
mod llm_proxy_routing_path;
#[path = "integration/llm_proxy_stream_output_sanitization_path.rs"]
mod llm_proxy_stream_output_sanitization_path;
#[path = "integration/llm_proxy_streaming_path.rs"]
mod llm_proxy_streaming_path;
#[path = "integration/llm_proxy_route_and_stream_contract_path.rs"]
mod llm_proxy_route_and_stream_contract_path;
#[path = "integration/llm_proxy_upstream_error_path.rs"]
mod llm_proxy_upstream_error_path;
#[path = "integration/llm_proxy_byok_passthrough_happy_path.rs"]
mod llm_proxy_byok_passthrough_happy_path;
#[path = "integration/llm_proxy_byok_passthrough_missing_credential_path.rs"]
mod llm_proxy_byok_passthrough_missing_credential_path;
#[path = "integration/llm_proxy_gateway_auth_failure_path.rs"]
mod llm_proxy_gateway_auth_failure_path;
#[path = "integration/llm_proxy_byok_invalid_provider_credential_path.rs"]
mod llm_proxy_byok_invalid_provider_credential_path;
#[path = "integration/llm_proxy_chat_completions_regression.rs"]
mod llm_proxy_chat_completions_regression;
#[path = "integration/responses_compat_happy_path.rs"]
mod responses_compat_happy_path;
#[path = "integration/responses_policy_block_path.rs"]
mod responses_policy_block_path;
#[path = "integration/responses_stream_happy_path.rs"]
mod responses_stream_happy_path;
#[path = "integration/responses_stream_malformed_chunk_path.rs"]
mod responses_stream_malformed_chunk_path;
#[path = "integration/responses_auth_missing_upstream_credential.rs"]
mod responses_auth_missing_upstream_credential;
#[path = "integration/responses_gateway_auth_failure.rs"]
mod responses_gateway_auth_failure;
#[path = "integration/responses_passthrough_single_bearer_path.rs"]
mod responses_passthrough_single_bearer_path;
#[path = "integration/mcp_allowed_tool_path.rs"]
mod mcp_allowed_tool_path;
#[path = "integration/mcp_argument_validation_path.rs"]
mod mcp_argument_validation_path;
#[path = "integration/mcp_validation_recovery_path.rs"]
mod mcp_validation_recovery_path;
#[path = "integration/mcp_blocked_tool_path.rs"]
mod mcp_blocked_tool_path;
#[path = "integration/mcp_output_sanitization_path.rs"]
mod mcp_output_sanitization_path;
#[path = "integration/mcp_pilot_subset_path.rs"]
mod mcp_pilot_subset_path;
#[path = "integration/mcp_invoke_path_tool_name_path.rs"]
mod mcp_invoke_path_tool_name_path;
#[path = "integration/mcp_upstream_unavailable_path.rs"]
mod mcp_upstream_unavailable_path;
#[path = "integration/sanitization_audit_explain_flow.rs"]
mod sanitization_audit_explain_flow;
#[path = "integration/sanitization_evaluate_flow.rs"]
mod sanitization_evaluate_flow;
#[path = "integration/sanitization_transform_flow.rs"]
mod sanitization_transform_flow;
#[path = "integration/startup_config_flow.rs"]
mod startup_config_flow;
#[path = "integration/bootstrap_acceptance_contract.rs"]
mod bootstrap_acceptance_contract;
#[path = "integration/rate_limit_request_budget_path.rs"]
mod rate_limit_request_budget_path;
#[path = "integration/rate_limit_token_budget_path.rs"]
mod rate_limit_token_budget_path;
#[path = "integration/blocked_metrics_profile_label_path.rs"]
mod blocked_metrics_profile_label_path;
#[path = "integration/hardening_metrics_flow.rs"]
mod hardening_metrics_flow;
#[path = "integration/hardening_metrics_degradation_path.rs"]
mod hardening_metrics_degradation_path;
#[path = "integration/hardening_dry_run_observability_path.rs"]
mod hardening_dry_run_observability_path;
#[path = "integration/hardening_degraded_shutdown_flow.rs"]
mod hardening_degraded_shutdown_flow;
#[path = "integration/hardening_end_to_end_release_flow.rs"]
mod hardening_end_to_end_release_flow;
#[path = "integration/hardening_release_evidence_fail_path.rs"]
mod hardening_release_evidence_fail_path;
#[path = "integration/byok_identity_rate_limit_isolation_path.rs"]
mod byok_identity_rate_limit_isolation_path;
#[path = "integration/byok_identity_policy_binding_path.rs"]
mod byok_identity_policy_binding_path;
#[path = "integration/byok_end_to_end_flow.rs"]
mod byok_end_to_end_flow;
#[path = "integration/mesh_mtls_gateway_auth_path.rs"]
mod mesh_mtls_gateway_auth_path;
