//! SSE-aware restore pass shared by the buffered and passthrough streaming
//! paths.
//!
//! Upstream bytes are buffered into complete events (split on blank lines) so
//! `data:` payloads that parse as JSON can be restored leaf-wise and
//! re-serialized — a restored fragment containing `"`, `\`, or a newline is
//! correctly escaped by serde and can never inject a phantom SSE event.
//! `choices[].delta.content` strings join a cross-event carry, so a token the
//! model splits across two delta events still resolves before reaching the
//! client. Non-JSON lines restore as plain text under a newline guard that
//! reverts any substitution which would break SSE framing.

use serde_json::Value;

use super::{
    find_subslice, holdback_index, rehydrate_text, rehydrate_value, RehydrateReport, RehydrationMap,
};

/// Stateful per-event rehydrator. Owns the request map so it can live inside
/// a `'static` response stream; the buffered path wraps it around a cloned
/// map instead.
pub struct EventRehydrator {
    map: RehydrationMap,
    delta_carry: String,
    restored_total: u32,
    unrestored_total: u32,
}

impl EventRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self { map, delta_carry: String::new(), restored_total: 0, unrestored_total: 0 }
    }

    /// Cumulative restore counters over all processed events.
    pub fn report(&self) -> RehydrateReport {
        RehydrateReport { restored: self.restored_total, unrestored: self.unrestored_total }
    }

    /// Processes one SSE event block (without its `\n\n` terminator) and
    /// returns the event text. When the block is terminal (`[DONE]` or a
    /// `finish_reason` chunk) a pending token-prefix carry is flushed ahead of
    /// it as a synthetic `chat.completion.chunk` delta event.
    pub fn rehydrate_event(&mut self, block: &str) -> String {
        let mut terminal = false;
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
                    let (rendered, is_terminal) = self.rehydrate_json_event(line, json);
                    terminal |= is_terminal;
                    lines.push(rendered);
                }
                Err(_) => lines.push(self.rehydrate_plain_line(line)),
            }
        }

        let event = lines.join("\n");
        if terminal && !self.delta_carry.is_empty() {
            // The delta chain ends while a token prefix is still pending; emit
            // the held bytes as their own delta so no upstream text is lost.
            return format!("{}\n\n{event}", self.synthetic_delta_event());
        }
        event
    }

    /// Emits the pending token-prefix carry as a synthetic delta event when
    /// the stream ended without a terminal event to flush it through.
    pub fn finish(&mut self) -> Option<String> {
        if self.delta_carry.is_empty() {
            return None;
        }
        Some(self.synthetic_delta_event())
    }

    /// Restores a JSON `data:` payload leaf-wise. `choices[].delta.content`
    /// strings are detached before the generic pass so carry-merged output is
    /// never re-scanned, then reattached afterwards.
    fn rehydrate_json_event(&mut self, line: &str, mut json: Value) -> (String, bool) {
        let mut terminal = false;
        let mut saved_contents: Vec<(usize, String)> = Vec::new();

        if let Some(choices) = json.get_mut("choices").and_then(Value::as_array_mut) {
            for (idx, choice) in choices.iter_mut().enumerate() {
                if choice.get("finish_reason").is_some_and(|reason| !reason.is_null()) {
                    terminal = true;
                }
                let content = choice
                    .pointer_mut("/delta/content")
                    .and_then(|value| value.as_str().map(str::to_string));
                if let Some(content) = content {
                    if let Some(slot) = choice.pointer_mut("/delta/content") {
                        *slot = Value::String(String::new());
                    }
                    saved_contents.push((idx, content));
                }
            }
        }

        let (mut restored, leaf) = rehydrate_value(json, &self.map);
        self.accumulate(leaf);
        let mut changed = leaf.restored > 0;

        if let Some(choices) = restored.get_mut("choices").and_then(Value::as_array_mut) {
            for (idx, content) in saved_contents {
                let processed = self.process_delta_content(&content);
                changed |= processed != content;
                if let Some(slot) = choices[idx].pointer_mut("/delta/content") {
                    *slot = Value::String(processed);
                }
            }
        }

        match serde_json::to_string(&restored) {
            Ok(encoded) if changed => (format!("data: {encoded}"), terminal),
            // Byte-faithful output when nothing was restored.
            _ => (line.to_string(), terminal),
        }
    }

    /// Joins the carry with the next delta `content`, holds back the trailing
    /// proper token prefix, and restores complete tokens in the emittable
    /// part. Restoring the carried merge would otherwise expose a token that
    /// spans two SSE delta events as unrestored.
    fn process_delta_content(&mut self, content: &str) -> String {
        let mut combined = std::mem::take(&mut self.delta_carry);
        combined.push_str(content);
        let cut = holdback_index(combined.as_bytes(), &self.map);
        let (emit, tail) = combined.split_at(cut);
        self.delta_carry = tail.to_string();
        let (restored, report) = rehydrate_text(emit, &self.map);
        self.accumulate(report);
        restored
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

    /// Wraps pending carry bytes in a minimal `chat.completion.chunk` delta so
    /// they reach the client as ordinary content instead of vanishing.
    fn synthetic_delta_event(&mut self) -> String {
        let carry = std::mem::take(&mut self.delta_carry);
        let event = serde_json::json!({
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"content": carry}}],
        });
        format!("data: {}", serde_json::to_string(&event).expect("synthetic delta serializes"))
    }

    fn accumulate(&mut self, report: RehydrateReport) {
        self.restored_total = self.restored_total.saturating_add(report.restored);
        self.unrestored_total = self.unrestored_total.saturating_add(report.unrestored);
    }
}

/// Byte-level wrapper driving `EventRehydrator` over a chunked upstream body:
/// complete SSE events are restored as soon as their `\n\n` terminator
/// arrives; bytes before it stay buffered, which keeps chunk-split tokens
/// contiguous and `data:` JSON intact for leaf-wise restore.
pub struct SseStreamRehydrator {
    inner: EventRehydrator,
    buffer: Vec<u8>,
}

impl SseStreamRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self { inner: EventRehydrator::new(map), buffer: Vec::new() }
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
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.buffer.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(boundary) = find_subslice(&self.buffer, b"\n\n") {
            let block: Vec<u8> = self.buffer.drain(..boundary + 2).collect();
            self.emit_block(&block, &mut out);
        }
        out
    }

    /// Processes the remaining buffered tail at end of stream, then appends
    /// the pending-carry flush (a synthetic delta event) when one is pending.
    pub fn finish(&mut self) -> Vec<u8> {
        let tail: Vec<u8> = self.buffer.drain(..).collect();
        let mut out = Vec::new();
        if !tail.is_empty() {
            self.emit_block(&tail, &mut out);
        }
        if let Some(flush) = self.inner.finish() {
            out.extend_from_slice(flush.as_bytes());
            out.extend_from_slice(b"\n\n");
        }
        out
    }

    /// Bytes upstream sent but not yet emitted, verbatim — for propagation
    /// ahead of a forwarded upstream stream error.
    pub fn drain_raw(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }

    /// Emits one raw event block (blank-line terminator included when
    /// present) through the event rehydrator.
    fn emit_block(&mut self, block: &[u8], out: &mut Vec<u8>) {
        let (body, terminator) = match block.strip_suffix(b"\n\n") {
            Some(body) => (body, &b"\n\n"[..]),
            None => (block, &b""[..]),
        };
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
    fn stream_rehydrator_drain_raw_returns_buffered_tail_verbatim() {
        let (map, _) = map_with("acme-corp");
        let mut rehydrator = SseStreamRehydrator::new(map);
        // A partial event without terminator stays buffered.
        rehydrator.feed(b"data: {\"cho");
        assert_eq!(rehydrator.drain_raw(), b"data: {\"cho");
        assert!(rehydrator.finish().is_empty());
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
