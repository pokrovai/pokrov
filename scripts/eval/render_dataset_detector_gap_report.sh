#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
REPORT_DATE="${1:-$(date +%F)}"

cd "$ROOT_DIR"

POKROV_WRITE_DATASET_DETECTOR_GAP_REPORT=1 \
POKROV_DATASET_DETECTOR_GAP_REPORT_DATE="$REPORT_DATE" \
cargo test dataset_detector_gap_report_can_be_regenerated_when_requested \
  --test contract \
  -- --ignored --nocapture
