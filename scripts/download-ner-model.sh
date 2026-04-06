#!/usr/bin/env bash
set -euo pipefail

MODEL_NAME="${1:-dslim/bert-base-NER}"
MODEL_DIR="${2:-models/bert-base-NER}"

echo "Downloading NER model: ${MODEL_NAME}"
echo "Output directory: ${MODEL_DIR}"

if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 is required for model conversion"
    exit 1
fi

python3 -c "
from transformers import AutoTokenizer, AutoModelForTokenClassification
import torch, json, os

model_name = '${MODEL_NAME}'
out_dir = '${MODEL_DIR}'
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
echo "Model exported to ${MODEL_DIR}"
ls -la "${MODEL_DIR}"
