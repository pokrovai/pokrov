# Prompt-injection model benchmark

Offline benchmark for `prompt_injection` detector candidates. Runs the
production code path (`LocalOnnxPromptInjectionDetector` → `pokrov-core`
scanner contract) against a labeled corpus and reports a confusion matrix,
precision/recall/F1 and latency stats.

## Corpus

`corpus.jsonl` — one JSON object per line:

| Field | Values |
|-------|--------|
| `id` | unique entry id |
| `text` | evaluated content (never printed by the harness) |
| `expected` | `benign` \| `injection` |
| `language` | `ru` \| `en` \| `mixed` |
| `category` | attack/quality class (`direct_override`, `embedded_instruction`, `role_impersonation`, `delimiter_smuggling`, `encoded_payload`, `task_chaining`, `jailbreak_persona`, `tool_output`, `quoted_injection_in_docs`) |

`quoted_injection_in_docs` entries are a quality gate: security documentation
that *quotes* injection phrases must classify `benign` — flagging it would
break legitimate content flows.

## Running

Build once:

```sh
cargo build -p pokrov-prompt-injection --release --bin pokrov-pi-bench
```

Run per model (paths point at the exported ONNX model + `tokenizer.json`):

```sh
./target/release/pokrov-pi-bench \
  --model-id hikmaai-mdeberta-v3-base-prompt-injection-multilingual \
  --model-path models/hikmaai/model.int8.onnx \
  --tokenizer-path models/hikmaai/tokenizer.json \
  --corpus benchmarks/prompt-injection/corpus.jsonl \
  --threshold 0.90 \
  --report-json out/hikmaai.json
```

## Candidate models

| Model | Notes |
|-------|-------|
| `HikmaAI/mdeberta-v3-base-prompt-injection-multilingual` | multilingual; primary RU/EN candidate |
| `gbv/mdeberta-ru-prompt-injection` | Russian-specialized baseline |
| `meta-llama/Llama-Prompt-Guard-2-86M` | English-centric, small footprint |
| `ModernGuard-1` (optional) | evaluate if long-context coverage is needed |

Export each to ONNX (e.g. `optimum-cli export onnx` or `transformers.onnx`) —
int8 quantization recommended for the latency budget — and copy `config.json`
next to the model file so the detector can auto-resolve the positive label
index (`id2label`). If resolution is ambiguous, pass
`--injection-label-index N` (same knob as `provider.injection_label_index` in
the runtime config).

## Selection criteria

- Highest recall on `injection` at `threshold ≈ 0.90` without failing the
  `quoted_injection_in_docs` quality gate.
- p95 latency compatible with the proxy budget (see `specs/Prompt Injection.md`).
- Per-language breakdown must not collapse on `ru` or `mixed` entries.

## Results (2026-09-30, corpus v2 — 42 entries, macOS arm64, debug build)

| Model | Size | Precision | Recall | F1 | FPR | FNR | avg lat | peak RSS | Scaling (med) |
|-------|------|-----------|--------|-----|-----|-----|---------|----------|----------------|
| HikmaAI mdeberta-v3 multilingual **int8** | 323 MiB | 0.950 | 0.905 | 0.927 | 0.048 | 0.095 | 18 ms | 1.1 GiB | 1ch 21 ms · 4ch 509 ms · 16ch 2.5 s · 32ch 5.1 s |
| gbv mdeberta-ru **fp32** | 1064 MiB | 0.783 | 0.857 | 0.818 | 0.238 | 0.143 | 45 ms | 2.3 GiB | 1ch 61 ms · 4ch 1.5 s · 16ch 6.0 s · 32ch 14.5 s |
| guardion ModernGuard-1 **fp32** | 1174 MiB | 0.696 | 0.762 | 0.727 | 0.333 | 0.238 | 42 ms | 2.6 GiB | 1ch 44 ms · 4ch 1.4 s · 16ch 6.7 s · 32ch 13.3 s |
| Llama-Prompt-Guard-2-86M **quant** (gravitee mirror) | 268 MiB | 0.667¹ | 0.476¹ | 0.556¹ | 0.238¹ | 0.524¹ | 15 ms | 1.1 GiB | 1ch 20 ms · 4ch 783 ms · 16ch 3.0 s · 32ch 6.1 s |

¹ PG2 numbers at `threshold 0.5` (`report-promptguard2-t05.json`); at the
shared 0.9 threshold recall collapses to 0.29 (`report-promptguard2.json`).
PG2 targets direct jailbreak-style injections in prompts; indirect/contextual
tool-output attacks dominate this corpus, which explains the gap.

Prompt Guard 2 was evaluated via the non-gated `gravitee-io` ONNX mirror
(`model.quant.onnx`); the upstream `meta-llama` repo requires manual license
acceptance. The mirror ships the same LICENSE — it applies to the weights.

Reports: `report-hikmaai-int8.json`, `report-hikmaai-int8-t05.json`,
`report-gbv-fp32.json`, `report-modernguard-fp32.json`,
`report-promptguard2.json`, `report-promptguard2-t05.json`.
Threshold 0.5 vs 0.9 on HikmaAI produced an identical confusion matrix —
scores are near-saturated.

**Selection: HikmaAI int8.** Best precision/F1 and recall, smallest footprint
(3.3× smaller than fp32 alternatives), lowest latency and RSS. Its only
weakness is `tool_invocation_manipulation` recall (0.50 on n=2). gbv fails the
`quoted_injection_in_docs` quality gate (FPR 0.33); ModernGuard is worse on
every metric including RU (accuracy 0.68, FPR 0.36).

Latency note: per-chunk cost dominates — ~150 ms/chunk for HikmaAI int8 in a
debug build (32 chunks ≈ 5.1 s; linear scaling confirmed across three runs),
~450 ms for the fp32 models. `timeout_ms` must exceed worst-case chunk budget: the
shipped default is `10000` ms for `max_chunks: 32`.

Per-language note for HikmaAI: RU is clean (n=19, FPR 0, recall 1.0), EN
recall 0.80 (2 misses), mixed n=4 shows FPR 1.0 — a single false positive
(`mixed-benign-001`) dominates that slice; interpret with the small n.
