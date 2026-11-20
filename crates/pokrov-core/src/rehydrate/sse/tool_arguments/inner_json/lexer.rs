//! Structural phases and the per-`(choice, tool)` inner-JSON state machine.

use super::super::{ArgumentDelta, ArgumentOutcome, MAX_INNER_DEPTH, MAX_PENDING_PREFIX};
use super::emitter::{emit_literal, Emitter, MarkerCounter};
use super::matching::{flush_run, scan, Tail};
use crate::rehydrate::{RehydrateReport, RehydrationMap};
use std::borrow::Cow;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RootPhase {
    Value,
    Done,
}

#[derive(Clone, Copy)]
enum Frame {
    ObjFirstKeyOrEnd,
    ObjKeyRequired,
    ObjColon,
    ObjValue,
    ObjCommaOrEnd,
    ArrFirstValueOrEnd,
    ArrValueRequired,
    ArrCommaOrEnd,
}

#[derive(Clone, Copy)]
enum EscapeState {
    None,
    AfterBackslash,
    Unicode(u8),
}

#[derive(Clone, Copy)]
enum LiteralKind {
    True,
    False,
    Null,
}

impl LiteralKind {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::True => b"true",
            Self::False => b"false",
            Self::Null => b"null",
        }
    }
}

#[derive(Clone, Copy)]
enum NumberState {
    Sign,
    Zero,
    Integer,
    FractionStart,
    Fraction,
    ExponentStart,
    ExponentSign,
    Exponent,
}

#[derive(Clone, Copy)]
enum Mode {
    Structural,
    KeyString(EscapeState),
    ValueString(EscapeState),
    Literal { kind: LiteralKind, matched: u8 },
    Number(NumberState),
}

/// Per-`(choice, index)` inner-JSON parser and token-prefix holder.
pub(crate) struct ArgumentDeltaState {
    root: RootPhase,
    stack: [Frame; MAX_INNER_DEPTH],
    depth: u8,
    mode: Mode,
    prefix: [u8; MAX_PENDING_PREFIX],
    prefix_len: u8,
    counter: MarkerCounter,
}

impl ArgumentDeltaState {
    pub(crate) fn new() -> Self {
        Self {
            root: RootPhase::Value,
            stack: [Frame::ObjFirstKeyOrEnd; MAX_INNER_DEPTH],
            depth: 0,
            mode: Mode::Structural,
            prefix: [0; MAX_PENDING_PREFIX],
            prefix_len: 0,
            counter: MarkerCounter::default(),
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.prefix_len > 0
    }

    /// Restores eligible tokens in one inner-JSON fragment. On a lexical or
    /// depth fault the tentative output is discarded, the pre-field prefix is
    /// released once, and the fragment is returned unchanged as opaque.
    pub(crate) fn push<'a>(&mut self, input: &'a str, map: &RehydrationMap) -> ArgumentDelta<'a> {
        let pre_len = self.prefix_len as usize;
        let mut pre_field = [0u8; MAX_PENDING_PREFIX];
        pre_field[..pre_len].copy_from_slice(&self.prefix[..pre_len]);
        self.prefix_len = 0;
        let mut buf = Tail::new();
        buf.extend(&pre_field[..pre_len]);
        let mut emit = Emitter::new(input.as_bytes(), &pre_field[..pre_len]);
        let mut report = RehydrateReport::default();
        let saved_counter = self.counter;
        for &byte in input.as_bytes() {
            if self.step(byte, map, &mut buf, &mut emit, &mut report).is_err() {
                // Fallback discards every tentative edit and its counts, then
                // passes the pre-field prefix and current field through once.
                self.counter = saved_counter;
                let mut unrestored = self.counter.count(&pre_field[..pre_len]);
                unrestored = unrestored.saturating_add(self.counter.count(input.as_bytes()));
                let text = if pre_len == 0 {
                    Cow::Borrowed(input)
                } else {
                    let mut text = String::with_capacity(pre_len + input.len());
                    text.push_str(
                        std::str::from_utf8(&pre_field[..pre_len]).expect("token prefix"),
                    );
                    text.push_str(input);
                    Cow::Owned(text)
                };
                return ArgumentDelta {
                    text,
                    report: RehydrateReport { restored: 0, unrestored },
                    outcome: ArgumentOutcome::Opaque,
                };
            }
            if buf.is_full() {
                self.settle(map, &mut buf, &mut emit, &mut report);
                debug_assert!(!buf.is_full(), "matcher buffer must make progress");
            }
        }
        self.settle(map, &mut buf, &mut emit, &mut report);
        let retained = buf.len;
        self.prefix[..retained].copy_from_slice(buf.as_slice());
        self.prefix_len = retained as u8;
        ArgumentDelta { text: emit.finish(), report, outcome: ArgumentOutcome::Trusted }
    }

    /// Takes the trailing held prefix for a finishing field or stream flush.
    /// The bytes were never emitted, so markers are counted exactly once.
    pub(crate) fn take_pending(&mut self) -> Option<(String, u32)> {
        if self.prefix_len == 0 {
            return None;
        }
        let bytes = &self.prefix[..self.prefix_len as usize];
        let text = std::str::from_utf8(bytes).expect("token prefix is ASCII").to_string();
        let unrestored = self.counter.count(bytes);
        self.prefix_len = 0;
        Some((text, unrestored))
    }

    fn step(
        &mut self,
        byte: u8,
        map: &RehydrationMap,
        buf: &mut Tail,
        emit: &mut Emitter,
        report: &mut RehydrateReport,
    ) -> Result<(), ()> {
        match self.mode {
            Mode::Structural => {
                self.step_structural(byte)?;
                emit.push_byte(byte);
                Ok(())
            }
            Mode::KeyString(escape) => self.step_key_string(byte, escape, emit),
            Mode::ValueString(escape) => {
                self.step_value_string(byte, escape, map, buf, emit, report)
            }
            Mode::Literal { kind, matched } => {
                let expected = kind.bytes();
                if expected.get(matched as usize) != Some(&byte) {
                    return Err(());
                }
                let next = matched as usize + 1;
                if next == expected.len() {
                    self.mode = Mode::Structural;
                    self.complete_value();
                } else {
                    self.mode = Mode::Literal { kind, matched: next as u8 };
                }
                emit.push_byte(byte);
                Ok(())
            }
            Mode::Number(state) => {
                if self.step_number(byte, state)? {
                    self.mode = Mode::Structural;
                    self.complete_value();
                    return self.step(byte, map, buf, emit, report);
                }
                emit.push_byte(byte);
                Ok(())
            }
        }
    }

    fn step_structural(&mut self, byte: u8) -> Result<(), ()> {
        if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
            return Ok(());
        }
        if self.depth == 0 {
            return match self.root {
                RootPhase::Value => self.start_value(byte),
                RootPhase::Done => Err(()),
            };
        }
        let top = self.depth as usize - 1;
        match self.stack[top] {
            Frame::ObjFirstKeyOrEnd if byte == b'}' => self.close_container(),
            Frame::ObjFirstKeyOrEnd | Frame::ObjKeyRequired if byte == b'"' => {
                self.mode = Mode::KeyString(EscapeState::None);
                Ok(())
            }
            Frame::ObjColon if byte == b':' => {
                self.stack[top] = Frame::ObjValue;
                Ok(())
            }
            Frame::ObjValue => self.start_value(byte),
            Frame::ObjCommaOrEnd if byte == b',' => {
                self.stack[top] = Frame::ObjKeyRequired;
                Ok(())
            }
            Frame::ObjCommaOrEnd if byte == b'}' => self.close_container(),
            Frame::ArrFirstValueOrEnd if byte == b']' => self.close_container(),
            Frame::ArrFirstValueOrEnd | Frame::ArrValueRequired => self.start_value(byte),
            Frame::ArrCommaOrEnd if byte == b',' => {
                self.stack[top] = Frame::ArrValueRequired;
                Ok(())
            }
            Frame::ArrCommaOrEnd if byte == b']' => self.close_container(),
            _ => Err(()),
        }
    }

    fn start_value(&mut self, byte: u8) -> Result<(), ()> {
        match byte {
            b'{' => self.push_frame(Frame::ObjFirstKeyOrEnd),
            b'[' => self.push_frame(Frame::ArrFirstValueOrEnd),
            b'"' => {
                self.mode = Mode::ValueString(EscapeState::None);
                Ok(())
            }
            b't' => {
                self.mode = Mode::Literal { kind: LiteralKind::True, matched: 1 };
                Ok(())
            }
            b'f' => {
                self.mode = Mode::Literal { kind: LiteralKind::False, matched: 1 };
                Ok(())
            }
            b'n' => {
                self.mode = Mode::Literal { kind: LiteralKind::Null, matched: 1 };
                Ok(())
            }
            b'-' => {
                self.mode = Mode::Number(NumberState::Sign);
                Ok(())
            }
            b'0' => {
                self.mode = Mode::Number(NumberState::Zero);
                Ok(())
            }
            b'1'..=b'9' => {
                self.mode = Mode::Number(NumberState::Integer);
                Ok(())
            }
            _ => Err(()),
        }
    }

    fn push_frame(&mut self, frame: Frame) -> Result<(), ()> {
        if self.depth as usize >= MAX_INNER_DEPTH {
            return Err(());
        }
        self.stack[self.depth as usize] = frame;
        self.depth += 1;
        Ok(())
    }

    fn close_container(&mut self) -> Result<(), ()> {
        if self.depth == 0 {
            return Err(());
        }
        self.depth -= 1;
        self.complete_value();
        Ok(())
    }

    fn complete_value(&mut self) {
        if self.depth == 0 {
            self.root = RootPhase::Done;
            return;
        }
        let top = self.depth as usize - 1;
        self.stack[top] = match self.stack[top] {
            Frame::ObjValue => Frame::ObjCommaOrEnd,
            Frame::ArrFirstValueOrEnd | Frame::ArrValueRequired => Frame::ArrCommaOrEnd,
            other => other,
        };
    }

    fn step_number(&mut self, byte: u8, state: NumberState) -> Result<bool, ()> {
        use NumberState::*;
        let next = match state {
            Sign if byte == b'0' => Zero,
            Sign if byte.is_ascii_digit() => Integer,
            Integer if byte.is_ascii_digit() => Integer,
            Fraction if byte.is_ascii_digit() => Fraction,
            Exponent if byte.is_ascii_digit() => Exponent,
            Zero | Integer if byte == b'.' => FractionStart,
            Zero | Integer | Fraction if matches!(byte, b'e' | b'E') => ExponentStart,
            ExponentStart if byte.is_ascii_digit() => Exponent,
            ExponentStart if matches!(byte, b'+' | b'-') => ExponentSign,
            ExponentSign if byte.is_ascii_digit() => Exponent,
            FractionStart if byte.is_ascii_digit() => Fraction,
            Zero if byte.is_ascii_digit() => return Err(()),
            Zero | Integer | Fraction | Exponent => return Ok(true),
            _ => return Err(()),
        };
        self.mode = Mode::Number(next);
        Ok(false)
    }

    fn step_key_string(
        &mut self,
        byte: u8,
        escape: EscapeState,
        emit: &mut Emitter,
    ) -> Result<(), ()> {
        match escape {
            EscapeState::None if byte == b'"' => {
                emit.push_byte(byte);
                self.mode = Mode::Structural;
                self.stack[self.depth as usize - 1] = Frame::ObjColon;
                Ok(())
            }
            EscapeState::None if byte == b'\\' => {
                emit.push_byte(byte);
                self.mode = Mode::KeyString(EscapeState::AfterBackslash);
                Ok(())
            }
            EscapeState::None if byte < 0x20 => Err(()),
            EscapeState::None => {
                emit.push_byte(byte);
                Ok(())
            }
            other => {
                let next = advance_escape(other, byte)?;
                emit.push_byte(byte);
                self.mode = Mode::KeyString(next);
                Ok(())
            }
        }
    }

    fn step_value_string(
        &mut self,
        byte: u8,
        escape: EscapeState,
        map: &RehydrationMap,
        buf: &mut Tail,
        emit: &mut Emitter,
        report: &mut RehydrateReport,
    ) -> Result<(), ()> {
        match escape {
            EscapeState::None if byte == b'"' => {
                flush_run(map, buf, emit, report, &mut self.counter);
                // No marker can span a closing quote or the next value.
                self.counter.reset();
                emit.push_byte(byte);
                self.mode = Mode::Structural;
                self.complete_value();
                Ok(())
            }
            EscapeState::None if byte == b'\\' => {
                // An escape disproves a held prefix; the sequence itself
                // never participates in token matching.
                flush_run(map, buf, emit, report, &mut self.counter);
                self.counter.reset();
                emit.push_byte(byte);
                self.mode = Mode::ValueString(EscapeState::AfterBackslash);
                Ok(())
            }
            EscapeState::None if byte < 0x20 => Err(()),
            EscapeState::None => {
                buf.push(byte);
                Ok(())
            }
            other => {
                let next = advance_escape(other, byte)?;
                emit.push_byte(byte);
                self.mode = Mode::ValueString(next);
                Ok(())
            }
        }
    }

    /// Emits everything that cannot start a token and holds the earliest
    /// proper-prefix suffix. Complete tokens are restored first; overlapping
    /// candidate starts are retried by rescanning the remaining suffix.
    fn settle(
        &mut self,
        map: &RehydrationMap,
        buf: &mut Tail,
        emit: &mut Emitter,
        report: &mut RehydrateReport,
    ) {
        loop {
            if buf.len == 0 {
                return;
            }
            let (matched, prefix) = scan(buf.as_slice(), map);
            if let Some((pos, len, original)) = matched {
                if prefix.is_none_or(|h| pos <= h) {
                    emit_literal(&buf.as_slice()[..pos], emit, report, &mut self.counter);
                    emit.replace(original);
                    self.counter.reset();
                    report.restored = report.restored.saturating_add(1);
                    buf.drain_front(pos + len);
                    continue;
                }
            }
            if let Some(h) = prefix {
                emit_literal(&buf.as_slice()[..h], emit, report, &mut self.counter);
                buf.drain_front(h);
                return;
            }
            emit_literal(buf.as_slice(), emit, report, &mut self.counter);
            buf.clear();
            return;
        }
    }
}

fn advance_escape(escape: EscapeState, byte: u8) -> Result<EscapeState, ()> {
    match escape {
        EscapeState::None => Err(()),
        EscapeState::AfterBackslash => match byte {
            b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => Ok(EscapeState::None),
            b'u' => Ok(EscapeState::Unicode(4)),
            _ => Err(()),
        },
        EscapeState::Unicode(remaining) => {
            if !byte.is_ascii_hexdigit() {
                return Err(());
            }
            Ok(if remaining == 1 { EscapeState::None } else { EscapeState::Unicode(remaining - 1) })
        }
    }
}
