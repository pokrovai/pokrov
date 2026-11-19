//! Byte-level SSE driver: buffers transport chunks into complete events and
//! restores each event as soon as its blank-line delimiter arrives.
//!
//! Chat Completions mode adds tool-argument continuity across events. Any
//! frame that cannot be parsed safely (malformed or multi-`data`, invalid
//! UTF-8, an in-band error, or one over the event budget) is opaque: pending
//! argument suffixes are flushed literally before the raw bytes and tool
//! association is closed for the rest of the stream. Text carry keeps its
//! own, independent continuity.

use super::{
    event_boundary_end, tool_arguments, EventRehydrator, RehydrationMap, MAX_EVENT_BUFFER_BYTES,
};
use crate::rehydrate::RehydrateReport;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Length of the trailing run of CR/LF bytes — a candidate half of an event
/// terminator that may complete only once the next chunk arrives. Kept
/// buffered instead of flushing so split boundaries are not lost.
fn trailing_eol_len(bytes: &[u8]) -> usize {
    bytes.iter().rev().take_while(|b| matches!(b, b'\r' | b'\n')).count()
}

/// Whether `bytes` ends with a complete blank-line delimiter. Counts CRLF as
/// one terminator and lone CR/LF as one each, so `\r\n\r` and `\r\r` both
/// qualify while a single `\r\n` does not.
fn ends_with_blank_line(bytes: &[u8]) -> bool {
    let mut terminators = 0usize;
    let mut index = bytes.len();
    while index > 0 {
        match bytes[index - 1] {
            b'\n' => {
                terminators += 1;
                index -= 1;
                if index > 0 && bytes[index - 1] == b'\r' {
                    index -= 1;
                }
            }
            b'\r' => {
                terminators += 1;
                index -= 1;
            }
            _ => break,
        }
    }
    terminators >= 2
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
    /// A consumed blank-line delimiter ended with a lone CR at the end of a
    /// transport chunk; a LF at the start of the next chunk completes that
    /// same delimiter instead of starting a new frame. Chat mode only, where
    /// synthetic suffix chunks must not merge into that delimiter.
    pending_delimiter_lf: bool,
    /// Whether the last bytes handed to the caller ended at a complete
    /// blank-line delimiter. Synthetic chunks use this state instead of the
    /// current feed's partial output, which may hold only the tail of a
    /// split delimiter.
    at_frame_boundary: bool,
    /// True until the first block or raw bytes are emitted. Only a BOM at
    /// this absolute stream start is ignored for Chat parsing; a mid-stream
    /// BOM-prefixed line is ordinary data bytes.
    at_stream_start: bool,
}

impl SseStreamRehydrator {
    pub fn new(map: RehydrationMap) -> Self {
        Self {
            inner: EventRehydrator::new(map),
            buffer: Vec::new(),
            scanned: 0,
            overflow_passthrough: false,
            pending_delimiter_lf: false,
            at_frame_boundary: true,
            at_stream_start: false,
        }
    }

    /// Chat Completions mode: restores text fields as in [`Self::new`] and
    /// additionally carries tool-argument token prefixes across events.
    /// Opaque frames flush pending argument suffixes and close tool
    /// association while leaving text continuity independent.
    pub fn new_chat_completions(map: RehydrationMap) -> Self {
        Self {
            inner: EventRehydrator::new_chat_completions(map),
            buffer: Vec::new(),
            scanned: 0,
            overflow_passthrough: false,
            pending_delimiter_lf: false,
            at_frame_boundary: true,
            at_stream_start: true,
        }
    }

    /// Cumulative restore counters; streaming callers report deltas against
    /// this snapshot per emitted chunk.
    pub fn report(&self) -> RehydrateReport {
        self.inner.report()
    }

    /// Whether the held buffer and every pending carry are empty — used by
    /// callers to decide if end-of-stream work is needed at all. A retained
    /// argument parser without pending bytes does not count as work.
    pub fn is_idle(&self) -> bool {
        self.buffer.is_empty()
            && self.inner.delta_carry.is_empty()
            && !self.inner.has_pending_arguments()
    }

    /// Feeds one upstream chunk and returns bytes safe to emit downstream.
    /// An unterminated event past `MAX_EVENT_BUFFER_BYTES` is flushed
    /// verbatim to keep the buffer bounded; its markers count as unrestored.
    /// The remainder of that event then streams through verbatim until its
    /// blank-line terminator — a partial JSON frame already emitted can never
    /// be safely mutated afterwards. In Chat mode the first bypassed byte
    /// also flushes pending tool-argument suffixes and closes association.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut buffer = std::mem::take(&mut self.buffer);
        buffer.extend_from_slice(chunk);
        let mut out = Vec::new();
        let mut start = 0usize;

        // The LF half of a CRLF delimiter split at the previous chunk
        // boundary belongs to the delimiter already emitted, not to the
        // next frame. Forward it once, in order.
        if self.pending_delimiter_lf {
            match buffer.first() {
                Some(&b'\n') => {
                    out.extend_from_slice(b"\n");
                    start = 1;
                    self.pending_delimiter_lf = false;
                }
                Some(_) => self.pending_delimiter_lf = false,
                None => {}
            }
        }

        if self.overflow_passthrough {
            match event_boundary_end(&buffer[start..]) {
                // The oversized event finally terminated; resume restoring
                // at the next event.
                Some(rel) => {
                    let end = start + rel;
                    let block = &buffer[start..end];
                    self.note_raw_bytes(block);
                    out.extend_from_slice(block);
                    self.at_frame_boundary = ends_with_blank_line(block);
                    if self.inner.is_chat_completions()
                        && end == buffer.len()
                        && block.last() == Some(&b'\r')
                    {
                        self.pending_delimiter_lf = true;
                    }
                    start = end;
                    self.overflow_passthrough = false;
                }
                // No terminator yet: stream the bytes out verbatim, keeping
                // memory bounded by the chunk rather than the event. The
                // trailing CR/LF run stays buffered: it may pair with the
                // next chunk's leading bytes to complete the awaited
                // terminator — a boundary split across this flush would
                // otherwise be lost.
                None => {
                    let keep = trailing_eol_len(&buffer[start..]);
                    let flush_end = buffer.len() - keep;
                    if flush_end > start {
                        let block = &buffer[start..flush_end];
                        self.note_raw_bytes(block);
                        out.extend_from_slice(block);
                        self.at_frame_boundary = false;
                        start = flush_end;
                    }
                    self.store_buffer(buffer, start);
                    return out;
                }
            }
        }

        let mut scan_from = self.scanned.max(start);
        while let Some(rel) = event_boundary_end(&buffer[scan_from..]) {
            let end = scan_from + rel;
            let block = &buffer[start..end];
            let ends_chunk = end == buffer.len();
            let ends_cr = block.last() == Some(&b'\r');
            self.emit_block(block, &mut out);
            start = end;
            scan_from = end;
            if ends_chunk && ends_cr && self.inner.is_chat_completions() {
                self.pending_delimiter_lf = true;
            }
        }

        let remaining = &buffer[start..];
        // Chat restoration measures the event budget before the blank-line
        // delimiter so a terminator split across chunks cannot tip an
        // otherwise admissible frame into passthrough. The default driver
        // keeps its original whole-buffer threshold.
        let over_budget = if self.inner.is_chat_completions() {
            remaining.len() - trailing_eol_len(remaining) - self.initial_bom_len(remaining)
                > MAX_EVENT_BUFFER_BYTES
        } else {
            remaining.len() > MAX_EVENT_BUFFER_BYTES
        };
        if over_budget {
            let keep = trailing_eol_len(remaining);
            let flush_end = buffer.len() - keep;
            if self.inner.is_chat_completions() && flush_end > start {
                self.flush_chat_carries(&mut out);
            }
            if flush_end > start {
                let block = &buffer[start..flush_end];
                self.note_raw_bytes(block);
                out.extend_from_slice(block);
                self.at_frame_boundary = false;
                start = flush_end;
            }
            self.overflow_passthrough = true;
        }

        self.store_buffer(buffer, start);
        out
    }

    /// Processes one complete frame supplied by a caller that guarantees a
    /// frame boundary. Chat mode routes it through the normal event path
    /// without the event-budget shortcut, so a source-admissible frame that
    /// grew during policy re-encoding can still restore. Invalid UTF-8,
    /// multi-data, error, and malformed frames remain opaque, and pending
    /// argument suffixes precede their raw bytes. Non-Chat mode forwards the
    /// frame raw.
    pub fn feed_complete_event(&mut self, frame: &[u8]) -> Vec<u8> {
        if !self.inner.is_chat_completions() {
            return self.bypass_complete_event(frame);
        }
        let mut out = Vec::new();
        if !self.buffer.is_empty() {
            let tail = std::mem::take(&mut self.buffer);
            self.note_raw_bytes(&tail);
            out.extend_from_slice(&tail);
            self.at_frame_boundary = ends_with_blank_line(&tail);
        }
        self.scanned = 0;
        self.overflow_passthrough = false;
        let mut frame = frame;
        if self.pending_delimiter_lf {
            if frame.first() == Some(&b'\n') {
                out.extend_from_slice(b"\n");
                frame = &frame[1..];
            }
            self.pending_delimiter_lf = false;
        }
        if !frame.is_empty() {
            self.emit_block_inner(frame, &mut out, false);
            self.at_frame_boundary = ends_with_blank_line(frame);
            if frame.last() == Some(&b'\r') {
                self.pending_delimiter_lf = true;
            }
        }
        out
    }

    /// Processes the remaining buffered tail at end of stream. Chat mode
    /// writes pending argument suffix chunks, then text carries, and finally
    /// an unterminated raw tail forwarded verbatim so it stays unterminated.
    pub fn finish(&mut self) -> Vec<u8> {
        let tail: Vec<u8> = self.buffer.drain(..).collect();
        self.scanned = 0;
        self.pending_delimiter_lf = false;
        let mut out = Vec::new();
        if self.inner.is_chat_completions() {
            if !std::mem::take(&mut self.overflow_passthrough) {
                self.flush_chat_carries(&mut out);
            }
            if !tail.is_empty() {
                self.note_raw_bytes(&tail);
                out.extend_from_slice(&tail);
                self.at_frame_boundary = ends_with_blank_line(&tail);
            }
            return out;
        }
        if self.overflow_passthrough {
            self.inner.note_unrestored_markers(&tail);
            out.extend_from_slice(&tail);
        } else if !tail.is_empty() {
            self.emit_block(&tail, &mut out);
        }
        self.append_carry_flush(&mut out);
        out
    }

    /// Bytes upstream sent but not yet emitted plus pending flushes — for
    /// propagation ahead of a forwarded upstream stream error so no
    /// already-received content is dropped. Chat mode places argument
    /// suffixes first, then text carries, then the raw tail verbatim last;
    /// the raw tail is never sent through argument or plain-line restoration
    /// and stays unterminated. The default driver is byte-for-byte unchanged.
    pub fn drain(&mut self) -> Vec<u8> {
        self.scanned = 0;
        self.pending_delimiter_lf = false;
        if self.inner.is_chat_completions() {
            let mut out = Vec::new();
            if !std::mem::take(&mut self.overflow_passthrough) {
                self.flush_chat_carries(&mut out);
            }
            let tail = std::mem::take(&mut self.buffer);
            self.note_raw_bytes(&tail);
            out.extend_from_slice(&tail);
            if !tail.is_empty() {
                self.at_frame_boundary = ends_with_blank_line(&tail);
            }
            return out;
        }
        let mut out = std::mem::take(&mut self.buffer);
        self.inner.note_unrestored_markers(&out);
        self.append_carry_flush(&mut out);
        out
    }

    /// Forwards one complete frame that a caller already decided is opaque
    /// (pre-policy provenance) while keeping pending argument bytes in
    /// order. Pending tool suffixes are written before `frame`, and tool
    /// continuity is closed so later fragments cannot join across the gap.
    pub fn bypass_complete_event(&mut self, frame: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if !self.buffer.is_empty() {
            // Buffered bytes precede the supplied frame; a partial event
            // cannot be safely re-framed around an inserted chunk.
            let tail = std::mem::take(&mut self.buffer);
            self.note_raw_bytes(&tail);
            out.extend_from_slice(&tail);
            self.at_frame_boundary = ends_with_blank_line(&tail);
        }
        self.scanned = 0;
        self.pending_delimiter_lf = false;
        self.overflow_passthrough = false;
        self.flush_pending_tool_arguments(&mut out);
        self.note_raw_bytes(frame);
        out.extend_from_slice(frame);
        self.at_frame_boundary = ends_with_blank_line(frame);
        if self.inner.is_chat_completions() && frame.last() == Some(&b'\r') {
            self.pending_delimiter_lf = true;
        }
        out
    }

    /// Emits pending tool suffixes into `out`, separated from preceding raw
    /// bytes by a blank line when necessary.
    fn flush_pending_tool_arguments(&mut self, out: &mut Vec<u8>) {
        if !self.inner.is_chat_completions() {
            return;
        }
        let events = tool_arguments::flush_pending_arguments(&mut self.inner);
        self.append_event_chunks(out, events);
    }

    fn flush_chat_carries(&mut self, out: &mut Vec<u8>) {
        let events = self.inner.finish();
        self.append_event_chunks(out, events);
    }

    fn initial_bom_len(&self, bytes: &[u8]) -> usize {
        if self.at_stream_start && bytes.starts_with(UTF8_BOM) {
            UTF8_BOM.len()
        } else {
            0
        }
    }

    /// Appends synthetic events. The driver's explicit boundary state decides
    /// whether a blank line is needed; the current feed's output may hold only
    /// the LF half of a split delimiter.
    fn append_event_chunks(&mut self, out: &mut Vec<u8>, events: Vec<String>) {
        if events.is_empty() {
            return;
        }
        if !self.at_frame_boundary {
            out.extend_from_slice(b"\n\n");
        }
        for event in events {
            out.extend_from_slice(event.as_bytes());
            out.extend_from_slice(b"\n\n");
        }
        self.at_frame_boundary = true;
    }

    /// Compacts consumed bytes once per `feed` and restores the incremental
    /// scan cursor: only the trailing EOL run can still join a future
    /// boundary.
    fn store_buffer(&mut self, mut buffer: Vec<u8>, consumed: usize) {
        if consumed >= buffer.len() {
            buffer.clear();
        } else {
            buffer.drain(..consumed);
        }
        self.buffer = buffer;
        self.scanned = self.buffer.len() - trailing_eol_len(&self.buffer);
    }

    /// Counts `__PKV_` markers in bytes forwarded verbatim.
    fn note_raw_bytes(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.at_stream_start = false;
        }
        self.inner.note_unrestored_markers(bytes);
    }

    /// Appends synthetic text-carry events for the default driver, keeping
    /// the pre-existing separator rule.
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
        self.emit_block_inner(block, out, true);
    }

    fn emit_block_inner(&mut self, block: &[u8], out: &mut Vec<u8>, enforce_budget: bool) {
        let tail_start = block
            .iter()
            .rposition(|byte| !matches!(byte, b'\r' | b'\n'))
            .map_or(0, |index| index + 1);
        let (body, terminator) = block.split_at(tail_start);
        let (bom, body) = body.split_at(self.initial_bom_len(body));
        self.at_stream_start = false;
        if enforce_budget && self.inner.is_chat_completions() && body.len() > MAX_EVENT_BUFFER_BYTES
        {
            // Match the ordering when this oversized frame arrives piecemeal.
            self.flush_chat_carries(out);
            self.note_raw_bytes(block);
            out.extend_from_slice(block);
        } else {
            match std::str::from_utf8(body) {
                Ok(text) => {
                    out.extend_from_slice(bom);
                    out.extend_from_slice(self.inner.rehydrate_event(text).as_bytes());
                    out.extend_from_slice(terminator);
                }
                Err(_) if self.inner.is_chat_completions() => self.emit_chat_raw(block, out),
                Err(_) => out.extend_from_slice(block),
            }
        }
        self.at_frame_boundary = ends_with_blank_line(block);
    }

    /// Flushes pending argument suffixes, then forwards one opaque Chat
    /// block once and closes tool association for the rest of the stream.
    fn emit_chat_raw(&mut self, block: &[u8], out: &mut Vec<u8>) {
        self.flush_pending_tool_arguments(out);
        self.note_raw_bytes(block);
        out.extend_from_slice(block);
    }
}

#[cfg(test)]
mod chat_tests;
