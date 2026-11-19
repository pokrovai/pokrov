//! Chat Completions argument restoration: per-`(choice, tool)` parsers,
//! bounded admission/fences, and synthetic suffix chunks.
//!
//! `inner_json` holds the bounded inner-JSON lexer/emitter, `shape` plans
//! event association, `streams` owns admission/fences and suffix rendering,
//! and `event` drives the per-event detach/restore/reattach pipeline. `sse.rs`
//! only selects this mode and delegates.

use crate::rehydrate::RehydrateReport;
use std::borrow::Cow;

mod event;
mod inner_json;
mod shape;
mod streams;

pub(crate) use event::{finish_chat, flush_pending_arguments, rehydrate_chat_event};
pub(crate) use inner_json::ArgumentDeltaState;
pub use shape::has_ambiguous_chat_tool_members;
pub(crate) use streams::ArgumentStreams;

/// Live argument pairs tracked per response, separate from text carry.
pub(crate) const MAX_ARGUMENT_PAIRS: usize = 32;

/// Completed or quarantined choice IDs retained as index-only fences.
pub(crate) const MAX_CHOICE_FENCES: usize = 32;

/// Nested object/array levels, excluding the root sentinel.
pub(crate) const MAX_INNER_DEPTH: usize = 32;

/// Retained token prefix per pair; a strict prefix is always shorter than a
/// complete token.
pub(crate) const MAX_PENDING_PREFIX: usize = 81;

/// Longest minted token: `__PKV_` + 64 hex + `_c<8hex>` + `__`.
pub(crate) const MAX_TOKEN_BYTES: usize = 82;

/// Combined choice and tool-call descriptors inspected per event.
pub(crate) const MAX_EVENT_ENTRIES: usize = 128;

/// Largest `id`/`model` value copied into a synthetic-chunk envelope.
pub(crate) const MAX_ENVELOPE_FIELD_BYTES: usize = 256;

/// Identity of one tool-call argument stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArgumentKey {
    pub(crate) choice: u64,
    pub(crate) tool: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArgumentOutcome {
    Trusted,
    Opaque,
}

pub(crate) struct ArgumentDelta<'a> {
    pub(crate) text: Cow<'a, str>,
    pub(crate) report: RehydrateReport,
    pub(crate) outcome: ArgumentOutcome,
}

#[cfg(test)]
mod tests;
