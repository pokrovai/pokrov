//! `pokrov-pi-bench` — offline benchmark harness for prompt-injection models.
//!
//! Reads a JSONL corpus (`{"id","text","expected","language","category"}` where
//! `expected` is `benign` or `injection`), runs the production ONNX detector
//! path, and reports a confusion matrix, precision/recall/F1 and latency
//! statistics. Output is metadata-only: corpus texts are never printed.
//! Run once per model to compare candidates (see `benchmarks/prompt-injection`).

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use pokrov_core::prompt_injection::PromptInjectionDetector;
use pokrov_prompt_injection::{ChunkingLimits, LocalOnnxPromptInjectionDetector};
use serde::Deserialize;

#[derive(Deserialize)]
struct CorpusEntry {
    id: String,
    text: String,
    expected: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    category: String,
}

struct Args {
    model_id: String,
    model_path: PathBuf,
    tokenizer_path: PathBuf,
    corpus: PathBuf,
    threshold: f32,
    injection_label_index: Option<u32>,
    chunking: ChunkingLimits,
    report_json: Option<PathBuf>,
    /// Chunk-count latency scenarios (spec §22); `None` disables the phase.
    scaling_chunks: Option<Vec<usize>>,
}

#[derive(Default)]
struct Confusion {
    tp: usize,
    fp: usize,
    tn: usize,
    r#fn: usize,
}

impl Confusion {
    fn record(&mut self, expected_injection: bool, predicted_injection: bool) {
        match (expected_injection, predicted_injection) {
            (true, true) => self.tp += 1,
            (false, true) => self.fp += 1,
            (true, false) => self.r#fn += 1,
            (false, false) => self.tn += 1,
        }
    }

    fn precision(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }

    fn recall(&self) -> f64 {
        ratio(self.tp, self.tp + self.r#fn)
    }

    fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    fn accuracy(&self) -> f64 {
        ratio(self.tp + self.tn, self.tp + self.tn + self.fp + self.r#fn)
    }

    /// False-positive rate: benign entries flagged as injection.
    fn fpr(&self) -> f64 {
        ratio(self.fp, self.fp + self.tn)
    }

    /// False-negative rate: injections that passed as benign.
    fn fnr(&self) -> f64 {
        ratio(self.r#fn, self.tp + self.r#fn)
    }
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p / 100.0).ceil() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} --model-id ID --model-path FILE --tokenizer-path FILE --corpus FILE.jsonl \\\n\
         \x20       [--threshold 0.9] [--injection-label-index N] [--max-tokens 512] \\\n\
         \x20       [--overlap-tokens 64] [--max-chunks 32] [--report-json OUT.json] \\\n\
         \x20       [--scaling-chunks 1,4,16,32 | none]"
    )
}

fn parse_args() -> Result<Args, String> {
    let mut model_id = None;
    let mut model_path = None;
    let mut tokenizer_path = None;
    let mut corpus = None;
    let mut threshold = 0.9f32;
    let mut injection_label_index = None;
    let mut chunking = ChunkingLimits::default();
    let mut report_json = None;
    let mut scaling_chunks = Some(vec![1usize, 4, 16, 32]);

    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut take = |flag: &str| -> Result<String, String> {
            it.next().ok_or_else(|| format!("missing value for {flag}"))
        };
        match arg.as_str() {
            "--model-id" => model_id = Some(take(&arg)?),
            "--model-path" => model_path = Some(PathBuf::from(take(&arg)?)),
            "--tokenizer-path" => tokenizer_path = Some(PathBuf::from(take(&arg)?)),
            "--corpus" => corpus = Some(PathBuf::from(take(&arg)?)),
            "--threshold" => {
                threshold = take(&arg)?.parse().map_err(|_| "invalid --threshold".to_string())?
            }
            "--injection-label-index" => {
                injection_label_index =
                    Some(take(&arg)?.parse().map_err(|_| "invalid index".to_string())?)
            }
            "--max-tokens" => {
                chunking.max_tokens =
                    take(&arg)?.parse().map_err(|_| "invalid --max-tokens".to_string())?
            }
            "--overlap-tokens" => {
                chunking.overlap_tokens =
                    take(&arg)?.parse().map_err(|_| "invalid --overlap-tokens".to_string())?
            }
            "--max-chunks" => {
                chunking.max_chunks =
                    take(&arg)?.parse().map_err(|_| "invalid --max-chunks".to_string())?
            }
            "--report-json" => report_json = Some(PathBuf::from(take(&arg)?)),
            "--scaling-chunks" => {
                let value = take(&arg)?;
                scaling_chunks = if value == "none" {
                    None
                } else {
                    Some(
                        value
                            .split(',')
                            .map(|part| {
                                part.trim().parse::<usize>().map_err(|_| {
                                    "invalid --scaling-chunks list".to_string()
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                };
            }
            "--help" | "-h" => return Err(usage("pokrov-pi-bench")),
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let missing =
        |name: &str| -> String { format!("missing required {name}\n{}", usage("pokrov-pi-bench")) };
    Ok(Args {
        model_id: model_id.ok_or_else(|| missing("--model-id"))?,
        model_path: model_path.ok_or_else(|| missing("--model-path"))?,
        tokenizer_path: tokenizer_path.ok_or_else(|| missing("--tokenizer-path"))?,
        corpus: corpus.ok_or_else(|| missing("--corpus"))?,
        threshold,
        injection_label_index,
        chunking,
        report_json,
        scaling_chunks,
    })
}

/// Process CPU seconds (user + system) via getrusage; 0 on non-unix.
fn process_cpu_secs() -> f64 {
    #[cfg(unix)]
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return 0.0;
        }
        (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as f64
            + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1e6
    }
    #[cfg(not(unix))]
    0.0
}

/// Peak RSS in bytes. `ru_maxrss` is bytes on macOS, kilobytes on Linux.
fn peak_rss_bytes() -> u64 {
    #[cfg(unix)]
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return 0;
        }
        #[cfg(target_os = "macos")]
        return usage.ru_maxrss as u64;
        #[cfg(not(target_os = "macos"))]
        return (usage.ru_maxrss as u64) * 1024;
    }
    #[cfg(not(unix))]
    0
}

/// Grows `base` (repeated) until the detector processes at least
/// `target_chunks` windows, probing via `chunks_processed`. Returns the text
/// and the actual chunk count, or `None` when the target is unreachable
/// within the sanity cap (e.g. `target > max_chunks`).
fn text_for_chunks(
    detector: &LocalOnnxPromptInjectionDetector,
    base: &str,
    target_chunks: usize,
) -> Option<(String, u32)> {
    if base.is_empty() || target_chunks == 0 {
        return None;
    }
    // Bracket then binary-search the repeat count: window count grows
    // monotonically in repeats, so O(log n) `count_windows` probes suffice.
    // `Err` (ContentTruncated) means the probe overshot the chunk budget.
    let probe = |repeats: usize| -> Option<u32> {
        let text = base.repeat(repeats);
        detector.count_windows(&text).ok()
    };
    match probe(1) {
        Some(count) if count >= target_chunks as u32 => {
            return Some((base.to_string(), count))
        }
        Some(_) => {}
        None => return None,
    }
    let mut lo = 1usize; // below target
    // First repeat count that reached or overshot the target band.
    let mut hi;
    let mut repeats = 2usize;
    loop {
        match probe(repeats) {
            Some(count) if count >= target_chunks as u32 => {
                hi = repeats;
                break;
            }
            Some(_) => lo = repeats,
            None => {
                hi = repeats;
                break;
            }
        }
        repeats = repeats.saturating_mul(2);
        if repeats > 8192 {
            return None;
        }
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        match probe(mid) {
            Some(count) if count >= target_chunks as u32 => hi = mid,
            Some(_) => lo = mid,
            None => hi = mid,
        }
    }
    // `hi` is the smallest overshot-or-hit repeat count; the text is usable
    // only when it still fits the chunk budget.
    let text = base.repeat(hi);
    match detector.count_windows(&text) {
        Ok(count) if count >= target_chunks as u32 => Some((text, count)),
        _ => None,
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let detector = match LocalOnnxPromptInjectionDetector::load(
        &args.model_id,
        &args.model_path,
        &args.tokenizer_path,
        args.injection_label_index,
        args.chunking,
    ) {
        Ok(detector) => detector,
        Err(e) => {
            eprintln!("detector load failed: {e}");
            return ExitCode::from(2);
        }
    };

    let corpus_text = match std::fs::read_to_string(&args.corpus) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("cannot read corpus {}: {e}", args.corpus.display());
            return ExitCode::from(2);
        }
    };

    let mut entries = Vec::new();
    for (line_no, line) in corpus_text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<CorpusEntry>(line) {
            Ok(entry) if matches!(entry.expected.as_str(), "benign" | "injection") => {
                entries.push(entry)
            }
            Ok(entry) => {
                eprintln!("{}:{} invalid expected value '{}'", args.corpus.display(), line_no + 1, entry.expected);
                return ExitCode::from(2);
            }
            Err(e) => {
                eprintln!("{}:{} invalid JSONL: {e}", args.corpus.display(), line_no + 1);
                return ExitCode::from(2);
            }
        }
    }
    if entries.is_empty() {
        eprintln!("corpus is empty: {}", args.corpus.display());
        return ExitCode::from(2);
    }

    let mut confusion = Confusion::default();
    let mut errors = 0usize;
    let mut latencies = Vec::with_capacity(entries.len());
    let mut per_language: BTreeMap<String, Confusion> = BTreeMap::new();
    let mut per_category: BTreeMap<String, Confusion> = BTreeMap::new();

    let cpu_start = process_cpu_secs();
    let wall_start = Instant::now();

    for entry in &entries {
        let started = Instant::now();
        let outcome = detector.detect(&entry.text);
        latencies.push(started.elapsed().as_secs_f64() * 1_000.0);
        let score = match outcome {
            Ok(detection) => detection.score,
            Err(e) => {
                errors += 1;
                eprintln!("entry {} detection error: {e}", entry.id);
                continue;
            }
        };
        let predicted = score >= args.threshold;
        let expected = entry.expected == "injection";
        confusion.record(expected, predicted);
        per_language
            .entry(if entry.language.is_empty() { "unknown".to_string() } else { entry.language.clone() })
            .or_default()
            .record(expected, predicted);
        per_category
            .entry(if entry.category.is_empty() { "unknown".to_string() } else { entry.category.clone() })
            .or_default()
            .record(expected, predicted);
    }

    let wall_secs = wall_start.elapsed().as_secs_f64();
    let cpu_secs = process_cpu_secs() - cpu_start;
    let model_size_bytes =
        std::fs::metadata(&args.model_path).map(|m| m.len()).unwrap_or(0);
    let peak_rss_mib = peak_rss_bytes() as f64 / (1024.0 * 1024.0);
    let throughput_eps = entries.len() as f64 / wall_secs.max(f64::EPSILON);
    let cpu_util_pct = cpu_secs / wall_secs.max(f64::EPSILON) * 100.0;

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let avg_latency = latencies.iter().sum::<f64>() / latencies.len().max(1) as f64;

    println!("model: {}", args.model_id);
    println!("entries: {} (errors: {})", entries.len(), errors);
    println!("threshold: {}", args.threshold);
    println!(
        "confusion: tp={} fp={} tn={} fn={}",
        confusion.tp, confusion.fp, confusion.tn, confusion.r#fn
    );
    println!(
        "precision={:.4} recall={:.4} f1={:.4} accuracy={:.4} fpr={:.4} fnr={:.4}",
        confusion.precision(),
        confusion.recall(),
        confusion.f1(),
        confusion.accuracy(),
        confusion.fpr(),
        confusion.fnr()
    );
    println!(
        "latency_ms: avg={:.1} p50={:.1} p95={:.1} max={:.1}",
        avg_latency,
        percentile(&latencies, 50.0),
        percentile(&latencies, 95.0),
        latencies.last().copied().unwrap_or(0.0)
    );
    for (language, stats) in &per_language {
        println!(
            "language={language}: n={} accuracy={:.4} recall={:.4} fpr={:.4} fnr={:.4}",
            stats.tp + stats.tn + stats.fp + stats.r#fn,
            stats.accuracy(),
            stats.recall(),
            stats.fpr(),
            stats.fnr()
        );
    }
    for (category, stats) in &per_category {
        println!(
            "category={category}: n={} accuracy={:.4} recall={:.4} fpr={:.4} fnr={:.4}",
            stats.tp + stats.tn + stats.fp + stats.r#fn,
            stats.accuracy(),
            stats.recall(),
            stats.fpr(),
            stats.fnr()
        );
    }
    println!(
        "resources: model_size_mib={:.1} peak_rss_mib={:.1} cpu_util_pct={:.1} throughput_eps={:.1}",
        model_size_bytes as f64 / (1024.0 * 1024.0),
        peak_rss_mib,
        cpu_util_pct,
        throughput_eps
    );

    // Per-chunk-count latency scenarios (spec §22): grow the longest benign
    // entry until the detector reports at least the target window count.
    let mut scaling_report = Vec::new();
    if let Some(targets) = &args.scaling_chunks {
        let base = entries
            .iter()
            .filter(|entry| entry.expected == "benign")
            .max_by_key(|entry| entry.text.len())
            .map(|entry| entry.text.clone())
            .unwrap_or_default();
        for &target in targets {
            match text_for_chunks(&detector, &base, target) {
                Some((text, actual_chunks)) => {
                    let mut times = Vec::new();
                    for _ in 0..3 {
                        let started = Instant::now();
                        let _ = detector.detect(&text);
                        times.push(started.elapsed().as_secs_f64() * 1_000.0);
                    }
                    times.sort_by(|a, b| {
                        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let median = times[times.len() / 2];
                    println!(
                        "scaling: target_chunks={target} actual_chunks={actual_chunks} median_ms={median:.1}"
                    );
                    scaling_report.push(serde_json::json!({
                        "target_chunks": target,
                        "actual_chunks": actual_chunks,
                        "median_ms": median,
                    }));
                }
                None => {
                    println!("scaling: target_chunks={target} unreachable");
                    scaling_report.push(serde_json::json!({
                        "target_chunks": target,
                        "error": "unreachable",
                    }));
                }
            }
        }
    }

    if let Some(path) = &args.report_json {
        let report = serde_json::json!({
            "model_id": args.model_id,
            "threshold": args.threshold,
            "entries": entries.len(),
            "errors": errors,
            "confusion": {
                "tp": confusion.tp, "fp": confusion.fp,
                "tn": confusion.tn, "fn": confusion.r#fn,
            },
            "precision": confusion.precision(),
            "recall": confusion.recall(),
            "f1": confusion.f1(),
            "accuracy": confusion.accuracy(),
            "fpr": confusion.fpr(),
            "fnr": confusion.fnr(),
            "latency_ms": {
                "avg": avg_latency,
                "p50": percentile(&latencies, 50.0),
                "p95": percentile(&latencies, 95.0),
                "max": latencies.last().copied().unwrap_or(0.0),
            },
            "per_language": per_language.iter().map(|(language, stats)| {
                (language.clone(), serde_json::json!({
                    "n": stats.tp + stats.tn + stats.fp + stats.r#fn,
                    "accuracy": stats.accuracy(),
                    "recall": stats.recall(),
                    "fpr": stats.fpr(),
                    "fnr": stats.fnr(),
                }))
            }).collect::<serde_json::Map<String, serde_json::Value>>(),
            "per_category": per_category.iter().map(|(category, stats)| {
                (category.clone(), serde_json::json!({
                    "n": stats.tp + stats.tn + stats.fp + stats.r#fn,
                    "accuracy": stats.accuracy(),
                    "recall": stats.recall(),
                    "fpr": stats.fpr(),
                    "fnr": stats.fnr(),
                }))
            }).collect::<serde_json::Map<String, serde_json::Value>>(),
            "resources": {
                "model_size_bytes": model_size_bytes,
                "peak_rss_mib": peak_rss_mib,
                "cpu_util_pct": cpu_util_pct,
                "throughput_entries_per_sec": throughput_eps,
            },
            "scaling": scaling_report,
        });
        match std::fs::write(path, serde_json::to_string_pretty(&report).unwrap_or_default()) {
            Ok(()) => println!("report written: {}", path.display()),
            Err(e) => {
                eprintln!("cannot write report {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    }

    // Non-zero exit when any entry could not be evaluated, so CI/comparison
    // runs do not silently consume partial results.
    if errors > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
