use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;

use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use pokrov_core::prompt_injection::{
    PromptInjectionDetection, PromptInjectionDetector, PromptInjectionDetectorDescriptor,
    PromptInjectionDetectorError,
};
use tokenizers::Tokenizer;
use tracing::info;

use crate::error::PromptInjectionProviderError;
use crate::label_index::resolve_injection_index;

/// Provider-side windowing limits. `max_tokens` is the full model input
/// window *including* special tokens; content capacity per window is
/// `max_tokens - specials`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkingLimits {
    pub max_tokens: usize,
    pub overlap_tokens: usize,
    pub max_chunks: usize,
}

impl Default for ChunkingLimits {
    fn default() -> Self {
        Self { max_tokens: 512, overlap_tokens: 64, max_chunks: 32 }
    }
}

/// Sequence-classification detector executed via ONNX Runtime.
///
/// Texts are tokenized once without special tokens, then evaluated in
/// overlapping windows bounded by `max_chunks`; the verdict score is the
/// maximum injection probability across windows, so an attack inside the
/// covered range is not diluted by benign neighbours. Content whose tail
/// exceeds the window budget fails with `ContentTruncated` — an unreviewed
/// fragment never masquerades as a benign verdict. The session sits behind
/// a `Mutex` because `Session::run` requires `&mut self`; callers are
/// responsible for offloading `detect` from async worker threads.
pub struct LocalOnnxPromptInjectionDetector {
    descriptor: PromptInjectionDetectorDescriptor,
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    /// Special token ids prepended to every window (e.g. `[CLS]`).
    prefix_ids: Vec<i64>,
    /// Special token ids appended to every window (e.g. `[SEP]`).
    suffix_ids: Vec<i64>,
    /// Declared model input names; `token_type_ids` is only fed when the
    /// model declares it (DeBERTa-family models do not).
    input_names: Vec<String>,
    injection_index: usize,
    limits: ChunkingLimits,
}

impl LocalOnnxPromptInjectionDetector {
    /// Loads the session and tokenizer and resolves the injection logit
    /// index. Expensive; call once during bootstrap.
    pub fn load(
        model_id: impl Into<String>,
        model_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
        injection_label_index: Option<u32>,
        chunking: ChunkingLimits,
    ) -> Result<Self, PromptInjectionProviderError> {
        let model_id = model_id.into();
        let model_path = model_path.as_ref();
        let tokenizer_path = tokenizer_path.as_ref();

        if !model_path.exists() {
            return Err(PromptInjectionProviderError::ModelNotFound {
                path: model_path.to_path_buf(),
            });
        }
        if !tokenizer_path.exists() {
            return Err(PromptInjectionProviderError::TokenizerNotFound {
                path: tokenizer_path.to_path_buf(),
            });
        }

        let session = Session::builder()
            .map_err(|e| PromptInjectionProviderError::SessionInit(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| PromptInjectionProviderError::SessionInit(e.to_string()))?;
        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| PromptInjectionProviderError::TokenizationFailed(e.to_string()))?;
        // A tokenizer.json may embed truncation/padding that silently drops
        // tokens beyond the model window (e.g. `max_length: 256`). The
        // chunker must see the full token stream, otherwise an unreviewed
        // tail would bypass the `ContentTruncated` guard.
        tokenizer
            .with_truncation(None)
            .map_err(|e| PromptInjectionProviderError::TokenizationFailed(e.to_string()))?;
        tokenizer.with_padding(None);

        // Derive the special-token wrapping from the tokenizer itself instead
        // of hardcoding BERT-style [CLS]/[SEP]: encoding an empty string yields
        // exactly the template specials for the model family.
        let specials = tokenizer
            .encode("", true)
            .map_err(|e| PromptInjectionProviderError::TokenizationFailed(e.to_string()))?;
        let special_ids = specials.get_ids();
        let (prefix_ids, suffix_ids) = match special_ids.split_first() {
            Some((first, rest)) => (vec![*first as i64], rest.iter().map(|&id| id as i64).collect()),
            None => (Vec::new(), Vec::new()),
        };

        let special_total = prefix_ids.len() + suffix_ids.len();
        let content_window = chunking.max_tokens.saturating_sub(special_total);
        if content_window == 0 {
            return Err(PromptInjectionProviderError::InvalidConfig(format!(
                "chunking.max_tokens={} leaves no room for content tokens (specials={special_total})",
                chunking.max_tokens
            )));
        }
        if chunking.overlap_tokens >= content_window {
            return Err(PromptInjectionProviderError::InvalidConfig(format!(
                "chunking.overlap_tokens={} must be smaller than the content window {content_window}",
                chunking.overlap_tokens
            )));
        }
        if chunking.max_chunks == 0 {
            return Err(PromptInjectionProviderError::InvalidConfig(
                "chunking.max_chunks must be greater than zero".to_string(),
            ));
        }

        let input_names: Vec<String> =
            session.inputs().iter().map(|input| input.name().to_string()).collect();
        let injection_index = resolve_injection_index(model_path, injection_label_index)?;

        info!(
            "Prompt-injection detector loaded: model_id={}, inputs={:?}, injection_index={}, specials={}, content_window={}",
            model_id,
            input_names,
            injection_index,
            special_total,
            content_window
        );

        Ok(Self {
            descriptor: PromptInjectionDetectorDescriptor {
                detector_id: "prompt-injection-onnx".to_string(),
                provider: "onnx".to_string(),
                model_id,
            },
            session: Mutex::new(session),
            tokenizer,
            prefix_ids,
            suffix_ids,
            input_names,
            injection_index,
            limits: chunking,
        })
    }

    /// Tokenizes `text` and returns how many content windows `detect` would
    /// evaluate, without running inference. Mirrors `detect` error behavior:
    /// a tail beyond the chunk budget yields `ContentTruncated`. Intended for
    /// offline tooling (e.g. the benchmark harness) that needs to size probe
    /// inputs without paying inference cost per iteration.
    pub fn count_windows(&self, text: &str) -> Result<u32, PromptInjectionDetectorError> {
        let encoding = self.tokenizer.encode(text, false).map_err(|e| {
            PromptInjectionDetectorError::InferenceFailed(format!("tokenization failed: {e}"))
        })?;
        let content_window =
            self.limits.max_tokens - self.prefix_ids.len() - self.suffix_ids.len();
        let (windows, truncated) = window_ranges(
            encoding.get_ids().len(),
            content_window,
            self.limits.overlap_tokens,
            self.limits.max_chunks,
        );
        if truncated {
            return Err(PromptInjectionDetectorError::ContentTruncated);
        }
        Ok(windows.len() as u32)
    }

    /// Runs one context window through the session and returns the injection
    /// softmax probability.
    fn infer_window(
        &self,
        content_ids: &[u32],
    ) -> Result<f32, PromptInjectionDetectorError> {
        let seq_len = self.prefix_ids.len() + content_ids.len() + self.suffix_ids.len();
        let mut ids = Vec::with_capacity(seq_len);
        ids.extend_from_slice(&self.prefix_ids);
        ids.extend(content_ids.iter().map(|&id| id as i64));
        ids.extend_from_slice(&self.suffix_ids);

        let to_tensor = |data: Vec<i64>| -> Result<Tensor<i64>, PromptInjectionDetectorError> {
            Tensor::from_array(([1usize, seq_len], data)).map_err(|e| {
                PromptInjectionDetectorError::InferenceFailed(e.to_string())
            })
        };

        // Feed only inputs the session declares; foreign inputs would abort
        // the run with an unknown-input error.
        let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> =
            Vec::with_capacity(self.input_names.len());
        for name in &self.input_names {
            let tensor = match name.as_str() {
                "input_ids" => Some(to_tensor(ids.clone())?),
                "attention_mask" => Some(to_tensor(vec![1i64; seq_len])?),
                "token_type_ids" => Some(to_tensor(vec![0i64; seq_len])?),
                _ => None,
            };
            if let Some(tensor) = tensor {
                inputs.push((Cow::Borrowed(name.as_str()), tensor.into()));
            }
        }

        // `SessionOutputs` borrows the session, so extraction must happen
        // while the lock guard is still alive.
        let mut session = self.session.lock().map_err(|_| {
            PromptInjectionDetectorError::InferenceFailed("session lock poisoned".to_string())
        })?;
        let outputs = session
            .run(inputs)
            .map_err(|e| PromptInjectionDetectorError::InferenceFailed(e.to_string()))?;
        let (_, logits) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| PromptInjectionDetectorError::InferenceFailed(e.to_string()))?;
        if self.injection_index >= logits.len() {
            return Err(PromptInjectionDetectorError::InferenceFailed(format!(
                "model produced {} logits, injection index {} out of range",
                logits.len(),
                self.injection_index
            )));
        }
        let probability = softmax_probability(logits, self.injection_index);
        // Drop order matters: `SessionOutputs` borrows the session guard.
        drop(outputs);
        drop(session);

        Ok(probability)
    }
}

impl PromptInjectionDetector for LocalOnnxPromptInjectionDetector {
    fn descriptor(&self) -> &PromptInjectionDetectorDescriptor {
        &self.descriptor
    }

    fn detect(
        &self,
        text: &str,
    ) -> Result<PromptInjectionDetection, PromptInjectionDetectorError> {
        // Tokenize once without specials; window boundaries are then pure
        // slicing, which keeps per-window token id sequences exact (re-encoding
        // each window would shift subword boundaries).
        let encoding = self.tokenizer.encode(text, false).map_err(|e| {
            PromptInjectionDetectorError::InferenceFailed(format!("tokenization failed: {e}"))
        })?;
        let ids = encoding.get_ids();
        if ids.is_empty() {
            return Ok(PromptInjectionDetection { score: 0.0, chunks_processed: 0 });
        }

        let content_window =
            self.limits.max_tokens - self.prefix_ids.len() - self.suffix_ids.len();
        let (windows, truncated) = window_ranges(
            ids.len(),
            content_window,
            self.limits.overlap_tokens,
            self.limits.max_chunks,
        );

        // An unevaluated tail must never pass as a benign verdict: the caller
        // degrades this per fail_mode instead of trusting a partial score.
        if truncated {
            return Err(PromptInjectionDetectorError::ContentTruncated);
        }

        let mut max_score = 0.0f32;
        let mut chunks_processed = 0u32;
        for (start, end) in windows {
            let score = self.infer_window(&ids[start..end])?;
            if score > max_score {
                max_score = score;
            }
            chunks_processed += 1;
        }

        Ok(PromptInjectionDetection { score: max_score, chunks_processed })
    }
}

/// Splits `total` content tokens into `(start, end)` windows of at most
/// `window` tokens advanced by `window - overlap`, capped at `max_chunks`.
/// The second tuple element is true when the cap leaves a tail unevaluated.
fn window_ranges(
    total: usize,
    window: usize,
    overlap: usize,
    max_chunks: usize,
) -> (Vec<(usize, usize)>, bool) {
    let step = window.saturating_sub(overlap).max(1);
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while start < total && ranges.len() < max_chunks {
        let end = (start + window).min(total);
        ranges.push((start, end));
        if end == total {
            break;
        }
        start += step;
    }
    let truncated = ranges.last().is_some_and(|&(_, end)| end < total);
    (ranges, truncated)
}

/// Numerically stable softmax restricted to the requested index.
fn softmax_probability(logits: &[f32], index: usize) -> f32 {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let denominator: f32 = logits.iter().map(|logit| (logit - max).exp()).sum();
    (logits[index] - max).exp() / denominator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_ranges_single_window_covers_short_input() {
        let (ranges, truncated) = window_ranges(100, 512, 64, 32);
        assert_eq!(ranges, vec![(0, 100)]);
        assert!(!truncated);
    }

    #[test]
    fn window_ranges_overlap_steps_cover_full_input() {
        // window=10, overlap=4 → step=6; coverage: [0,10) [6,16) [12,22) [18,25)
        let (ranges, truncated) = window_ranges(25, 10, 4, 8);
        assert_eq!(ranges, vec![(0, 10), (6, 16), (12, 22), (18, 25)]);
        assert!(!truncated);
    }

    #[test]
    fn window_ranges_exact_fit_is_not_truncated() {
        // Exactly two windows cover the input: [0,10) [6,16).
        let (ranges, truncated) = window_ranges(16, 10, 4, 2);
        assert_eq!(ranges.len(), 2);
        assert!(!truncated);
    }

    #[test]
    fn window_ranges_reports_truncated_tail() {
        // Third window would start at 12 but max_chunks=2 stops at 16 < 25.
        let (ranges, truncated) = window_ranges(25, 10, 4, 2);
        assert_eq!(ranges, vec![(0, 10), (6, 16)]);
        assert!(truncated);
    }

    #[test]
    fn window_ranges_empty_input_produces_no_windows() {
        let (ranges, truncated) = window_ranges(0, 512, 64, 32);
        assert!(ranges.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn softmax_probability_picks_index() {
        let p = softmax_probability(&[0.0, 2.0], 1);
        assert!((p - 0.8808).abs() < 0.001);
    }
}
