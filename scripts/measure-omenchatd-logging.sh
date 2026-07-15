#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

duration="${OMENCHATD_LOG_SOAK_SECONDS:-60}"
output="${1:-/tmp/omenchatd-log-soak-$(date -u +%Y%m%dT%H%M%SZ)}"
test_name='server_log::tests::bounded_logger_stays_nonblocking_and_retained_under_slow_filesystem_soak'

if [[ "$(uname -s)" != "Linux" || ! -r /proc/self/status || ! -d /proc/self/fd ]]; then
  echo "the omenchatd logging RSS/FD soak currently requires Linux /proc" >&2
  exit 2
fi
if ! [[ "$duration" =~ ^[0-9]+$ ]] || (( duration < 3 || duration > 600 )); then
  echo "OMENCHATD_LOG_SOAK_SECONDS must be an integer from 3 through 600" >&2
  exit 2
fi
for tool in cargo rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing logging measurement tool: $tool" >&2
    exit 2
  fi
done

mkdir -p "$output"
raw="$output/soak.log"
echo "== omenchatd bounded logger slow-writer soak (${duration}s) =="
OMENCHATD_LOG_SOAK_SECONDS="$duration" \
  cargo test --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features "$test_name" -- --exact --ignored --nocapture \
    2>&1 | tee "$raw"

summary="$(rg '^LOG_SOAK_SUMMARY ' "$raw" | tail -n 1)"
if [[ -z "$summary" ]]; then
  echo "logging soak completed without a machine-readable summary" >&2
  exit 1
fi
printf '%s\n' "$summary" > "$output/summary-line.txt"
printf '%s\n' "$summary" | tr ' ' '\n' | tail -n +2 > "$output/summary.txt"

echo "logging soak results: $output"
echo "$summary"
