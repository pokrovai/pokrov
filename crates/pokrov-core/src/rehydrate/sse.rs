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
//! never cross-contaminate. Non-JSON lines restore as plain text under a
//! newline guard that reverts any substitution which would break SSE framing.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{
    count_unrestored_markers, holdback_index, rehydrate_text, rehydrate_value, RehydrateReport,
    RehydrationMap,
};

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
                // A buffer-final `\r` is inconclusive: it may be the first
                // half of a `\r\n` split across chunks. Only bytes after it
                // can settle whether this blank line actually ended.
                return if buffer.get(line_end + 1) == Some(&b'\n') {
                    Some(line_end + 2)
                } else if line_end + 1 < buffer.len() {
                    Some(line_end + 1)
                } else {
                    None
                };
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
}

impl EventRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self {
            map,
            delta_carry: BTreeMap::new(),
            restored_total: 0,
            unrestored_total: 0,
        }
    }

    /// Cumulative restore counters over all processed events.
    pub fn report(&self) -> RehydrateReport {
        RehydrateReport { restored: self.restored_total, unrestored: self.unrestored_total }
    }

    /// Processes one SSE event block (without its blank-line terminator) and
    /// returns the event text. When a stream leg terminates (`[DONE]`, a
    /// `finish_reason` choice, or a responses `*.done`/lifecycle event) the
    /// pending token-prefix carries for the terminated legs are flushed ahead
    /// of it as synthetic delta events.
    pub fn rehydrate_event(&mut self, block: &str) -> String {
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
        } else if json.get("type").and_then(Value::as_str)
            == Some("response.output_text.delta")
        {
            let content =
                json.get("delta").and_then(|value| value.as_str().map(str::to_string));
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
        self.unrestored_total = self
            .unrestored_total
            .saturating_add(count_unrestored_markers(bytes, &self.map));
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

/// Length of the trailing run of CR/LF bytes — a candidate half of an event
/// terminator that may complete only once the next chunk arrives. Kept
/// buffered instead of flushing so split boundaries are not lost.
fn trailing_eol_len(bytes: &[u8]) -> usize {
    bytes.iter().rev().take_while(|b| matches!(b, b'\r' | b'\n')).count()
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
            CarryKey::ChatChoice(index) => chat_choices
                .push(serde_json::json!({"index": index, "delta": {"content": carry}})),
            CarryKey::ResponsesText => events.push(format!(
                "data: {}",
                serde_json::json!({"type": "response.output_text.delta", "delta": carry})
            )),
        }
    }
    if !chat_choices.is_empty() {
        let event =
            serde_json::json!({"object": "chat.completion.chunk", "choices": chat_choices});
        events.insert(0, format!("data: {}", event));
    }
    events
}

/// Byte-level wrapper driving `EventRehydrator` over a chunked upstream body:
/// complete SSE events are restored as soon as their blank-line terminator
/// arrives; bytes before it stay buffered, which keeps chunk-split tokens
/// contiguous and `data:` JSON intact for leaf-wise restore.
pub struct SseStreamRehydrator {
    inner: EventRehydrator,
    buffer: Vec<u8>,
    /// Bytes before this offset are known to contain no event boundary, so
    /// `feed` resumes the terminator scan here instead of re-scanning the
    /// whole buffer on every chunk (quadratic on small chunks).
    scanned: usize,
    /// After an oversized event flushed verbatim, the remainder of that same
    /// event must pass through untouched: restoring tokens inside an
    /// already-emitted partial JSON frame could inject unescaped bytes.
    overflow_passthrough: bool,
}

impl SseStreamRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self {
            inner: EventRehydrator::new(map),
            buffer: Vec::new(),
            scanned: 0,
            overflow_passthrough: false,
        }
    }

    /// Cumulative restore counters; streaming callers report deltas against
    /// this snapshot per emitted chunk.
    pub fn report(&self) -> RehydrateReport {
        self.inner.report()
    }

    /// Whether the held buffer is empty — used by callers to decide if
    /// end-of-stream work is needed at all.
    pub fn is_idle(&self) -> bool {
        self.buffer.is_empty() && self.inner.delta_carry.is_empty()
    }

    /// Feeds one upstream chunk and returns bytes safe to emit downstream.
    /// An unterminated event past `MAX_EVENT_BUFFER_BYTES` is flushed
    /// verbatim to keep the buffer bounded; its markers count as unrestored.
    /// The remainder of that event then streams through verbatim until its
    /// blank-line terminator — a partial JSON frame already emitted can never
    /// be safely mutated afterwards.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.buffer.extend_from_slice(chunk);
        let mut out = Vec::new();
        if self.overflow_passthrough {
            match event_boundary_end(&self.buffer) {
                // The oversized event finally terminated; resume restoring
                // at the next event.
                Some(end) => {
                    self.flush_passthrough(end, &mut out);
                    self.overflow_passthrough = false;
                    self.scanned = 0;
                }
                // No terminator yet: stream the bytes out verbatim, keeping
                // memory bounded by the chunk rather than the event. The
                // trailing CR/LF run stays buffered: it may pair with the
                // next chunk's leading bytes to complete the awaited
                // terminator — a boundary split across this flush would
                // otherwise be lost.
                None => {
                    let keep = trailing_eol_len(&self.buffer);
                    self.flush_passthrough(self.buffer.len() - keep, &mut out);
                    self.scanned = self.buffer.len().saturating_sub(1);
                    return out;
                }
            }
        }
        while let Some(rel) = event_boundary_end(&self.buffer[self.scanned..]) {
            let end = self.scanned + rel;
            let block: Vec<u8> = self.buffer.drain(..end).collect();
            self.emit_block(&block, &mut out);
            // Remaining bytes were never scanned: the previous scan stopped
            // at this boundary.
            self.scanned = 0;
        }
        // The whole buffer is boundary-free; rescan from the last byte only
        // so a terminator split across the next chunk is still found.
        self.scanned = self.buffer.len().saturating_sub(1);
        if self.buffer.len() > MAX_EVENT_BUFFER_BYTES {
            let keep = trailing_eol_len(&self.buffer);
            self.flush_passthrough(self.buffer.len() - keep, &mut out);
            self.scanned = self.buffer.len().saturating_sub(1);
            self.overflow_passthrough = true;
        }
        out
    }

    /// Processes the remaining buffered tail at end of stream, then appends
    /// pending-carry flushes (synthetic delta events) when any are pending.
    pub fn finish(&mut self) -> Vec<u8> {
        let tail: Vec<u8> = self.buffer.drain(..).collect();
        self.scanned = 0;
        let mut out = Vec::new();
        if self.overflow_passthrough {
            self.inner.note_unrestored_markers(&tail);
            out.extend_from_slice(&tail);
        } else if !tail.is_empty() {
            self.emit_block(&tail, &mut out);
        }
        self.append_carry_flush(&mut out);
        out
    }

    /// Emits `len` buffered bytes verbatim, counting enclosed `__PKV_`
    /// markers as unrestored.
    fn flush_passthrough(&mut self, len: usize, out: &mut Vec<u8>) {
        let bytes: Vec<u8> = self.buffer.drain(..len).collect();
        self.inner.note_unrestored_markers(&bytes);
        out.extend_from_slice(&bytes);
    }

    /// Bytes upstream sent but not yet emitted plus pending-carry flushes —
    /// for propagation ahead of a forwarded upstream stream error so no
    /// already-received content is dropped. The raw tail goes out unrestored
    /// (a partial event cannot be safely mutated) and its markers are
    /// counted as such.
    pub fn drain(&mut self) -> Vec<u8> {
        let mut out = std::mem::take(&mut self.buffer);
        self.scanned = 0;
        self.inner.note_unrestored_markers(&out);
        self.append_carry_flush(&mut out);
        out
    }

    /// Appends synthetic delta events for every pending token-prefix carry,
    /// separated from a possibly unterminated raw tail by a blank line so the
    /// flush never merges into a partial event's last line.
    fn append_carry_flush(&mut self, out: &mut Vec<u8>) {
        let events = self.inner.finish();
        if events.is_empty() {
            return;
        }
        if !out.is_empty() && !out.ends_with(b"\n\n") {
            out.extend_from_slice(b"\n\n");
        }
        for event in events {
            out.extend_from_slice(event.as_bytes());
            out.extend_from_slice(b"\n\n");
        }
    }

    /// Emits one raw event block (blank-line terminator included when
    /// present) through the event rehydrator; the trailing CR/LF run is
    /// re-appended verbatim so original framing bytes survive.
    fn emit_block(&mut self, block: &[u8], out: &mut Vec<u8>) {
        let tail_start = block
            .iter()
            .rposition(|byte| !matches!(byte, b'\r' | b'\n'))
            .map_or(0, |index| index + 1);
        let (body, terminator) = block.split_at(tail_start);
        match std::str::from_utf8(body) {
            Ok(text) => {
                out.extend_from_slice(self.inner.rehydrate_event(text).as_bytes());
                out.extend_from_slice(terminator);
            }
            // Invalid UTF-8 cannot participate in token restore; forward raw.
            Err(_) => out.extend_from_slice(block),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rehydrate::TokenDeriver;

    fn map_with(fragment: &str) -> (RehydrationMap, String) {
        let deriver = TokenDeriver::new(b"test-key");
        let mut map = RehydrationMap::new();
        let token = map.token_for(&deriver, fragment);
        (map, token)
    }

    fn delta_event(content: &str) -> String {
        format!("data: {{\"object\":\"chat.completion.chunk\",\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}")
    }

    fn indexed_delta_event(index: i64, content: &str) -> String {
        format!("data: {{\"object\":\"chat.completion.chunk\",\"choices\":[{{\"index\":{index},\"delta\":{{\"content\":\"{content}\"}}}}]}}")
    }

    fn responses_delta_event(content: &str) -> String {
        format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"{content}\"}}")
    }

    #[test]
    fn event_rehydrator_restores_token_split_across_delta_events() {
        let (map, token) = map_with("acme-corp");
        let (first, second) = token.split_at(token.len() / 2);
        let mut rehydrator = EventRehydrator::new(map);

        // First delta ends mid-token: the carry holds the prefix back and the
        // client sees nothing yet.
        let out1 = rehydrator.rehydrate_event(&delta_event(first));
        let parsed1: Value =
            serde_json::from_str(out1.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed1["choices"][0]["delta"]["content"], "");
        assert_eq!(rehydrator.report().unrestored, 0);

        // Second delta completes the token: client receives the fragment.
        let out2 = rehydrator.rehydrate_event(&delta_event(second));
        let parsed2: Value =
            serde_json::from_str(out2.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed2["choices"][0]["delta"]["content"], "acme-corp");
        assert_eq!(rehydrator.report().restored, 1);
    }

    #[test]
    fn event_rehydrator_keeps_carries_isolated_per_choice_index() {
        let (mut map, token_zero) = map_with("acme-corp");
        let token_one = map.token_for(&TokenDeriver::new(b"test-key"), "globex");
        let (z_first, z_second) = token_zero.split_at(token_zero.len() / 2);
        let (o_first, o_second) = token_one.split_at(token_one.len() / 2);
        let mut rehydrator = EventRehydrator::new(map);

        // Interleave partial tokens of choice 0 and choice 1: each carry must
        // join only its own index.
        let out = rehydrator.rehydrate_event(&indexed_delta_event(0, z_first));
        let parsed: Value =
            serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], "");

        let out = rehydrator.rehydrate_event(&indexed_delta_event(1, o_first));
        let parsed: Value =
            serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], "");

        let out = rehydrator.rehydrate_event(&indexed_delta_event(1, o_second));
        let parsed: Value =
            serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], "globex");

        let out = rehydrator.rehydrate_event(&indexed_delta_event(0, z_second));
        let parsed: Value =
            serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], "acme-corp");
        assert_eq!(rehydrator.report().restored, 2);
    }

    #[test]
    fn event_rehydrator_flushes_only_terminated_choice_carry() {
        let (mut map, token_zero) = map_with("acme-corp");
        map.token_for(&TokenDeriver::new(b"test-key"), "globex");
        let partial_zero = &token_zero[..12];
        let mut rehydrator = EventRehydrator::new(map);

        rehydrator.rehydrate_event(&indexed_delta_event(0, partial_zero));
        rehydrator.rehydrate_event(&indexed_delta_event(1, "visible"));

        // Choice 0 finishes mid-token: its carry flushes as a synthetic event
        // with the matching index while choice 1 is unaffected.
        let out = rehydrator.rehydrate_event(
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}",
        );
        let (synthetic, terminal) = out.split_once("\n\n").unwrap();
        let parsed: Value =
            serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(parsed["choices"][0]["index"], 0);
        assert_eq!(parsed["choices"][0]["delta"]["content"], partial_zero);
        assert!(terminal.contains("finish_reason"));
    }

    #[test]
    fn event_rehydrator_carries_responses_output_text_delta() {
        let (map, token) = map_with("acme-corp");
        let (first, second) = token.split_at(token.len() / 2);
        let mut rehydrator = EventRehydrator::new(map);

        let out = rehydrator.rehydrate_event(&responses_delta_event(first));
        let parsed: Value =
            serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed["delta"], "");

        let out = rehydrator.rehydrate_event(&responses_delta_event(second));
        let parsed: Value =
            serde_json::from_str(out.strip_prefix("data: ").unwrap().trim()).unwrap();
        assert_eq!(parsed["delta"], "acme-corp");
        assert_eq!(rehydrator.report().restored, 1);
    }

    #[test]
    fn event_rehydrator_flushes_responses_carry_on_done_type() {
        let (map, token) = map_with("acme-corp");
        let partial = &token[..12];
        let mut rehydrator = EventRehydrator::new(map);

        rehydrator.rehydrate_event(&responses_delta_event(partial));
        let out = rehydrator.rehydrate_event(
            "data: {\"type\":\"response.output_text.done\",\"text\":\"done\"}",
        );

        let (synthetic, done) = out.split_once("\n\n").unwrap();
        let parsed: Value =
            serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(parsed["type"], "response.output_text.delta");
        assert_eq!(parsed["delta"], partial);
        assert!(done.contains("response.output_text.done"));
    }

    #[test]
    fn event_rehydrator_escapes_restored_fragment_inside_json() {
        let (map, token) = map_with("acme\"corp\nsuffix");
        let mut rehydrator = EventRehydrator::new(map);

        let out = rehydrator.rehydrate_event(&delta_event(&token));
        let payload = out.strip_prefix("data: ").unwrap();
        let parsed: Value = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], "acme\"corp\nsuffix");
        // The raw line must contain the escaped form, never a literal newline.
        assert!(!payload.contains('\n'));
        assert!(payload.contains("\\n"));
    }

    #[test]
    fn event_rehydrator_flushes_carry_before_done() {
        let (map, token) = map_with("acme-corp");
        let partial = &token[..10];
        let mut rehydrator = EventRehydrator::new(map);

        rehydrator.rehydrate_event(&delta_event(partial));
        let out = rehydrator.rehydrate_event("data: [DONE]");

        // The held prefix is delivered as its own delta event ahead of [DONE].
        let (synthetic, done) = out.split_once("\n\n").unwrap();
        let parsed: Value =
            serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], partial);
        assert_eq!(done, "data: [DONE]");
    }

    #[test]
    fn event_rehydrator_reverts_fragment_that_would_break_framing() {
        let (map, token) = map_with("acme\ncorp");
        let mut rehydrator = EventRehydrator::new(map);

        // Non-JSON data line: restoring would inject a real newline.
        let line = format!("data: RAW {token} TAIL");
        let out = rehydrator.rehydrate_event(&line);
        assert_eq!(out, line);
        let report = rehydrator.report();
        assert_eq!(report.restored, 0);
        assert!(report.unrestored >= 1);
    }

    #[test]
    fn stream_rehydrator_restores_token_split_across_byte_chunks() {
        let (map, token) = map_with("acme-corp");
        let event = format!("{}\n\n", delta_event(&token));
        let mut rehydrator = SseStreamRehydrator::new(map);

        let (first, second) = event.split_at(event.len() / 2 + 5);
        assert!(rehydrator.feed(first.as_bytes()).is_empty());
        let out = rehydrator.feed(second.as_bytes());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("acme-corp"), "restored body must contain the fragment");
        assert!(rehydrator.finish().is_empty());
    }

    #[test]
    fn stream_rehydrator_emits_crlf_terminated_events() {
        let (map, token) = map_with("acme-corp");
        let mut rehydrator = SseStreamRehydrator::new(map);

        // CRLF framing: the event must terminate on `\r\n\r\n`, not hang
        // waiting for `\n\n`.
        let event = format!("{}\r\n\r\n", delta_event(&token));
        let out = rehydrator.feed(event.as_bytes());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("acme-corp"), "CRLF event must restore the token");
        assert!(text.ends_with("\r\n\r\n"));
        assert!(rehydrator.finish().is_empty());
    }

    #[test]
    fn stream_rehydrator_bounds_unterminated_event_buffer() {
        let (map, token) = map_with("acme-corp");
        let mut rehydrator = SseStreamRehydrator::new(map);

        // An oversized event without a terminator flushes verbatim instead
        // of growing the buffer without bound.
        let mut flood = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 8];
        flood.extend_from_slice(token.as_bytes());
        let out = rehydrator.feed(&flood);
        assert_eq!(out.len(), flood.len());
        assert!(out.ends_with(token.as_bytes()));
        assert_eq!(rehydrator.report().unrestored, 1);
        assert!(rehydrator.is_idle());
    }

    #[test]
    fn stream_rehydrator_drain_returns_tail_and_pending_carry() {
        let (map, token) = map_with("acme-corp");
        let partial = &token[..12];
        let mut rehydrator = SseStreamRehydrator::new(map);

        // A complete event holding a partial token (carry pending) followed
        // by an unterminated event fragment.
        rehydrator.feed(format!("{}\n\n", delta_event(partial)).as_bytes());
        rehydrator.feed(b"data: {\"cho");

        let drained = rehydrator.drain();
        let text = String::from_utf8(drained).unwrap();
        // Raw buffered bytes survive verbatim, then the carry flushes as a
        // synthetic delta so no received content is lost.
        assert!(text.starts_with("data: {\"cho"));
        let synthetic = text.split("\n\n").nth(1).expect("carry event follows");
        let parsed: Value =
            serde_json::from_str(synthetic.strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(parsed["choices"][0]["delta"]["content"], partial);
        assert!(rehydrator.finish().is_empty());
    }

    #[test]
    fn stream_rehydrator_passes_oversized_event_remainder_verbatim() {
        let (map, token) = map_with("acme\ncorp");
        let mut rehydrator = SseStreamRehydrator::new(map);

        // Overflow mid-event, then the remainder arrives containing a full
        // token — it must NOT be restored into the already-emitted partial
        // JSON frame (the fragment contains a newline; substituting it as
        // plain text would corrupt the emitted bytes' framing anyway).
        let first = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 1];
        let out = rehydrator.feed(&first);
        assert_eq!(out.len(), first.len());

        let remainder = format!("tail {token} end\n\n");
        let out = rehydrator.feed(remainder.as_bytes());
        assert_eq!(String::from_utf8(out).unwrap(), remainder);
        assert!(rehydrator.report().unrestored >= 1);

        // After the terminator, normal restore resumes for the next event.
        let next = format!("{}\n\n", delta_event(&token));
        let out = rehydrator.feed(next.as_bytes());
        let text = String::from_utf8(out).unwrap();
        // The fragment contains a newline; inside a JSON delta leaf serde
        // escapes it, so the event restores fine.
        assert!(text.contains("acme\\ncorp"));
    }

    #[test]
    fn stream_rehydrator_carry_flush_counts_unrestored_markers() {
        let (map, token) = map_with("acme-corp");
        let partial = &token[..12];
        let mut rehydrator = SseStreamRehydrator::new(map);
        rehydrator.feed(format!("{}\n\n", delta_event(partial)).as_bytes());
        assert_eq!(rehydrator.report().unrestored, 0);

        // The flushed carry emits a visible incomplete token marker — it is
        // unrestored and must be counted for audit/metrics.
        rehydrator.finish();
        assert_eq!(rehydrator.report().unrestored, 1);
    }

    #[test]
    fn event_rehydrator_bounds_carry_key_count() {
        let (map, token) = map_with("acme-corp");
        let partial = &token[..12];
        let mut rehydrator = EventRehydrator::new(map);

        // Spray more distinct indices than the carry cap: extra legs get no
        // carry join and emit their partial markers unrestored instead.
        for index in 0..(MAX_CARRY_KEYS as i64 + 1) {
            rehydrator.rehydrate_event(&indexed_delta_event(index, partial));
        }
        assert_eq!(rehydrator.delta_carry.len(), MAX_CARRY_KEYS);
        // The last (over-cap) index emitted its marker unrestored.
        assert_eq!(rehydrator.report().unrestored, 1);
    }

    #[test]
    fn stream_rehydrator_passthrough_keeps_split_boundary() {
        // Every terminator form is exercised with its boundary split across
        // the passthrough flush: the oversized event's trailing EOL byte is
        // retained, so the next event is not swallowed into passthrough.
        for (terminator_first, terminator_rest) in [
            ("\n", "\n"),
            ("\r\n", "\r\n"),
            ("\r", "\r"),
            // CRLF split inside a single line ending.
            ("\r\n\r", "\n"),
        ] {
            let (map, token) = map_with("acme-corp");
            let mut rehydrator = SseStreamRehydrator::new(map);

            // Overflow mid-event; the flood's final byte is the first half
            // of the oversized event's terminator.
            let mut first = vec![b'x'; MAX_EVENT_BUFFER_BYTES + 1];
            first.extend_from_slice(terminator_first.as_bytes());
            let out = rehydrator.feed(&first);
            // The trailing EOL prefix must stay buffered, not flushed.
            assert!(
                out.len() < first.len(),
                "terminator {terminator_first:?}{terminator_rest:?}: out {} vs first {}",
                out.len(),
                first.len()
            );

            let rest = format!("{}{}", terminator_rest, delta_event(&token));
            rehydrator.feed(rest.as_bytes());
            let out = rehydrator.feed(b"\n\n");
            let text = String::from_utf8(out).unwrap();
            assert!(
                text.contains("acme-corp"),
                "next event after overflow must restore, terminator {terminator_first:?}{terminator_rest:?}: {text}"
            );
            assert!(rehydrator.finish().is_empty());
        }
    }

    #[test]
    fn stream_rehydrator_rescans_only_appended_bytes() {
        // Regression guard for incremental scanning: a near-limit event fed
        // in small chunks must still terminate and restore correctly — the
        // cursor keeps the scan linear.
        let (map, token) = map_with("acme-corp");
        let mut rehydrator = SseStreamRehydrator::new(map);
        let mut out = Vec::new();
        for _ in 0..64 {
            out.extend(rehydrator.feed(&vec![b'x'; 1024]));
        }
        out.extend(rehydrator.feed(format!("{}\n\n", delta_event(&token)).as_bytes()));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("acme-corp"));
    }

    #[test]
    fn stream_rehydrator_forwards_non_utf8_events_verbatim() {
        let (map, _) = map_with("acme-corp");
        let mut rehydrator = SseStreamRehydrator::new(map);
        let block = b"data: \xff\xfe\x00\n\n".to_vec();
        let out = rehydrator.feed(&block);
        assert_eq!(out, block);
    }

    #[test]
    fn stream_rehydrator_finish_flushes_carry_as_synthetic_delta() {
        let (map, token) = map_with("acme-corp");
        let partial = &token[..12];
        let mut rehydrator = SseStreamRehydrator::new(map);
        rehydrator.feed(format!("{}\n\n", delta_event(partial)).as_bytes());

        let tail = rehydrator.finish();
        let text = String::from_utf8(tail).unwrap();
        let event: Value =
            serde_json::from_str(text.strip_prefix("data: ").unwrap().trim_end()).unwrap();
        assert_eq!(event["choices"][0]["delta"]["content"], partial);
    }
}
