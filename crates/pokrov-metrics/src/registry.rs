use std::sync::atomic::{AtomicU64, Ordering};

use crate::hooks::{LifecycleEvent, RuntimeMetricsHooks};

#[derive(Debug, Default)]
pub struct RuntimeMetricsRegistry {
    starting_total: AtomicU64,
    ready_total: AtomicU64,
    draining_total: AtomicU64,
    stopped_total: AtomicU64,
    requests_started_total: AtomicU64,
    requests_finished_total: AtomicU64,
}

impl RuntimeMetricsRegistry {
    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            starting_total: self.starting_total.load(Ordering::Relaxed),
            ready_total: self.ready_total.load(Ordering::Relaxed),
            draining_total: self.draining_total.load(Ordering::Relaxed),
            stopped_total: self.stopped_total.load(Ordering::Relaxed),
            requests_started_total: self.requests_started_total.load(Ordering::Relaxed),
            requests_finished_total: self.requests_finished_total.load(Ordering::Relaxed),
        }
    }
}

impl RuntimeMetricsHooks for RuntimeMetricsRegistry {
    fn on_lifecycle_event(&self, event: LifecycleEvent) {
        match event {
            LifecycleEvent::Starting => {
                self.starting_total.fetch_add(1, Ordering::Relaxed);
            }
            LifecycleEvent::Ready => {
                self.ready_total.fetch_add(1, Ordering::Relaxed);
            }
            LifecycleEvent::Draining => {
                self.draining_total.fetch_add(1, Ordering::Relaxed);
            }
            LifecycleEvent::Stopped => {
                self.stopped_total.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn on_request_started(&self) {
        self.requests_started_total.fetch_add(1, Ordering::Relaxed);
    }

    fn on_request_finished(&self) {
        self.requests_finished_total.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RuntimeMetricsSnapshot {
    pub starting_total: u64,
    pub ready_total: u64,
    pub draining_total: u64,
    pub stopped_total: u64,
    pub requests_started_total: u64,
    pub requests_finished_total: u64,
}
