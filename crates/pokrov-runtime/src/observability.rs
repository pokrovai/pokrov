use tracing_subscriber::{fmt, fmt::MakeWriter, EnvFilter};

pub fn init_json_observability(default_level: &str) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    let subscriber = fmt::Subscriber::builder()
        .json()
        .with_env_filter(env_filter)
        .with_current_span(false)
        .with_target(false)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub fn json_subscriber_with_writer<W>(
    default_level: &str,
    writer: W,
) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter = EnvFilter::new(default_level);

    fmt::Subscriber::builder()
        .json()
        .with_env_filter(env_filter)
        .with_current_span(false)
        .with_target(false)
        .with_writer(writer)
        .finish()
}

pub fn log_lifecycle_event(component: &str, action: &str, request_id: Option<&str>, state: &str) {
    tracing::info!(
        component = component,
        action = action,
        request_id = request_id.unwrap_or("system"),
        state = state
    );
}

pub fn log_evaluate_event(
    request_id: &str,
    profile_id: &str,
    final_action: pokrov_core::types::PolicyAction,
    rule_hits_total: u32,
) {
    tracing::info!(
        component = "sanitization",
        action = "evaluate",
        request_id = request_id,
        profile_id = profile_id,
        final_action = ?final_action,
        rule_hits_total
    );
}

pub fn log_auth_stage_event(request_id: &str, auth_mode: &str, stage: &str, decision: &str) {
    tracing::info!(
        component = "runtime",
        action = "auth_stage",
        request_id = request_id,
        auth_mode = auth_mode,
        stage = stage,
        decision = decision
    );
}
