#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
CACHE_DIR="$ROOT_DIR/tests/fixtures/eval/datasets/open-cache"
ROWS_LENGTH="${HF_ROWS_LENGTH:-25}"
USER_AGENT="pokrov-open-dataset-pipeline/1.0"

mkdir -p "$CACHE_DIR"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

require_cmd curl
require_cmd python3

hf_snapshot() {
  local source_id="$1"
  local dataset="$2"
  local tmp_splits tmp_rows out_path

  tmp_splits="$(mktemp)"
  tmp_rows="$(mktemp)"
  out_path="$CACHE_DIR/${source_id}.json"

  curl -fsSL \
    -A "$USER_AGENT" \
    --get \
    --data-urlencode "dataset=${dataset}" \
    "https://datasets-server.huggingface.co/splits" \
    > "$tmp_splits"

  local config split parsed
  parsed="$(python3 - "$tmp_splits" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as f:
    payload = json.load(f)

splits = payload.get("splits") or []
if not splits:
    raise SystemExit("no splits in response")
first = splits[0]
print(first.get("config") or "")
print(first.get("split") or "")
PY
)"
  config="$(printf '%s\n' "$parsed" | sed -n '1p')"
  split="$(printf '%s\n' "$parsed" | sed -n '2p')"

  if [[ -z "$config" || -z "$split" ]]; then
    echo "failed to parse split info for dataset ${dataset}" >&2
    exit 1
  fi

  curl -fsSL \
    -A "$USER_AGENT" \
    --get \
    --data-urlencode "dataset=${dataset}" \
    --data-urlencode "config=${config}" \
    --data-urlencode "split=${split}" \
    --data-urlencode "offset=0" \
    --data-urlencode "length=${ROWS_LENGTH}" \
    "https://datasets-server.huggingface.co/rows" \
    > "$tmp_rows"

  python3 - "$source_id" "$dataset" "$config" "$split" "$tmp_rows" "$out_path" <<'PY'
import json
import sys

source_id = sys.argv[1]
dataset = sys.argv[2]
config = sys.argv[3]
split = sys.argv[4]
rows_path = sys.argv[5]
out_path = sys.argv[6]

with open(rows_path, "r", encoding="utf-8") as f:
    rows_payload = json.load(f)

snapshot = {
    "source_id": source_id,
    "source_kind": "huggingface_dataset",
    "dataset": dataset,
    "config": config,
    "split": split,
    "rows": rows_payload.get("rows", []),
}

with open(out_path, "w", encoding="utf-8") as f:
    json.dump(snapshot, f, ensure_ascii=False, indent=2)
PY

  rm -f "$tmp_splits" "$tmp_rows"
  echo "wrote $out_path"
}

github_repo_snapshot() {
  local source_id="$1"
  local repository="$2"
  local out_path="$CACHE_DIR/${source_id}.json"
  local tmp_repo

  tmp_repo="$(mktemp)"
  if curl -fsSL \
    -A "$USER_AGENT" \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${repository}" \
    > "$tmp_repo"; then
    if [[ -s "$tmp_repo" ]]; then
      python3 - "$source_id" "$repository" "$out_path" "$tmp_repo" <<'PY'
import json
import sys

source_id = sys.argv[1]
repository = sys.argv[2]
out_path = sys.argv[3]
repo_path = sys.argv[4]
with open(repo_path, "r", encoding="utf-8") as f:
    repo_payload = json.load(f)

snapshot = {
    "source_id": source_id,
    "source_kind": "github_repository",
    "repository": repository,
    "default_branch": repo_payload.get("default_branch"),
    "html_url": repo_payload.get("html_url"),
    "description": repo_payload.get("description"),
}

with open(out_path, "w", encoding="utf-8") as f:
    json.dump(snapshot, f, ensure_ascii=False, indent=2)
PY
      rm -f "$tmp_repo"
      echo "wrote $out_path"
      return 0
    fi
  fi

  python3 - "$source_id" "$repository" "$out_path" <<'PY'
import json
import sys

source_id = sys.argv[1]
repository = sys.argv[2]
out_path = sys.argv[3]

snapshot = {
    "source_id": source_id,
    "source_kind": "github_repository",
    "repository": repository,
    "default_branch": None,
    "html_url": f"https://github.com/{repository}",
    "description": "metadata fetch degraded; fallback snapshot",
}

with open(out_path, "w", encoding="utf-8") as f:
    json.dump(snapshot, f, ensure_ascii=False, indent=2)
PY
  rm -f "$tmp_repo"
  echo "wrote $out_path"
}

hf_snapshot "open_ai4privacy_pii_masking_200k" "ai4privacy/pii-masking-200k"
hf_snapshot "open_nvidia_nemotron_pii" "nvidia/Nemotron-PII"
hf_snapshot "open_gretel_pii_masking_en_v1" "gretelai/gretel-pii-masking-en-v1"
github_repo_snapshot "open_presidio_research_repo" "microsoft/presidio-research"

echo "open dataset snapshots are available in $CACHE_DIR"
