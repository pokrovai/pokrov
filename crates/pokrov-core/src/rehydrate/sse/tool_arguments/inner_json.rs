//! Bounded incremental lexer for the inner JSON carried by Chat Completions
//! `function.arguments` deltas.
//!
//! `lexer` holds the structural phases and per-pair state machine, `matching`
//! scans token candidates in the retained tail, and `emitter` materializes
//! escaped output and counts visible markers.
//!
//! The lexer keeps structural phases, a fixed-depth frame stack, and one
//! trailing token prefix per pair. Literal, unescaped bytes of value strings
//! feed the matcher; keys, punctuation, and escape bytes pass through.
//! Restored fragments are escaped as inner JSON string content so the outer
//! serde pass can encode them again.

mod emitter;
mod lexer;
mod matching;

pub(crate) use lexer::ArgumentDeltaState;
