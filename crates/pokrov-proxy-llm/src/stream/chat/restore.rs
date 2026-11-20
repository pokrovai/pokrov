//! Buffered Chat SSE restoration pass.
//!
//! Each complete post-policy frame is handed to the core stream driver as a
//! complete event. Source-affected frames bypass restoration with the carry
//! semantics that match live delivery, so policy normalization growth cannot
//! change the restore/bypass decision.

use pokrov_core::rehydrate::{
    event_boundary_end, RehydrateReport, RehydrationMap, SseStreamRehydrator,
};

use super::policy::ChatPolicyBody;

/// Why a complete source frame must bypass buffered restoration. An
/// oversized frame takes the oversize path even when it is also ambiguous,
/// because live overflow flushes both tool and text carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarrierKind {
    Ambiguous,
    Oversize,
}

/// Restores Chat tool arguments after policy evaluation. Complete frames are
/// fed as complete events so policy normalization growth cannot push them over
/// the size-based `feed` budget. Ambiguous barriers use tool-carry bypass;
/// oversized barriers flush tool and text carries. The driver owns
/// absolute-stream-start BOM handling; the body is passed through unchanged so
/// a second BOM stays significant. Complete frames and their synthetic carries
/// are emitted first; an unterminated raw tail is appended verbatim afterwards
/// so a held text carry can never place a blank-line terminator after it and
/// turn it into a dispatchable event.
pub(crate) fn rehydrate_chat_sse_stream(
    body: ChatPolicyBody,
    map: RehydrationMap,
) -> (String, RehydrateReport) {
    if map.is_empty() {
        return (body.body, RehydrateReport::default());
    }

    let bytes = body.body.as_bytes();
    let barriers = body.source_barriers;

    let mut rehydrator = SseStreamRehydrator::new_chat_completions(map);
    let mut output = Vec::with_capacity(body.body.len());
    let mut barrier_index = 0usize;
    let mut cursor = 0usize;
    while let Some(offset) = event_boundary_end(&bytes[cursor..]) {
        let end = cursor + offset;
        let barrier_kind = match barriers.get(barrier_index) {
            Some((source_barrier, kind)) if cursor == source_barrier.start => {
                assert_eq!(end, source_barrier.end, "source barrier must align with one frame");
                barrier_index += 1;
                Some(*kind)
            }
            _ => None,
        };
        match barrier_kind {
            Some(BarrierKind::Ambiguous) => {
                output.extend(rehydrator.bypass_complete_event(&bytes[cursor..end]))
            }
            Some(BarrierKind::Oversize) => {
                output.extend(rehydrator.bypass_source_oversize_event(&bytes[cursor..end]))
            }
            None => output.extend(rehydrator.feed_complete_event(&bytes[cursor..end])),
        }
        cursor = end;
    }
    assert_eq!(barrier_index, barriers.len(), "every source barrier must be visited");

    output.extend(rehydrator.finish());
    let tail = &bytes[cursor..];
    if !tail.is_empty() {
        // Feed then drain so the driver observes every raw-tail byte and
        // counts literal markers. The tail has no complete boundary, and
        // finish() already flushed pending carries, so drain() returns only
        // those bytes with no synthetic separators.
        output.extend(rehydrator.feed(tail));
        output.extend(rehydrator.drain());
    }

    let report = rehydrator.report();
    let restored = String::from_utf8(output)
        .expect("buffered Chat restoration only rewrites valid UTF-8 string values");
    (restored, report)
}
