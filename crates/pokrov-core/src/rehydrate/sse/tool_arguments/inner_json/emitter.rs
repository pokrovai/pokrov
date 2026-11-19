//! Escaped output materialization and visible-marker counting.

use crate::rehydrate::{RehydrateReport, TOKEN_PREFIX};
use std::borrow::Cow;

/// Lazily materializes output only when a substitution or carry movement
/// makes the bytes differ from the borrowed input.
pub(super) struct Emitter<'i, 'p> {
    input: &'i [u8],
    pre: &'p [u8],
    emitted: usize,
    owned: Option<Vec<u8>>,
}

impl<'i, 'p> Emitter<'i, 'p> {
    pub(super) fn new(input: &'i [u8], pre: &'p [u8]) -> Self {
        Self { input, pre, emitted: 0, owned: None }
    }

    fn push(&mut self, bytes: &[u8]) {
        match &mut self.owned {
            Some(out) => out.extend_from_slice(bytes),
            None => self.emitted += bytes.len(),
        }
    }

    pub(super) fn push_byte(&mut self, byte: u8) {
        match &mut self.owned {
            Some(out) => out.push(byte),
            None => self.emitted += 1,
        }
    }

    pub(super) fn replace(&mut self, original: &str) {
        if self.owned.is_none() {
            let mut out = Vec::with_capacity(self.emitted + original.len());
            let pre_part = self.emitted.min(self.pre.len());
            out.extend_from_slice(&self.pre[..pre_part]);
            if self.emitted > self.pre.len() {
                out.extend_from_slice(&self.input[..self.emitted - self.pre.len()]);
            }
            self.owned = Some(out);
        }
        escape_fragment(original, self.owned.as_mut().unwrap());
    }

    pub(super) fn finish(self) -> Cow<'i, str> {
        if let Some(out) = self.owned {
            return Cow::Owned(String::from_utf8(out).expect("restoration keeps valid UTF-8"));
        }
        if self.pre.is_empty() && self.emitted == self.input.len() {
            return Cow::Borrowed(std::str::from_utf8(self.input).expect("valid UTF-8 input"));
        }
        let mut out = Vec::with_capacity(self.emitted);
        let pre_part = self.emitted.min(self.pre.len());
        out.extend_from_slice(&self.pre[..pre_part]);
        if self.emitted > self.pre.len() {
            out.extend_from_slice(&self.input[..self.emitted - self.pre.len()]);
        }
        Cow::Owned(String::from_utf8(out).expect("restoration keeps valid UTF-8"))
    }
}

/// Counts visible `__PKV_` markers across contiguous literal emissions,
/// retaining only the five bytes that can still start a marker.
#[derive(Clone, Copy, Default)]
pub(super) struct MarkerCounter {
    tail: [u8; TOKEN_PREFIX.len() - 1],
    len: u8,
}

impl MarkerCounter {
    pub(super) fn reset(&mut self) {
        self.len = 0;
    }

    pub(super) fn count(&mut self, bytes: &[u8]) -> u32 {
        let prefix = TOKEN_PREFIX.as_bytes();
        let tail_len = self.len as usize;
        let total = tail_len + bytes.len();
        let mut count = 0u32;
        if total >= prefix.len() {
            for pos in 0..=total - prefix.len() {
                let matched = prefix.iter().enumerate().all(|(offset, expected)| {
                    let index = pos + offset;
                    let byte =
                        if index < tail_len { self.tail[index] } else { bytes[index - tail_len] };
                    byte == *expected
                });
                if matched {
                    count = count.saturating_add(1);
                }
            }
        }
        let keep = total.min(self.tail.len());
        let start = total - keep;
        for offset in 0..keep {
            let index = start + offset;
            self.tail[offset] =
                if index < tail_len { self.tail[index] } else { bytes[index - tail_len] };
        }
        self.len = keep as u8;
        count
    }
}

pub(super) fn emit_literal(
    bytes: &[u8],
    emit: &mut Emitter,
    report: &mut RehydrateReport,
    counter: &mut MarkerCounter,
) {
    emit.push(bytes);
    report.unrestored = report.unrestored.saturating_add(counter.count(bytes));
}

fn escape_fragment(original: &str, out: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in original.as_bytes() {
        match byte {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x00..=0x1f => out.extend_from_slice(&[
                b'\\',
                b'u',
                b'0',
                b'0',
                HEX[(byte >> 4) as usize],
                HEX[(byte & 0xf) as usize],
            ]),
            _ => out.push(byte),
        }
    }
}
