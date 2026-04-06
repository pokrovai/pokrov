#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: download-ner-model.sh [MODEL_ID [OUTPUT_DIR]]

Download a single Hugging Face NER model and export it to ONNX format.

Options:
  --all    Download all recommended models (EN + RU)

Examples:
  # Download a single model
  ./scripts/download-ner-model.sh dslim/bert-base-NER models/bert-base-NER

  # Download all recommended models
  ./scripts/download-ner-model.sh --all

Recommended models:
  EN: dslim/bert-base-NER   -> models/bert-base-NER/
  RU: cointegrated/rubert-tiny2-ner -> models/ner-rubert-tiny-news/

Dependencies: python3, torch, transformers, optimum
EOF
    exit "${1:-0}"
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage 0
fi

export_model() {
    local model_name="$1"
    local out_dir="$2"

    echo "Downloading NER model: ${model_name}"
    echo "Output directory: ${out_dir}"

    if ! command -v python3 &>/dev/null; then
        echo "ERROR: python3 is required for model conversion"
        exit 1
    fi

    python3 -c "
from transformers import AutoTokenizer, AutoModelForTokenClassification
import torch, json, os

model_name = '${model_name}'
out_dir = '${out_dir}'
os.makedirs(out_dir, exist_ok=True)

print(f'Loading tokenizer and model: {model_name}')
tokenizer = AutoTokenizer.from_pretrained(model_name)
model = AutoModelForTokenClassification.from_pretrained(model_name)

tokenizer.save_pretrained(out_dir)

dummy = tokenizer('Hello world', return_tensors='pt')
onnx_path = os.path.join(out_dir, 'model.onnx')

has_token_type_ids = 'token_type_ids' in dummy
print(f'Model uses token_type_ids: {has_token_type_ids}')

if has_token_type_ids:
    torch.onnx.export(
        model,
        (dummy['input_ids'], dummy['attention_mask'], dummy['token_type_ids']),
        onnx_path,
        input_names=['input_ids', 'attention_mask', 'token_type_ids'],
        output_names=['logits'],
        dynamic_axes={
            'input_ids': {0: 'batch', 1: 'seq'},
            'attention_mask': {0: 'batch', 1: 'seq'},
            'token_type_ids': {0: 'batch', 1: 'seq'},
            'logits': {0: 'batch', 1: 'seq'},
        },
        opset_version=14,
    )
else:
    torch.onnx.export(
        model,
        (dummy['input_ids'], dummy['attention_mask']),
        onnx_path,
        input_names=['input_ids', 'attention_mask'],
        output_names=['logits'],
        dynamic_axes={
            'input_ids': {0: 'batch', 1: 'seq'},
            'attention_mask': {0: 'batch', 1: 'seq'},
            'logits': {0: 'batch', 1: 'seq'},
        },
        opset_version=14,
    )

config = {
    'id2label': {str(k): v for k, v in model.config.id2label.items()},
    'num_labels': model.config.num_labels,
}
with open(os.path.join(out_dir, 'config.json'), 'w') as f:
    json.dump(config, f, indent=2)

print(f'Done. Labels: {model.config.id2label}')
print(f'Files saved to {out_dir}/')
for name in sorted(os.listdir(out_dir)):
    size = os.path.getsize(os.path.join(out_dir, name))
    print(f'  {name} ({size:,} bytes)')
"

    echo ""
    echo "Model exported to ${out_dir}"
    ls -la "${out_dir}"
}

if [[ "${1:-}" == "--all" ]]; then
    echo "=== Downloading all recommended NER models ==="
    echo ""
    export_model "dslim/bert-base-NER" "models/bert-base-NER"
    echo ""
    echo "============================================="
    echo ""
    export_model "cointegrated/rubert-tiny2-ner" "models/ner-rubert-tiny-news"
    echo ""
    echo "=== All models downloaded ==="
    exit 0
fi

MODEL_NAME="${1:-dslim/bert-base-NER}"
MODEL_DIR="${2:-models/$(basename "$MODEL_NAME" | tr '/' '-')}"
export_model "$MODEL_NAME" "$MODEL_DIR"
