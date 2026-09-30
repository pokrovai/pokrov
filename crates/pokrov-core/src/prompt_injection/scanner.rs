use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

use super::types::{
    PromptInjectionAction, PromptInjectionClassification, PromptInjectionDecision,
    PromptInjectionDetection, PromptInjectionDetector, PromptInjectionDetectorError,
    PromptInjectionFailMode, PromptInjectionMode, PromptInjectionOutcome, PromptInjectionPolicy,
    PromptInjectionSource,
};

/// Evaluates untrusted content through a pluggable detector and resolves the
/// effective decision from policy: score → classification → action/mode/fail
/// mode. The scanner owns source gating, size limits and the inference time
/// budget; providers own tokenization and fragment splitting.
pub struct PromptInjectionScanner {
    policy: PromptInjectionPolicy,
    /// `None` when the provider could not be constructed during bootstrap
    /// under `fail_open`; every scan then resolves as a degraded outcome.
    detector: Option<Arc<dyn PromptInjectionDetector>>,
    /// Single in-flight inference slot. A timed-out worker keeps running to
    /// completion while holding the slot, so concurrent scans fail fast as
    /// `detector_busy` instead of queueing OS threads on a saturated session.
    inference_slot: Arc<AtomicBool>,
}

/// RAII guard that releases the single-flight slot when the worker thread
/// finishes — including unwind from a detector panic.
struct InferenceSlot(Arc<AtomicBool>);

impl Drop for InferenceSlot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl PromptInjectionScanner {
    pub fn new(
        policy: PromptInjectionPolicy,
        detector: Option<Arc<dyn PromptInjectionDetector>>,
    ) -> Self {
        Self { policy, detector, inference_slot: Arc::new(AtomicBool::new(false)) }
    }

    pub const fn policy(&self) -> &PromptInjectionPolicy {
        &self.policy
    }

    /// Scans `text` when `source` is enabled in policy; returns `None` for
    /// disabled sources (no evaluation happened, nothing to audit).
    ///
    /// This call blocks the current thread for up to `timeout_ms` while the
    /// detector runs on a worker thread; async callers should wrap it in
    /// `tokio::task::spawn_blocking`.
    pub fn scan(
        &self,
        source: PromptInjectionSource,
        text: &str,
    ) -> Option<PromptInjectionOutcome> {
        let source_policy =
            self.policy.sources.get(&source).filter(|policy| policy.enabled)?;
        let threshold = source_policy.threshold.unwrap_or(self.policy.threshold);
        let started = Instant::now();

        // Empty content is deterministically benign; skipping inference keeps
        // the hot path free of pointless model calls.
        if text.trim().is_empty() {
            return Some(self.evaluated_outcome(
                source,
                threshold,
                PromptInjectionDetection { score: 0.0, chunks_processed: 0 },
                started.elapsed(),
            ));
        }

        if text.len() > self.policy.max_content_bytes {
            return Some(self.failure_outcome(
                source,
                threshold,
                "content_exceeds_max_bytes",
                started.elapsed(),
            ));
        }

        let Some(detector) = self.detector.clone() else {
            return Some(self.failure_outcome(
                source,
                threshold,
                "detector_unavailable",
                started.elapsed(),
            ));
        };

        // The detector runs on a worker thread that outlives the caller's
        // timeout; without a single-flight gate, each timed-out scan would
        // leave a zombie inference behind and pile threads onto one session.
        if self.inference_slot.swap(true, Ordering::AcqRel) {
            return Some(self.failure_outcome(
                source,
                threshold,
                "detector_busy",
                started.elapsed(),
            ));
        }
        let slot = InferenceSlot(Arc::clone(&self.inference_slot));

        match detect_with_timeout(detector, text, self.policy.timeout_ms, slot) {
            Ok(detection) => {
                Some(self.evaluated_outcome(source, threshold, detection, started.elapsed()))
            }
            Err(error) => Some(self.failure_outcome(
                source,
                threshold,
                reason_code(&error),
                started.elapsed(),
            )),
        }
    }

    /// Degraded outcome for callers whose blocking detector task could not be
    /// joined (panic in `spawn_blocking`); equivalent to an unavailable
    /// detector.
    pub fn task_failure_outcome(&self, source: PromptInjectionSource) -> PromptInjectionOutcome {
        let threshold = self.threshold_for(source);
        self.failure_outcome(source, threshold, "detector_task_failed", Duration::ZERO)
    }

    /// Threshold that would apply to `source` without running the scan.
    pub fn threshold_for(&self, source: PromptInjectionSource) -> f32 {
        self.policy
            .sources
            .get(&source)
            .and_then(|policy| policy.threshold)
            .unwrap_or(self.policy.threshold)
    }

    fn evaluated_outcome(
        &self,
        source: PromptInjectionSource,
        threshold: f32,
        detection: PromptInjectionDetection,
        duration: Duration,
    ) -> PromptInjectionOutcome {
        let classification = if detection.score >= threshold {
            PromptInjectionClassification::Injection
        } else {
            PromptInjectionClassification::Benign
        };
        let detected = classification == PromptInjectionClassification::Injection;
        let would_block = detected && self.policy.action == PromptInjectionAction::Block;
        let decision = if would_block && self.policy.mode == PromptInjectionMode::Enforce {
            PromptInjectionDecision::Block
        } else {
            PromptInjectionDecision::Allow
        };

        let (detector_id, provider, model_id) = self.detector_labels();
        PromptInjectionOutcome {
            source,
            detector_id,
            provider,
            model_id,
            classification: Some(classification),
            score: Some(detection.score),
            threshold,
            decision,
            would_block,
            fail_mode: self.policy.fail_mode,
            degraded: false,
            degraded_reason: None,
            chunks_processed: detection.chunks_processed,
            duration_ms: duration.as_millis() as u64,
        }
    }

    fn failure_outcome(
        &self,
        source: PromptInjectionSource,
        threshold: f32,
        reason: &'static str,
        duration: Duration,
    ) -> PromptInjectionOutcome {
        // `would_block` records what enforce mode would have done, so dry-run
        // telemetry still surfaces latent fail-closed blocks.
        let would_block = self.policy.fail_mode == PromptInjectionFailMode::FailClosed;
        let decision = if would_block && self.policy.mode == PromptInjectionMode::Enforce {
            PromptInjectionDecision::Block
        } else {
            PromptInjectionDecision::Allow
        };

        let (detector_id, provider, model_id) = self.detector_labels();
        PromptInjectionOutcome {
            source,
            detector_id,
            provider,
            model_id,
            classification: None,
            score: None,
            threshold,
            decision,
            would_block,
            fail_mode: self.policy.fail_mode,
            degraded: true,
            degraded_reason: Some(reason.to_string()),
            chunks_processed: 0,
            duration_ms: duration.as_millis() as u64,
        }
    }

    fn detector_labels(&self) -> (String, String, String) {
        match self.detector.as_ref() {
            Some(detector) => {
                let descriptor = detector.descriptor();
                (
                    descriptor.detector_id.clone(),
                    descriptor.provider.clone(),
                    descriptor.model_id.clone(),
                )
            }
            None => ("none".to_string(), "none".to_string(), "none".to_string()),
        }
    }
}

/// Runs the synchronous detector on a dedicated worker thread so a hung or
/// over-budget inference cannot stall the caller past `timeout_ms`. Mirrors
/// the isolation strategy used by the NER adapter.
fn detect_with_timeout(
    detector: Arc<dyn PromptInjectionDetector>,
    text: &str,
    timeout_ms: u64,
    slot: InferenceSlot,
) -> Result<PromptInjectionDetection, PromptInjectionDetectorError> {
    let owned = text.to_string();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        // The slot clears on drop when the worker exits — normally after
        // `detect`, or during unwind if the detector panics.
        let _slot = slot;
        let _ = tx.send(detector.detect(&owned));
    });

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(PromptInjectionDetectorError::Timeout(timeout_ms))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(
            PromptInjectionDetectorError::InferenceFailed(
                "detector worker disconnected".to_string(),
            ),
        ),
    }
}

fn reason_code(error: &PromptInjectionDetectorError) -> &'static str {
    match error {
        PromptInjectionDetectorError::Unavailable(_) => "detector_unavailable",
        PromptInjectionDetectorError::Timeout(_) => "detector_timeout",
        PromptInjectionDetectorError::InferenceFailed(_) => "detector_inference_failed",
        PromptInjectionDetectorError::ContentTruncated => "content_truncated",
    }
}
