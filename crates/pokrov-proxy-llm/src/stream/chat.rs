//! Buffered Chat Completions output-policy pass and tool-argument restoration.
//!
//! The buffered path evaluates output policy before restoration and keeps the
//! trace payload pre-restore. Policy evaluation rewrites every parseable
//! `data:` line, which can hide source-level duplicate members or shrink an
//! oversized frame below the streaming event budget. The sanitizer records
//! every post-policy byte range of an affected complete frame; restoration
//! later bypasses each frame with the carry semantics that match live
//! delivery, which flushes pending suffixes and closes tool admission.

mod policy;
mod restore;

#[cfg(test)]
mod tests;

pub(super) use policy::evaluate_policy_payload;
pub(crate) use policy::{sanitize_chat_sse_stream, ChatPolicyBody};
pub(crate) use restore::{rehydrate_chat_sse_stream, BarrierKind};
