#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "cache index measurement: isolated temporary fixtures"
echo "host=$(uname -srm)"
echo "rustc=$(rustc -V)"

CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo test --locked --release --no-default-features --features desktop-product \
  measure_cache_index_latency -- --ignored --nocapture
