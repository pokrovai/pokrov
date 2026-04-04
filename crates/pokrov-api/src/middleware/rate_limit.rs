use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use pokrov_config::rate_limit::{RateLimitEnforcementMode, RateLimitProfile};
use tokio::sync::Mutex;

use crate::app::{RateLimitDecision, RateLimitReason, RateLimitWindowState};

const WINDOW_DURATION: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RateLimitWindowKind {
    Requests,
    TokenUnits,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RateLimitWindowKey {
    api_key_id: String,
    profile_id: String,
    kind: RateLimitWindowKind,
}

#[derive(Debug)]
struct BucketResult {
    exceeded: bool,
    limit: u32,
    remaining: u32,
    retry_after_ms: u64,
    reset_at_unix_ms: u64,
}

pub struct RateLimiter {
    default_profile: String,
    profiles: BTreeMap<String, RateLimitProfile>,
    windows: Mutex<HashMap<RateLimitWindowKey, RateLimitWindowState>>,
}

impl RateLimiter {
    pub fn new(default_profile: String, profiles: BTreeMap<String, RateLimitProfile>) -> Self {
        Self {
            default_profile,
            profiles,
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub async fn evaluate(
        &self,
        api_key_id: &str,
        profile_id: &str,
        token_units: u32,
    ) -> RateLimitDecision {
        self.evaluate_at(
            api_key_id,
            profile_id,
            token_units,
            Instant::now(),
            SystemTime::now(),
        )
        .await
    }

    async fn evaluate_at(
        &self,
        api_key_id: &str,
        profile_id: &str,
        token_units: u32,
        now_monotonic: Instant,
        now_wall_clock: SystemTime,
    ) -> RateLimitDecision {
        let profile = self
            .profiles
            .get(profile_id)
            .or_else(|| self.profiles.get(&self.default_profile));

        let Some(profile) = profile else {
            return RateLimitDecision {
                allowed: true,
                reason: RateLimitReason::WithinBudget,
                retry_after_ms: 0,
                limit: 0,
                remaining: 0,
                reset_at_unix_ms: unix_ms(now_wall_clock),
                enforcement_mode: RateLimitEnforcementMode::Enforce,
            };
        };

        let mode = profile.enforcement_mode;
        let request_limit = effective_limit(profile.requests_per_minute, profile.burst_multiplier);
        let token_limit = effective_limit(profile.token_units_per_minute, profile.burst_multiplier);
        let token_units = token_units.max(1);

        let mut windows = self.windows.lock().await;
        let request_key = RateLimitWindowKey {
            api_key_id: api_key_id.to_string(),
            profile_id: profile_id.to_string(),
            kind: RateLimitWindowKind::Requests,
        };
        let token_key = RateLimitWindowKey {
            api_key_id: api_key_id.to_string(),
            profile_id: profile_id.to_string(),
            kind: RateLimitWindowKind::TokenUnits,
        };

        let request_result = inspect_bucket(
            &mut windows,
            &request_key,
            request_limit,
            1,
            now_monotonic,
            now_wall_clock,
        );

        let token_result = inspect_bucket(
            &mut windows,
            &token_key,
            token_limit,
            token_units,
            now_monotonic,
            now_wall_clock,
        );

        // Enforce mode applies consumption only when both buckets pass to avoid
        // draining the request window on token-budget rejections.
        let should_consume = mode == RateLimitEnforcementMode::DryRun
            || (!request_result.exceeded && !token_result.exceeded);
        if should_consume {
            consume_bucket(&mut windows, &request_key, 1);
            consume_bucket(&mut windows, &token_key, token_units);
        }

        if request_result.exceeded {
            return RateLimitDecision {
                allowed: mode == RateLimitEnforcementMode::DryRun,
                reason: RateLimitReason::RequestBudgetExhausted,
                retry_after_ms: request_result.retry_after_ms,
                limit: request_result.limit,
                remaining: request_result.remaining,
                reset_at_unix_ms: request_result.reset_at_unix_ms,
                enforcement_mode: mode,
            };
        }

        if token_result.exceeded {
            return RateLimitDecision {
                allowed: mode == RateLimitEnforcementMode::DryRun,
                reason: RateLimitReason::TokenBudgetExhausted,
                retry_after_ms: token_result.retry_after_ms,
                limit: token_result.limit,
                remaining: token_result.remaining,
                reset_at_unix_ms: token_result.reset_at_unix_ms,
                enforcement_mode: mode,
            };
        }

        RateLimitDecision {
            allowed: true,
            reason: RateLimitReason::WithinBudget,
            retry_after_ms: 0,
            limit: request_result.limit,
            remaining: request_result.remaining,
            reset_at_unix_ms: request_result.reset_at_unix_ms,
            enforcement_mode: mode,
        }
    }
}

fn inspect_bucket(
    windows: &mut HashMap<RateLimitWindowKey, RateLimitWindowState>,
    key: &RateLimitWindowKey,
    limit: u32,
    requested: u32,
    now_monotonic: Instant,
    now_wall_clock: SystemTime,
) -> BucketResult {
    let window = windows
        .entry(key.clone())
        .or_insert_with(|| RateLimitWindowState::new(now_monotonic));
    window.reset_if_stale(now_monotonic, WINDOW_DURATION);

    let used = window.consumed;
    let remaining = limit.saturating_sub(used);
    let exceeded = requested > remaining;
    let consumed_after_request = used.saturating_add(requested);

    let elapsed = now_monotonic.duration_since(window.window_started_at);
    let retry_after_ms =
        WINDOW_DURATION.saturating_sub(elapsed).as_millis().try_into().unwrap_or(u64::MAX);
    let reset_at_unix_ms = unix_ms(now_wall_clock)
        .saturating_add(WINDOW_DURATION.saturating_sub(elapsed).as_millis() as u64);

    BucketResult {
        exceeded,
        limit,
        remaining: if exceeded { 0 } else { limit.saturating_sub(consumed_after_request) },
        retry_after_ms: if exceeded { retry_after_ms.max(1) } else { 0 },
        reset_at_unix_ms,
    }
}

fn consume_bucket(
    windows: &mut HashMap<RateLimitWindowKey, RateLimitWindowState>,
    key: &RateLimitWindowKey,
    requested: u32,
) {
    if let Some(window) = windows.get_mut(key) {
        window.consumed = window.consumed.saturating_add(requested);
    }
}

fn effective_limit(base_limit: u32, burst_multiplier: f32) -> u32 {
    ((base_limit as f64) * (burst_multiplier as f64))
        .round()
        .max(1.0) as u32
}

fn unix_ms(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{RateLimitWindowKey, RateLimitWindowKind, RateLimiter};
    use pokrov_config::rate_limit::{RateLimitEnforcementMode, RateLimitProfile};
    use std::{
        collections::BTreeMap,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    #[tokio::test]
    async fn blocks_when_request_budget_is_exhausted_in_enforce_mode() {
        let limiter = limiter_with_profile(2, 10, RateLimitEnforcementMode::Enforce);
        let now = Instant::now();
        let wall = UNIX_EPOCH + Duration::from_secs(1);

        let first = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        assert!(first.allowed);

        let second = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        assert!(second.allowed);

        let third = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        assert!(!third.allowed);
        assert_eq!(
            third.reason,
            crate::app::RateLimitReason::RequestBudgetExhausted
        );
        assert!(third.retry_after_ms > 0);
    }

    #[tokio::test]
    async fn allows_when_dry_run_budget_is_exhausted() {
        let limiter = limiter_with_profile(1, 1, RateLimitEnforcementMode::DryRun);
        let now = Instant::now();
        let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(5);

        let _ = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        let second = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;

        assert!(second.allowed);
        assert_eq!(
            second.reason,
            crate::app::RateLimitReason::RequestBudgetExhausted
        );
    }

    #[tokio::test]
    async fn resets_window_after_sixty_seconds() {
        let limiter = limiter_with_profile(1, 100, RateLimitEnforcementMode::Enforce);
        let now = Instant::now();
        let wall = UNIX_EPOCH + Duration::from_secs(10);
        let _ = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        let blocked = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        assert!(!blocked.allowed);

        let allowed = limiter
            .evaluate_at(
                "k1",
                "strict",
                1,
                now + Duration::from_secs(61),
                wall + Duration::from_secs(61),
            )
            .await;
        assert!(allowed.allowed);
    }

    #[tokio::test]
    async fn does_not_consume_request_budget_when_token_budget_rejects() {
        let limiter = limiter_with_profile(3, 3, RateLimitEnforcementMode::Enforce);
        let now = Instant::now();
        let wall = UNIX_EPOCH + Duration::from_secs(20);

        let first = limiter
            .evaluate_at("k1", "strict", 3, now, wall)
            .await;
        assert!(first.allowed);

        let second = limiter
            .evaluate_at("k1", "strict", 1, now, wall)
            .await;
        assert!(!second.allowed);
        assert_eq!(
            second.reason,
            crate::app::RateLimitReason::TokenBudgetExhausted
        );

        let windows = limiter.windows.lock().await;
        let request_window = windows
            .get(&RateLimitWindowKey {
                api_key_id: "k1".to_string(),
                profile_id: "strict".to_string(),
                kind: RateLimitWindowKind::Requests,
            })
            .expect("request window should be present");
        assert_eq!(request_window.consumed, 1);
    }

    fn limiter_with_profile(
        requests_per_minute: u32,
        token_units_per_minute: u32,
        enforcement_mode: RateLimitEnforcementMode,
    ) -> RateLimiter {
        RateLimiter::new(
            "strict".to_string(),
            BTreeMap::from([(
                "strict".to_string(),
                RateLimitProfile {
                    requests_per_minute,
                    token_units_per_minute,
                    burst_multiplier: 1.0,
                    enforcement_mode,
                },
            )]),
        )
    }
}
