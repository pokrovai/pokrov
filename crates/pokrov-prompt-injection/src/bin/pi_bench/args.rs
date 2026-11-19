//! Command-line parsing for `pokrov-pi-bench`.

use std::env;
use std::path::PathBuf;

use pokrov_prompt_injection::ChunkingLimits;

use super::Args;

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} --model-id ID --model-path FILE --tokenizer-path FILE --corpus FILE.jsonl \\\n\
         \x20       [--threshold 0.9] [--injection-label-index N] [--max-tokens 512] \\\n\
         \x20       [--overlap-tokens 64] [--max-chunks 32] [--report-json OUT.json] \\\n\
         \x20       [--scaling-chunks 1,4,16,32 | none]"
    )
}

pub(super) fn parse_args() -> Result<Args, String> {
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
                                part.trim()
                                    .parse::<usize>()
                                    .map_err(|_| "invalid --scaling-chunks list".to_string())
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
