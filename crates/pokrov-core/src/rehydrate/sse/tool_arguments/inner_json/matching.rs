//! Retained-tail matching: bounded buffer, token scan, and literal-run flush.

use super::emitter::{emit_literal, Emitter, MarkerCounter};
use crate::rehydrate::{
    is_proper_token_prefix, RehydrateReport, RehydrationMap, TOKEN_BASE_LEN, TOKEN_PREFIX,
};

use super::super::MAX_TOKEN_BYTES;

/// Transient matcher buffer: one token plus headroom for overlapping starts.
const TAIL_CAP: usize = MAX_TOKEN_BYTES + 8;

/// Restores every complete token in a confirmed literal run, then emits the
/// rest literally. Used when an escape or closing quote ends the run.
pub(super) fn flush_run(
    map: &RehydrationMap,
    buf: &mut Tail,
    emit: &mut Emitter,
    report: &mut RehydrateReport,
    counter: &mut MarkerCounter,
) {
    loop {
        if buf.len == 0 {
            return;
        }
        let (matched, _) = scan(buf.as_slice(), map);
        let Some((pos, len, original)) = matched else {
            emit_literal(buf.as_slice(), emit, report, counter);
            buf.clear();
            return;
        };
        emit_literal(&buf.as_slice()[..pos], emit, report, counter);
        emit.replace(original);
        counter.reset();
        report.restored = report.restored.saturating_add(1);
        buf.drain_front(pos + len);
    }
}

pub(super) struct Tail {
    bytes: [u8; TAIL_CAP],
    pub(super) len: usize,
}

impl Tail {
    pub(super) fn new() -> Self {
        Self { bytes: [0; TAIL_CAP], len: 0 }
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub(super) fn extend(&mut self, bytes: &[u8]) {
        debug_assert!(self.len == 0 && bytes.len() <= TAIL_CAP);
        self.bytes[..bytes.len()].copy_from_slice(bytes);
        self.len = bytes.len();
    }

    pub(super) fn push(&mut self, byte: u8) {
        debug_assert!(self.len < TAIL_CAP);
        self.bytes[self.len] = byte;
        self.len += 1;
    }

    pub(super) fn drain_front(&mut self, count: usize) {
        debug_assert!(count <= self.len);
        self.bytes.copy_within(count..self.len, 0);
        self.len -= count;
    }

    pub(super) fn clear(&mut self) {
        self.len = 0;
    }

    pub(super) fn is_full(&self) -> bool {
        self.len == TAIL_CAP
    }
}

/// Earliest complete token and earliest proper-prefix start in `buf`.
pub(super) fn scan<'m>(
    buf: &[u8],
    map: &'m RehydrationMap,
) -> (Option<(usize, usize, &'m str)>, Option<usize>) {
    let mut matched = None;
    for idx in 0..buf.len() {
        if buf[idx] != b'_' {
            continue;
        }
        let tail = &buf[idx..];
        if tail.len() < TOKEN_BASE_LEN || !tail.starts_with(TOKEN_PREFIX.as_bytes()) {
            continue;
        }
        if let Some((token, original)) = map.longest_match_at(tail) {
            matched = Some((idx, token.len(), original));
            break;
        }
    }
    let mut prefix = None;
    for idx in 0..buf.len() {
        if buf[idx] != b'_' {
            continue;
        }
        let tail = &buf[idx..];
        let is_prefix = if tail.len() < TOKEN_PREFIX.len() {
            TOKEN_PREFIX.as_bytes().starts_with(tail)
        } else {
            tail.starts_with(TOKEN_PREFIX.as_bytes()) && is_proper_token_prefix(tail, map)
        };
        if is_prefix {
            prefix = Some(idx);
            break;
        }
    }
    (matched, prefix)
}
