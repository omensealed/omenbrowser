#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "runtime thread measurement: isolated synthetic async and bounded-file workload"
echo "host=$(uname -srm)"
echo "rustc=$(rustc -V)"

command=(cargo test --locked --release --test runtime_thread_measurement
  measure_runtime_thread_policies -- --ignored --nocapture)

if [[ "${1:-}" == "--two-core" ]]; then
  if ! command -v taskset >/dev/null 2>&1; then
    echo "error: --two-core requires taskset (util-linux)" >&2
    exit 2
  fi
  echo "cpu_affinity=0,1"
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" taskset -c 0,1 "${command[@]}"
else
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" "${command[@]}"
fi
