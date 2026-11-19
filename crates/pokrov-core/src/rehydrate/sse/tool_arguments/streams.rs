//! Bounded argument state: per-pair parsers, admission, fences, and suffix
//! records rendered as synthetic chunks.

use super::{
    ArgumentDelta, ArgumentDeltaState, ArgumentKey, ArgumentOutcome, MAX_ARGUMENT_PAIRS,
    MAX_CHOICE_FENCES, MAX_ENVELOPE_FIELD_BYTES,
};
use crate::rehydrate::{count_unrestored_markers, RehydrateReport, RehydrationMap};
use serde_json::Value;
use std::borrow::Cow;

#[derive(Clone, Default)]
pub(super) struct Envelope {
    pub(super) id: Option<String>,
    pub(super) model: Option<String>,
    pub(super) created: Option<serde_json::Number>,
}

/// Borrowed per-event envelope fields. Cloned only when the first suffix is
/// actually retained.
pub(super) struct EnvelopeCandidate<'a> {
    id: Option<&'a str>,
    model: Option<&'a str>,
    created: Option<&'a serde_json::Number>,
    bounded: bool,
}

impl<'a> EnvelopeCandidate<'a> {
    pub(super) fn new(json: &'a Value) -> Self {
        let mut bounded = true;
        let id = match json.get("id") {
            None => None,
            Some(Value::String(value)) if value.len() <= MAX_ENVELOPE_FIELD_BYTES => {
                Some(value.as_str())
            }
            Some(_) => {
                bounded = false;
                None
            }
        };
        let model = match json.get("model") {
            None => None,
            Some(Value::String(value)) if value.len() <= MAX_ENVELOPE_FIELD_BYTES => {
                Some(value.as_str())
            }
            Some(_) => {
                bounded = false;
                None
            }
        };
        let created = match json.get("created") {
            None => None,
            Some(Value::Number(number)) => Some(number),
            Some(_) => {
                bounded = false;
                None
            }
        };
        Self { id, model, created, bounded }
    }

    fn capture(&self) -> Option<Envelope> {
        if !self.bounded {
            return None;
        }
        Some(Envelope {
            id: self.id.map(str::to_string),
            model: self.model.map(str::to_string),
            created: self.created.cloned(),
        })
    }
}

impl Envelope {
    fn extend_json(&self, object: &mut serde_json::Map<String, Value>) {
        if let Some(id) = &self.id {
            object.insert("id".to_string(), Value::String(id.clone()));
        }
        if let Some(created) = &self.created {
            object.insert("created".to_string(), Value::Number(created.clone()));
        }
        if let Some(model) = &self.model {
            object.insert("model".to_string(), Value::String(model.clone()));
        }
    }
}

pub(crate) struct ArgumentStreams {
    pub(super) streams: Vec<(ArgumentKey, ArgumentDeltaState)>,
    pub(super) closed_pairs: Vec<ArgumentKey>,
    pub(super) choice_fences: Vec<u64>,
    pub(super) admissions_open: bool,
    pub(super) envelope: Option<Envelope>,
}

impl ArgumentStreams {
    pub(crate) fn new() -> Self {
        Self {
            streams: Vec::new(),
            closed_pairs: Vec::new(),
            choice_fences: Vec::new(),
            admissions_open: true,
            envelope: None,
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.streams.iter().any(|(_, state)| state.has_pending())
    }

    pub(crate) fn is_choice_fenced(&self, choice: u64) -> bool {
        self.choice_fences.contains(&choice)
    }

    pub(crate) fn is_pair_closed(&self, key: ArgumentKey) -> bool {
        self.closed_pairs.contains(&key)
    }

    /// Feeds one detached argument fragment for `key`, admitting the pair
    /// when capacity and fences allow. A fault releases the pre-field prefix
    /// and quarantines the pair unless `fence_choice` supersedes it.
    pub(super) fn push<'a>(
        &mut self,
        key: ArgumentKey,
        input: &'a str,
        map: &RehydrationMap,
        candidate: &EnvelopeCandidate<'_>,
        fence_choice: bool,
    ) -> ArgumentDelta<'a> {
        if self.is_choice_fenced(key.choice) || self.is_pair_closed(key) {
            return bypass(input, map);
        }
        let index = if let Some(index) = self.streams.iter().position(|(c, _)| *c == key) {
            index
        } else {
            if !self.admissions_open || self.streams.len() >= MAX_ARGUMENT_PAIRS {
                self.admissions_open = false;
                return bypass(input, map);
            }
            self.streams.push((key, ArgumentDeltaState::new()));
            self.streams.len() - 1
        };
        let mut delta = self.streams[index].1.push(input, map);
        if delta.outcome == ArgumentOutcome::Opaque {
            self.streams.remove(index);
            if !fence_choice {
                self.close_pair(key);
            }
            return delta;
        }
        if self.streams[index].1.has_pending() && self.envelope.is_none() {
            if let Some(envelope) = candidate.capture() {
                self.envelope = Some(envelope);
            } else if let Some((suffix, unrestored)) = self.streams[index].1.take_pending() {
                // No bounded envelope: the suffix cannot be delivered in a
                // later synthetic chunk, so release it literally now.
                delta.report.unrestored = delta.report.unrestored.saturating_add(unrestored);
                let mut text = String::with_capacity(delta.text.len() + suffix.len());
                text.push_str(&delta.text);
                text.push_str(&suffix);
                delta.text = Cow::Owned(text);
            }
        }
        delta
    }

    /// Removes a pair's state and returns its held suffix for same-field append.
    pub(crate) fn take_final_suffix(&mut self, key: ArgumentKey) -> Option<(String, u32)> {
        let index = self.streams.iter().position(|(candidate, _)| *candidate == key)?;
        let (_, mut state) = self.streams.remove(index);
        state.take_pending()
    }

    /// Flushes a pair's held suffix; `persist_tombstone` records the closure.
    pub(crate) fn flush_pair_records(
        &mut self,
        key: ArgumentKey,
        persist_tombstone: bool,
    ) -> Vec<(ArgumentKey, String, u32)> {
        let mut records = Vec::new();
        if let Some(index) = self.streams.iter().position(|(candidate, _)| *candidate == key) {
            let (_, mut state) = self.streams.remove(index);
            if let Some((text, unrestored)) = state.take_pending() {
                records.push((key, text, unrestored));
            }
        }
        if persist_tombstone {
            self.close_pair(key);
        }
        records
    }

    /// Flushes and fences one choice; rendering orders its records by tool.
    pub(crate) fn flush_choice_records(&mut self, choice: u64) -> Vec<(ArgumentKey, String, u32)> {
        let mut records = Vec::new();
        let mut index = 0;
        while index < self.streams.len() {
            if self.streams[index].0.choice == choice {
                let key = self.streams[index].0;
                let (_, mut state) = self.streams.remove(index);
                if let Some((text, unrestored)) = state.take_pending() {
                    records.push((key, text, unrestored));
                }
            } else {
                index += 1;
            }
        }
        self.fence_choice(choice);
        records
    }

    /// Pre-reclaims tombstones for a choice that will be fenced later in the
    /// current event, so unrelated quarantines cannot exhaust the pair cap
    /// before the fence frees those slots.
    pub(crate) fn reclaim_choice_tombstones(&mut self, choice: u64) {
        self.closed_pairs.retain(|key| key.choice != choice);
    }

    /// Releases every held suffix record and permanently closes admissions.
    pub(super) fn flush_all_records(&mut self) -> Vec<(ArgumentKey, String, u32)> {
        let mut records = Vec::new();
        for (key, mut state) in std::mem::take(&mut self.streams) {
            if let Some((text, unrestored)) = state.take_pending() {
                records.push((key, text, unrestored));
            }
        }
        self.admissions_open = false;
        records
    }

    /// Renders records into one synthetic chunk per choice. Records are
    /// sorted by `(choice, tool)` so terminal and ambiguous flushes follow
    /// deterministic index order.
    pub(super) fn render_suffixes(
        &self,
        mut records: Vec<(ArgumentKey, String, u32)>,
    ) -> Vec<String> {
        records.sort_by_key(|(key, _, _)| (key.choice, key.tool));
        let mut chunks = Vec::new();
        let mut iter = records.into_iter().peekable();
        while let Some((key, text, _)) = iter.next() {
            let mut tools = vec![(key.tool, text)];
            while iter.peek().is_some_and(|(next, _, _)| next.choice == key.choice) {
                let (next, text, _) = iter.next().expect("peeked");
                tools.push((next.tool, text));
            }
            chunks.push(self.synthetic_chunk(key.choice, &tools));
        }
        chunks
    }

    fn close_pair(&mut self, key: ArgumentKey) {
        if self.closed_pairs.contains(&key) {
            return;
        }
        if self.closed_pairs.len() >= MAX_ARGUMENT_PAIRS {
            self.admissions_open = false;
            return;
        }
        self.closed_pairs.push(key);
    }

    fn fence_choice(&mut self, choice: u64) {
        self.closed_pairs.retain(|key| key.choice != choice);
        if self.choice_fences.contains(&choice) {
            return;
        }
        if self.choice_fences.len() >= MAX_CHOICE_FENCES {
            self.admissions_open = false;
            return;
        }
        self.choice_fences.push(choice);
    }

    fn synthetic_chunk(&self, choice: u64, tools: &[(u64, String)]) -> String {
        let mut object = serde_json::Map::new();
        object.insert("object".to_string(), Value::String("chat.completion.chunk".to_string()));
        if let Some(envelope) = &self.envelope {
            envelope.extend_json(&mut object);
        }
        let tool_calls: Vec<Value> = tools
            .iter()
            .map(|(index, arguments)| {
                serde_json::json!({"index": index, "function": {"arguments": arguments}})
            })
            .collect();
        object.insert(
            "choices".to_string(),
            serde_json::json!([{"index": choice, "delta": {"tool_calls": tool_calls}, "finish_reason": null}]),
        );
        format!("data: {}", Value::Object(object))
    }
}

fn bypass<'a>(input: &'a str, map: &RehydrationMap) -> ArgumentDelta<'a> {
    let unrestored = count_unrestored_markers(input.as_bytes(), map);
    ArgumentDelta {
        text: Cow::Borrowed(input),
        report: RehydrateReport { restored: 0, unrestored },
        outcome: ArgumentOutcome::Opaque,
    }
}

/// Releases every held suffix and drops the envelope after a global gap.
pub(super) fn flush_all_and_close(streams: &mut ArgumentStreams) -> (Vec<String>, RehydrateReport) {
    let records = streams.flush_all_records();
    let report = report_from_records(&records);
    let chunks = streams.render_suffixes(records);
    streams.envelope = None;
    (chunks, report)
}

fn report_from_records(records: &[(ArgumentKey, String, u32)]) -> RehydrateReport {
    let mut report = RehydrateReport::default();
    for (_, _, unrestored) in records {
        report.unrestored = report.unrestored.saturating_add(*unrestored);
    }
    report
}
