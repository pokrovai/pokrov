use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use pokrov_api::app::{RuntimeStateReader, RuntimeStateView};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Starting,
    Ready,
    Draining,
    Stopped,
}

impl From<RuntimeState> for RuntimeStateView {
    fn from(value: RuntimeState) -> Self {
        match value {
            RuntimeState::Starting => RuntimeStateView::Starting,
            RuntimeState::Ready => RuntimeStateView::Ready,
            RuntimeState::Draining => RuntimeStateView::Draining,
            RuntimeState::Stopped => RuntimeStateView::Stopped,
        }
    }
}

#[derive(Debug)]
pub struct RuntimeLifecycle {
    state: RwLock<RuntimeState>,
    config_loaded: AtomicBool,
    active_requests: AtomicUsize,
    shutdown_started_at: RwLock<Option<Instant>>,
}

impl RuntimeLifecycle {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(RuntimeState::Starting),
            config_loaded: AtomicBool::new(false),
            active_requests: AtomicUsize::new(0),
            shutdown_started_at: RwLock::new(None),
        }
    }

    pub async fn state(&self) -> RuntimeState {
        *self.state.read().await
    }

    pub async fn set_config_loaded(&self, loaded: bool) {
        self.config_loaded.store(loaded, Ordering::Relaxed);
    }

    pub async fn mark_ready(&self) {
        *self.state.write().await = RuntimeState::Ready;
    }

    pub async fn mark_draining(&self) {
        *self.state.write().await = RuntimeState::Draining;
        *self.shutdown_started_at.write().await = Some(Instant::now());
    }

    pub async fn mark_stopped(&self) {
        *self.state.write().await = RuntimeState::Stopped;
    }

    pub async fn wait_for_drain(&self, timeout: Duration) {
        let started_at = Instant::now();
        while self.active_requests.load(Ordering::Relaxed) > 0 {
            if started_at.elapsed() >= timeout {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub fn config_loaded(&self) -> bool {
        self.config_loaded.load(Ordering::Relaxed)
    }

    pub fn active_requests(&self) -> usize {
        self.active_requests.load(Ordering::Relaxed)
    }

    pub fn increment_requests(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_requests(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeStateReader for RuntimeLifecycle {
    fn state(&self) -> RuntimeStateView {
        let state = self.state.try_read().map(|guard| *guard).unwrap_or(RuntimeState::Starting);
        state.into()
    }

    fn config_loaded(&self) -> bool {
        self.config_loaded()
    }

    fn active_requests(&self) -> usize {
        self.active_requests()
    }

    fn on_request_started(&self) {
        self.increment_requests();
    }

    fn on_request_finished(&self) {
        self.decrement_requests();
    }
}

pub type SharedRuntimeLifecycle = Arc<RuntimeLifecycle>;

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{RuntimeLifecycle, RuntimeState};

    #[tokio::test]
    async fn supports_starting_ready_draining_stopped_transitions() {
        let lifecycle = RuntimeLifecycle::new();
        assert_eq!(lifecycle.state().await, RuntimeState::Starting);

        lifecycle.set_config_loaded(true).await;
        lifecycle.mark_ready().await;
        assert_eq!(lifecycle.state().await, RuntimeState::Ready);

        lifecycle.mark_draining().await;
        assert_eq!(lifecycle.state().await, RuntimeState::Draining);

        lifecycle.mark_stopped().await;
        assert_eq!(lifecycle.state().await, RuntimeState::Stopped);
    }

    #[tokio::test]
    async fn wait_for_drain_respects_timeout_when_requests_are_active() {
        let lifecycle = RuntimeLifecycle::new();
        lifecycle.increment_requests();

        let started = Instant::now();
        lifecycle.wait_for_drain(Duration::from_millis(50)).await;
        let elapsed = started.elapsed();

        assert!(elapsed >= Duration::from_millis(50));
        assert!(lifecycle.active_requests() > 0);
    }
}
