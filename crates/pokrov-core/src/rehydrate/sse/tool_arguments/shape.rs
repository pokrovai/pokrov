//! Chat Completions event preflight: source-level duplicate-member detection
//! and a bounded plan of detached `function.arguments` values.
//!
//! `preflight` detects duplicate members along the argument-association
//! paths, `duplicates` builds the bounded per-event plan, and `bypass` counts
//! markers in malformed tool-call values. Descriptors record array positions
//! only for writeback; identities always come from numeric `index` members.
//! The whole event is inspected before any argument is mutated so a later
//! duplicate cannot invalidate an earlier restore.

mod bypass;
mod duplicates;
mod preflight;

pub(crate) use bypass::count_bypassed_argument_markers;
pub(crate) use duplicates::plan_event;
pub use preflight::has_ambiguous_chat_tool_members;

use serde_json::Value;

use super::ArgumentKey;

pub(crate) enum EventPlan {
    /// The frame cannot be associated safely; the caller forwards it raw.
    Opaque,
    Process(ProcessPlan),
}

pub(crate) struct ProcessPlan {
    /// Release every argument pair and close admissions before the event.
    pub(crate) global_gap: bool,
    pub(crate) choices: Vec<PlannedChoice>,
    pub(crate) detached: Vec<DetachedArguments>,
    pub(crate) detached_tool_calls: Vec<DetachedToolCalls>,
    pub(crate) detached_deltas: Vec<DetachedDelta>,
    /// Choices to flush and fence before the event is forwarded.
    pub(crate) flush_choices: Vec<u64>,
    /// Pairs to flush and quarantine before the event is forwarded.
    pub(crate) flush_pairs: Vec<ArgumentKey>,
}

impl ProcessPlan {
    fn empty() -> Self {
        Self {
            global_gap: false,
            choices: Vec::new(),
            detached: Vec::new(),
            detached_tool_calls: Vec::new(),
            detached_deltas: Vec::new(),
            flush_choices: Vec::new(),
            flush_pairs: Vec::new(),
        }
    }
}

pub(crate) struct PlannedChoice {
    pub(crate) index: Option<u64>,
    pub(crate) finish: bool,
    pub(crate) quarantined: bool,
    pub(crate) tools: Vec<PlannedTool>,
}

pub(crate) struct PlannedTool {
    pub(crate) key: ArgumentKey,
    pub(crate) writeback: usize,
}

pub(crate) struct DetachedArguments {
    pub(crate) choice: usize,
    pub(crate) tool: usize,
    pub(crate) value: Value,
    /// False when the value bypasses the parser (quarantined, invalid slot,
    /// or an association gap); the integration counts visible markers.
    pub(crate) process: bool,
}

pub(crate) struct DetachedToolCalls {
    pub(crate) choice: usize,
    pub(crate) value: Value,
}

pub(crate) struct DetachedDelta {
    pub(crate) choice: usize,
    pub(crate) value: Value,
}
