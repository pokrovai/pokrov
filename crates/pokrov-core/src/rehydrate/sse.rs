//! SSE-aware restore pass shared by the buffered and passthrough streaming
//! paths.
//!
//! Upstream bytes are buffered into complete events (split on blank lines,
//! LF/CRLF/CR line endings alike) so `data:` payloads that parse as JSON can
//! be restored leaf-wise and re-serialized — a restored fragment containing
//! `"`, `\`, or a newline is correctly escaped by serde and can never inject
//! a phantom SSE event. `choices[].delta.content` and
//! `response.output_text.delta` strings join a per-stream carry keyed by
//! choice index, so a token the model splits across two delta events still
//! resolves before reaching the client and interleaved `n`-choice streams
//! never cross-contaminate. Chat Completions mode additionally parses
//! `choices[].delta.tool_calls[].function.arguments` as inner JSON and holds
//! only a bounded trailing token prefix per `(choice, tool)` pair. Non-JSON
//! lines restore as plain text under a newline guard that reverts any
//! substitution which would break SSE framing.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{
    count_unrestored_markers, holdback_index, rehydrate_text, rehydrate_value, RehydrateReport,
    RehydrationMap,
};

mod stream;
mod tool_arguments;

pub use stream::SseStreamRehydrator;
pub use tool_arguments::has_ambiguous_chat_tool_members;

/// Byte budget for one unterminated SSE event. A stream that never emits a
/// blank-line terminator would grow the rehydration buffer without bound;
/// past the cap the buffered bytes are flushed verbatim so tokens inside
/// stay unresolved (fail-visible) and count as unrestored markers.
pub const MAX_EVENT_BUFFER_BYTES: usize = 1 << 20;

/// Distinct delta legs (`choices[].index`, responses text) tracked
/// simultaneously. Beyond the cap content is restored without carry join —
/// tokens split across extra legs stay visible and count as unrestored.
const MAX_CARRY_KEYS: usize = 128;

/// End index (exclusive) of the first complete SSE event in `buffer`:
/// content lines terminated by a blank line. Recognizes LF, CRLF, and
/// lone-CR line endings, so `\r\n\r\n` and `\r\r` terminate events just as
/// `\n\n` does.
pub fn event_boundary_end(buffer: &[u8]) -> Option<usize> {
    let mut cursor = 0;
    while cursor < buffer.len() {
        let line_end = match buffer[cursor] {
            b'\n' => cursor + 1,
            b'\r' if buffer.get(cursor + 1) == Some(&b'\n') => cursor + 2,
            b'\r' => cursor + 1,
            _ => {
                cursor += 1;
                continue;
            }
        };
        match buffer.get(line_end) {
            Some(&b'\n') => return Some(line_end + 1),
            Some(&b'\r') => {
                return Some(if buffer.get(line_end + 1) == Some(&b'\n') {
                    line_end + 2
                } else {
                    line_end + 1
                });
            }
            _ => cursor = line_end,
        }
    }
    None
}

/// Identity of a text stream whose token carry is tracked independently.
/// Multi-choice (`n > 1`) chat chunks share one event but stream one delta
/// per `choices[].index`; mixing their carries would corrupt both.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CarryKey {
    /// `choices[].index` of a `chat.completion.chunk` stream.
    ChatChoice(i64),
    /// Top-level `delta` text of `response.output_text.delta` events
    /// emitted by the buffered `/v1/responses` conversion.
    ResponsesText,
}

/// Where a detached content string must be written back after restore.
enum Slot {
    /// `choices[<position>].delta.content` inside the same JSON event.
    Choice(usize),
    /// Top-level `delta` of a `response.output_text.delta` event.
    ResponsesDelta,
}

/// Stateful per-event rehydrator. Owns the request map so it can live inside
/// a `'static` response stream; the buffered path wraps it around a cloned
/// map instead.
pub struct EventRehydrator {
    map: RehydrationMap,
    delta_carry: BTreeMap<CarryKey, String>,
    restored_total: u32,
    unrestored_total: u32,
    /// Present only for streaming Chat Completions responses; holds the
    /// bounded per-pair inner-JSON parsers and suffix envelope.
    chat_arguments: Option<tool_arguments::ArgumentStreams>,
}

impl EventRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self {
            map,
            delta_carry: BTreeMap::new(),
            restored_total: 0,
            unrestored_total: 0,
            chat_arguments: None,
        }
    }

    /// Chat Completions mode: identical to [`Self::new`] for text fields,
    /// plus bounded incremental restoration of tokens inside
    /// `choices[].delta.tool_calls[].function.arguments` inner JSON.
    pub fn new_chat_completions(map: RehydrationMap) -> Self {
        Self { chat_arguments: Some(tool_arguments::ArgumentStreams::new()), ..Self::new(map) }
    }

    /// Cumulative restore counters over all processed events.
    pub fn report(&self) -> RehydrateReport {
        RehydrateReport { restored: self.restored_total, unrestored: self.unrestored_total }
    }

    /// Whether any argument pair retains a trailing token prefix. Parser
    /// records alone do not count as pending output.
    pub(crate) fn has_pending_arguments(&self) -> bool {
        self.chat_arguments.as_ref().is_some_and(tool_arguments::ArgumentStreams::has_pending)
    }

    /// Whether this rehydrator was built for streaming Chat Completions.
    pub(crate) fn is_chat_completions(&self) -> bool {
        self.chat_arguments.is_some()
    }

    /// Processes one SSE event block (without its blank-line terminator) and
    /// returns the event text. When a stream leg terminates (`[DONE]`, a
    /// `finish_reason` choice, or a responses `*.done`/lifecycle event) the
    /// pending token-prefix carries for the terminated legs are flushed ahead
    /// of it as synthetic delta events.
    pub fn rehydrate_event(&mut self, block: &str) -> String {
        if self.chat_arguments.is_some() {
            return tool_arguments::rehydrate_chat_event(self, block);
        }
        let mut terminal = false;
        let mut prelude: Vec<String> = Vec::new();
        let mut lines = Vec::new();

        for line in block.lines() {
            let Some(data) = line.strip_prefix("data:") else {
                lines.push(self.rehydrate_plain_line(line));
                continue;
            };
            let payload = data.trim();
            if payload == "[DONE]" {
                terminal = true;
                lines.push(line.to_string());
                continue;
            }
            match serde_json::from_str::<Value>(payload) {
                Ok(json) => {
                    let (rendered, flushed) = self.rehydrate_json_event(line, json);
                    prelude.extend(flushed);
                    lines.push(rendered);
                }
                Err(_) => lines.push(self.rehydrate_plain_line(line)),
            }
        }

        if terminal {
            prelude.extend(self.flush_all_carries());
        }

        let event = lines.join("\n");
        if prelude.is_empty() {
            return event;
        }
        format!("{}\n\n{event}", prelude.join("\n\n"))
    }

    /// Emits every pending token-prefix carry as synthetic delta events when
    /// the stream ended without a terminal event to flush them through.
    pub fn finish(&mut self) -> Vec<String> {
        if self.chat_arguments.is_some() {
            return tool_arguments::finish_chat(self);
        }
        self.flush_all_carries()
    }

    /// Restores a JSON `data:` payload leaf-wise. Delta content strings are
    /// detached before the generic pass so carry-merged output is never
    /// re-scanned, then reattached afterwards. Returns the rendered line and
    /// any carry flushes triggered by terminal legs in this event.
    fn rehydrate_json_event(&mut self, line: &str, mut json: Value) -> (String, Vec<String>) {
        let mut terminal_keys: Vec<CarryKey> = Vec::new();
        let mut saved_contents: Vec<(Slot, CarryKey, String)> = Vec::new();

        if let Some(choices) = json.get_mut("choices").and_then(Value::as_array_mut) {
            for (position, choice) in choices.iter_mut().enumerate() {
                let key = CarryKey::ChatChoice(
                    choice.get("index").and_then(Value::as_i64).unwrap_or(position as i64),
                );
                if choice.get("finish_reason").is_some_and(|reason| !reason.is_null()) {
                    terminal_keys.push(key);
                }
                let content = choice
                    .pointer_mut("/delta/content")
                    .and_then(|value| value.as_str().map(str::to_string));
                if let Some(content) = content {
                    if let Some(slot) = choice.pointer_mut("/delta/content") {
                        *slot = Value::String(String::new());
                    }
                    saved_contents.push((Slot::Choice(position), key, content));
                }
            }
        } else if json.get("type").and_then(Value::as_str) == Some("response.output_text.delta") {
            let content = json.get("delta").and_then(|value| value.as_str().map(str::to_string));
            if let Some(content) = content {
                if let Some(slot) = json.get_mut("delta") {
                    *slot = Value::String(String::new());
                }
                saved_contents.push((Slot::ResponsesDelta, CarryKey::ResponsesText, content));
            }
        }
        if let Some(event_type) = json.get("type").and_then(Value::as_str) {
            if is_terminal_responses_type(event_type) {
                terminal_keys.push(CarryKey::ResponsesText);
            }
        }

        let (mut restored, leaf) = rehydrate_value(json, &self.map);
        self.accumulate(leaf);
        let mut changed = leaf.restored > 0;

        for (slot, key, content) in saved_contents {
            let processed = self.process_delta_content(key, &content);
            changed |= processed != content;
            let target = match slot {
                Slot::Choice(position) => {
                    restored.pointer_mut(&format!("/choices/{position}/delta/content"))
                }
                Slot::ResponsesDelta => restored.pointer_mut("/delta"),
            };
            if let Some(target) = target {
                *target = Value::String(processed);
            }
        }

        let flushed = self.flush_carries(&terminal_keys);
        match serde_json::to_string(&restored) {
            Ok(encoded) if changed => (format!("data: {encoded}"), flushed),
            // Byte-faithful output when nothing was restored.
            _ => (line.to_string(), flushed),
        }
    }

    /// Joins the carry for `key` with the next delta `content`, holds back
    /// the trailing proper token prefix, and restores complete tokens in the
    /// emittable part. Restoring the carried merge would otherwise expose a
    /// token that spans two SSE delta events as unrestored. Past
    /// `MAX_CARRY_KEYS` distinct legs the carry join is skipped so a hostile
    /// index spray cannot grow the map without bound.
    fn process_delta_content(&mut self, key: CarryKey, content: &str) -> String {
        if !self.delta_carry.contains_key(&key) && self.delta_carry.len() >= MAX_CARRY_KEYS {
            let (restored, report) = rehydrate_text(content, &self.map);
            self.accumulate(report);
            return restored;
        }
        let mut combined = self.delta_carry.remove(&key).unwrap_or_default();
        combined.push_str(content);
        let cut = holdback_index(combined.as_bytes(), &self.map);
        let (emit, tail) = combined.split_at(cut);
        if !tail.is_empty() {
            self.delta_carry.insert(key, tail.to_string());
        }
        let (restored, report) = rehydrate_text(emit, &self.map);
        self.accumulate(report);
        restored
    }

    /// Drains the carries of the given (terminated) legs into synthetic
    /// events; chat legs share one `chat.completion.chunk` carrying every
    /// pending index, responses text flushes as `response.output_text.delta`.
    /// Flushed carries are incomplete token prefixes — markers emitted
    /// unrestored, so they are counted as such.
    fn flush_carries(&mut self, keys: &[CarryKey]) -> Vec<String> {
        let mut carries = Vec::new();
        for key in keys {
            if let Some(carry) = self.delta_carry.remove(key) {
                if !carry.is_empty() {
                    self.note_unrestored_markers(carry.as_bytes());
                    carries.push((*key, carry));
                }
            }
        }
        synthesize_carry_events(carries)
    }

    /// Drains every pending carry — used at `[DONE]`, end of stream, and
    /// ahead of a forwarded upstream error.
    fn flush_all_carries(&mut self) -> Vec<String> {
        let carries: Vec<(CarryKey, String)> = std::mem::take(&mut self.delta_carry)
            .into_iter()
            .filter(|(_, carry)| !carry.is_empty())
            .collect();
        for (_, carry) in &carries {
            self.note_unrestored_markers(carry.as_bytes());
        }
        synthesize_carry_events(carries)
    }

    /// Counts `__PKV_` markers in bytes emitted unrestored (verbatim
    /// overflow, error-path passthrough, carry flushes).
    fn note_unrestored_markers(&mut self, bytes: &[u8]) {
        self.unrestored_total =
            self.unrestored_total.saturating_add(count_unrestored_markers(bytes, &self.map));
    }

    /// Restores a non-JSON line. When a substituted fragment would introduce
    /// a raw CR/LF — splitting the event or creating a phantom one — the
    /// substitution is reverted so the marker stays visible instead.
    fn rehydrate_plain_line(&mut self, line: &str) -> String {
        let (restored, report) = rehydrate_text(line, &self.map);
        self.accumulate(report);
        if report.restored > 0 && (restored.contains('\n') || restored.contains('\r')) {
            self.restored_total -= report.restored;
            self.unrestored_total += report.restored;
            return line.to_string();
        }
        restored
    }

    fn accumulate(&mut self, report: RehydrateReport) {
        self.restored_total = self.restored_total.saturating_add(report.restored);
        self.unrestored_total = self.unrestored_total.saturating_add(report.unrestored);
    }
}

/// Whether a `/v1/responses` lifecycle event ends the text stream: terminal
/// states and per-item `*.done` markers.
fn is_terminal_responses_type(event_type: &str) -> bool {
    event_type.ends_with(".done")
        || matches!(event_type, "response.completed" | "response.failed" | "response.incomplete")
}

/// Wraps pending carry bytes in synthetic delta events so they reach the
/// client as ordinary content instead of vanishing.
fn synthesize_carry_events(carries: Vec<(CarryKey, String)>) -> Vec<String> {
    let mut chat_choices = Vec::new();
    let mut events = Vec::new();
    for (key, carry) in carries {
        match key {
            CarryKey::ChatChoice(index) => {
                chat_choices.push(serde_json::json!({"index": index, "delta": {"content": carry}}))
            }
            CarryKey::ResponsesText => events.push(format!(
                "data: {}",
                serde_json::json!({"type": "response.output_text.delta", "delta": carry})
            )),
        }
    }
    if !chat_choices.is_empty() {
        let event = serde_json::json!({"object": "chat.completion.chunk", "choices": chat_choices});
        events.insert(0, format!("data: {}", event));
    }
    events
}

#[cfg(test)]
mod tests;
